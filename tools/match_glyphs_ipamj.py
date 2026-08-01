#!/usr/bin/env python3
"""外字注記辞書の字形（輪郭）を IPAmj明朝のグリフと重ね合わせて MJ 文字図形を特定する。

    pip install fonttools pillow
    python3 tools/match_glyphs_ipamj.py \
        crates/aozora-core/data ipamjm.ttf mji.tsv > gaiji_chuki_mj.tsv

**任意の工程**。IPAmj明朝はライセンス同意の先にあり、MJ 文字情報一覧表も別途入手が要る
ので、抽出器本体とは分けてある。対象は `glyph=1` の 222 件——埋め込みサブセット
フォントで描かれていて CID が外から解釈できず、輪郭しか手がかりが無いもの。

候補は代用字（`→［包摂適用 X］` の X）から作る。X の異体字が MJ に何通りか載っている
ので、それぞれを IPAmj明朝から描いて、辞書の字形と一番よく重なるものを選ぶ。

較正: 正解と分かっているペアの IoU は中央値 0.61、無関係なペアは 0.22 で分離する。
IoU>=0.40 かつ 2 位と 0.05 以上の差があるものを「確信できる一致」とした（139 件、
IoU 中央値 0.95）。

MJ 文字情報一覧表: https://moji.or.jp/mojikiban/mjlist/ の mji.*.xlsx を
  id / 対応するUCS / 実装したUCS / 実装したMoji_JohoコレクションIVS の 4 列を持つ
  TSV（mj, ucs, ucs_impl, ivs）にしたもの。
"""
import collections
import csv
import re
import sys

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont
from PIL import Image, ImageChops, ImageDraw

N = 96          # 比較に使う正方形の一辺
MIN_IOU = 0.40  # 確信できる一致の下限
MIN_MARGIN = 0.05


class PolyPen(BasePen):
    """輪郭を閉じた折れ線の列にする（曲線は等分割で近似）。"""

    def __init__(self, glyph_set):
        super().__init__(glyph_set)
        self.polys, self.cur = [], []

    def _moveTo(self, p):
        self._closePath()
        self.cur = [p]

    def _lineTo(self, p):
        self.cur.append(p)

    def _curveToOne(self, a, b, c):
        p0 = self.cur[-1]
        for i in range(1, 9):
            t = i / 8
            self.cur.append(tuple((1 - t)**3 * p0[j] + 3 * (1 - t)**2 * t * a[j]
                                  + 3 * (1 - t) * t * t * b[j] + t**3 * c[j] for j in (0, 1)))

    def _qCurveToOne(self, a, b):
        p0 = self.cur[-1]
        for i in range(1, 7):
            t = i / 6
            self.cur.append(tuple((1 - t)**2 * p0[j] + 2 * (1 - t) * t * a[j] + t * t * b[j]
                                  for j in (0, 1)))

    def _closePath(self):
        if len(self.cur) > 2:
            self.polys.append(self.cur)
        self.cur = []

    _endPath = _closePath


def svg_polys(d):
    """SVG のパスデータを折れ線の列にする。M/L/C/Z だけ扱う（cairo の出力はこれで足りる）。"""
    cmds = []
    for c, n in re.findall(r'([MLCZmlcz])|(-?\d+\.?\d*(?:e-?\d+)?)', d):
        if c:
            cmds.append([c, []])
        elif cmds:
            cmds[-1][1].append(float(n))
    polys, cur, pos = [], [], (0.0, 0.0)

    def close():
        nonlocal cur
        if len(cur) > 2:
            polys.append(cur)
        cur = []

    for c, v in cmds:
        if c in 'Mm':
            close()
            cur = [(v[0], v[1])]
            pos = cur[0]
            for i in range(2, len(v), 2):
                cur.append((v[i], v[i + 1]))
                pos = cur[-1]
        elif c in 'Ll':
            for i in range(0, len(v), 2):
                cur.append((v[i], v[i + 1]))
                pos = cur[-1]
        elif c in 'Cc':
            for i in range(0, len(v), 6):
                p0 = pos
                for k in range(1, 9):
                    t = k / 8
                    pos = tuple((1 - t)**3 * p0[j] + 3 * (1 - t)**2 * t * v[i + j]
                                + 3 * (1 - t) * t * t * v[i + 2 + j] + t**3 * v[i + 4 + j]
                                for j in (0, 1))
                    cur.append(pos)
        elif c in 'Zz':
            close()
    close()
    return polys


def raster(polys, flip_y):
    """外接矩形で正規化してから N×N に描く。偶奇規則（XOR）で塗るので中抜きも残る。"""
    xs = [p[0] for q in polys for p in q]
    ys = [p[1] for q in polys for p in q]
    if not xs:
        return None
    w, h = max(xs) - min(xs), max(ys) - min(ys)
    if w <= 0 or h <= 0:
        return None
    s = (N - 8) / max(w, h)
    ox, oy = (N - w * s) / 2, (N - h * s) / 2
    img = Image.new('1', (N, N), 0)
    for q in polys:
        layer = Image.new('1', (N, N), 0)
        pts = [((x - min(xs)) * s + ox,
                (N - ((y - min(ys)) * s + oy)) if flip_y else (y - min(ys)) * s + oy)
               for x, y in q]
        ImageDraw.Draw(layer).polygon(pts, fill=1)
        img = ImageChops.logical_xor(img, layer)
    return img


def iou(a, b):
    inter = sum(ImageChops.logical_and(a, b).point(lambda v: v and 1).getdata())
    union = sum(ImageChops.logical_or(a, b).point(lambda v: v and 1).getdata())
    return inter / union if union else 0.0


def main(datadir, font_path, mji_tsv):
    font = TTFont(font_path)
    glyph_set = font.getGlyphSet()
    uvs = next(t for t in font['cmap'].tables if t.format == 14).uvsDict
    ivs_to_glyph = {(base, sel): name for sel, pairs in uvs.items() for base, name in pairs}
    cmap = font.getBestCmap()

    def font_polys(ivs):
        base, sel = (int(x, 16) for x in ivs.split('_'))
        name = ivs_to_glyph.get((base, sel)) or cmap.get(base)
        if not name:
            return None
        pen = PolyPen(glyph_set)
        glyph_set[name].draw(pen)
        pen._closePath()
        return pen.polys or None

    by_ucs = collections.defaultdict(list)
    for m in csv.DictReader(open(mji_tsv, encoding='utf-8'), delimiter='\t'):
        u = m['ucs'] or m['ucs_impl']
        if u.startswith('U+'):
            try:
                by_ucs[chr(int(u[2:], 16))].append(m)
            except ValueError:
                pass

    outlines = collections.defaultdict(list)
    for g in csv.DictReader(open(f'{datadir}/gaiji_chuki_glyphs.tsv', encoding='utf-8'),
                            delimiter='\t'):
        outlines[g['id']].append((float(g['dx']), g['d']))

    print('\t'.join(['id', 'desc', 'sub', 'mj', 'ivs', 'iou', 'runner_up', 'candidates']))
    for r in csv.DictReader(open(f'{datadir}/gaiji_chuki.tsv', encoding='utf-8'), delimiter='\t'):
        if not r['glyph'] or len(r['sub']) != 1:
            continue
        polys = [[(x + dx, y) for x, y in q] for dx, d in outlines[r['id']] for q in svg_polys(d)]
        a = raster(polys, False) if polys else None
        if a is None:
            continue
        scored = []
        for m in by_ucs.get(r['sub'], []):
            p = font_polys(m['ivs']) if m['ivs'] else None
            if p is None and (m['ucs'] or '').startswith('U+'):
                p = font_polys(m['ucs'][2:] + '_E0100')
            b = raster(p, True) if p else None
            if b is not None:
                scored.append((iou(a, b), m))
        if not scored:
            continue
        scored.sort(key=lambda t: -t[0])
        best = scored[0]
        second = scored[1][0] if len(scored) > 1 else 0.0
        if best[0] < MIN_IOU or best[0] - second < MIN_MARGIN:
            continue
        print('\t'.join([r['id'], r['desc'], r['sub'], best[1]['mj'], best[1]['ivs'],
                         f'{best[0]:.3f}', f'{second:.3f}', str(len(scored))]))


if __name__ == '__main__':
    main(*sys.argv[1:4])
