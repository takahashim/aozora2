#!/usr/bin/env ruby
# frozen_string_literal: true

# `ivs` / `cid` を目視で確かめるための一覧を組む。
#
#   ruby tools/gaiji_chuki_ivs_sheet.rb [出力先.html]   # 既定は tmp/gaiji_chuki_ivs.html
#
# この 2 列だけは PDF と機械的に突き合わせる手が無い（crates/aozora-core/data/gaiji_chuki.md
# の「裏の取れていないもの」）。content stream の CID をこちらで読んで IVD で異体字セレクタ
# 列に落としたもので、独立な情報源が存在しない。
#
# **見るのは「その字形が代用字の異体になっているか」。** 危ないのは値そのものより結び付け
# ——CID を隣のエントリのものと取り違えていれば、字形は代用字と無関係な字になる。並べれば
# 一目で分かる。値が 1 つずれているだけの取り違えは、これでも見つからない。
#
#   [代用字]  [字形]     ← 同じ字の書き分けに見えるか
#     併       倂
#
# 字形の出し方は 2 通り。IVS があればブラウザに描かせる（macOS の Hiragino は
# Adobe-Japan1 の IVS を持っている。対応していないフォントだと代用字と同じ形に見えるので、
# その場合はこの一覧では判断できない）。IVS が引けなかった 224 件は PDF から取った輪郭を
# そのまま置く——こちらは辞書が実際に描いている形そのもの。

require 'json'

DATA = File.expand_path('../crates/aozora-core/data', __dir__)
OUT = ARGV[0] || File.expand_path('../tmp/gaiji_chuki_ivs.html', __dir__)

def read_tsv(path)
  rows = File.readlines(path, chomp: true).map { |l| l.split("\t", -1) }
  head = rows.shift
  rows.map { |r| head.zip(r).to_h }
end

def h(str)
  str.to_s.gsub('&', '&amp;').gsub('<', '&lt;').gsub('>', '&gt;').gsub('"', '&quot;')
end

entries = read_tsv(File.join(DATA, 'gaiji_chuki.tsv'))
outlines = read_tsv(File.join(DATA, 'gaiji_chuki_glyphs.tsv')).group_by { |g| g['id'] }
jis2ucs = JSON.parse(File.read(File.join(DATA, 'jis2ucs.json')))

# 注記になっている代用字から字を取り出す。`※［＃「七／（七＋七）」、第3水準1-14-3］` は 㐂。
def sub_char(note, jis2ucs)
  if (m = note.match(/第[34]水準([0-9]+-[0-9]+-[0-9]+)/))
    men, ku, ten = m[1].split('-').map(&:to_i)
    ent = jis2ucs[format('%d-%02d-%02d', men, ku, ten)]
    return ent.scan(/&#x([0-9A-Fa-f]+);/).flatten.map { |x| x.to_i(16) }.pack('U*') if ent
  end
  return [Regexp.last_match(1).to_i(16)].pack('U') if note =~ /U\+([0-9A-F]{4,6})/

  nil
end

# build_gaiji_chuki_html.rb と同じ組み立て。あちらは表示、こちらは検分に使う。
# SVG のパスから座標を拾って外接矩形を出す。M/L/C はすべて絶対座標で、C だけ 3 点。
def path_extent(d, dx)
  xs = []
  ys = []
  d.scan(/[MLC]([^MLCZ]*)/) do
    nums = Regexp.last_match(1).scan(/-?\d*\.?\d+/).map(&:to_f)
    nums.each_slice(2) { |x, y| next if y.nil?; xs << x + dx; ys << y }
  end
  xs.empty? ? nil : [xs.min, ys.min, xs.max, ys.max]
end

# 輪郭の座標系での全角 1 文字ぶん。実測の外接矩形から。
GLYPH_EM = 9.2

# poppler が描けなかったパーツは輪郭を持たない（`d` が空で `ivs` と `box` がある）。
# `box` は poppler が実際に置いた枠——字は間違っていても位置と大きさは正しいので、
# そこへ字を流し込む。辞書はパーツを横 0.7 倍などに潰して組むので、これが要る。
def part_box(p)
  b = p[%q{box}].to_s.split(%q{ }).map(&:to_f)
  b.size == 4 ? b : nil
end

def part_extent(p)
  # マスクは字より大きいので外接矩形には数えない。塗るだけ。
  return nil if p[%q{fill}].to_s.delete(%q{ }) == %q{rgb(100%,100%,100%)}
  # パスも dy だけ下がる。ここを忘れると viewBox が足りず上が切れる。
  if p[%q{ivs}].to_s.empty?
    e = path_extent(p[%q{d}], p[%q{dx}].to_f) or return nil
    dy0 = p[%q{dy}].to_f
    return [e[0], e[1] + dy0, e[2], e[3] + dy0]
  end


  b = part_box(p) or return nil
  dx = p[%q{dx}].to_f
  dy = p[%q{dy}].to_f
  [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy]
end

def part_svg(p, fill)
  # マスクは背景色で塗る。色は CSS 側に任せる（地の色は表示の文脈で決まる）。
  if p[%q{fill}].to_s.delete(%q{ }) == %q{rgb(100%,100%,100%)}
    return %(<path class="mask" d="#{h p[%q{d}]}" transform="translate(#{p[%q{dx}]},#{p[%q{dy}]})"/>)
  end

  return %(<path d="#{h p[%q{d}]}" fill="#{fill}" transform="translate(#{p[%q{dx}]},#{p[%q{dy}]})"/>) if p[%q{ivs}].to_s.empty?

  b = part_box(p) or return %q{}
  ch = p[%q{ivs}].split(%q{ }).map { |c| c.to_i(16) }.pack(%q{U*})
  dx = p[%q{dx}].to_f
  # box は content stream から出した em の箱。高さがそのまま font-size、幅が textLength。
  size = (b[3] - b[1]).round(3)
  # dy は上下に積む字のずれ。<path> の transform と同じだけ <text> にも効かせる。
  dy = p[%q{dy}].to_f
  %(<text x="#{(b[0] + dx).round(3)}" y="#{(b[3] - size * 0.12 + dy).round(3)}" font-size="#{size}" ) +
    %(textLength="#{(b[2] - b[0]).round(3)}" lengthAdjust="spacingAndGlyphs" fill="#{fill}">#{h ch}</text>)
end

# 輪郭を 1 つの SVG にする。1 文字が複数パーツで組まれることがあり、同じ id の行を
# dx だけ右にずらして重ねる。色は辞書の凡例がそのまま入っている。
def outline_svg(parts)
  boxes = parts.filter_map { |p| part_extent(p) }
  return nil if boxes.empty?

  x0 = boxes.map(&:first).min
  y0 = boxes.map { |b| b[1] }.min
  x1 = boxes.map { |b| b[2] }.max
  y1 = boxes.map(&:last).max
  pad = [(x1 - x0), (y1 - y0)].max * 0.06
  view = [x0 - pad, y0 - pad, (x1 - x0) + pad * 2, (y1 - y0) + pad * 2]
  paths = parts.map { |p| part_svg(p, '#1c1a18') }
  %(<svg viewBox="#{view.map { |v| v.round(3) }.join(' ')}" role="img">#{paths.join}</svg>)
end


# 部品を組み立てて作ってある字か。マスク以外のパーツが 2 つ以上あればそう。
def composed?(parts)
  parts.to_a.count { |p| p['fill'].to_s.delete(' ') != 'rgb(100%,100%,100%)' } > 1
end

targets = entries.reject { |e| e['ivs'].empty? && e['cid'].empty? }
cells = targets.map do |e|
  parts = outlines[e['id']]
  composed = composed?(parts)
  glyph, kind = if e['ivs'].empty?
                  [outline_svg(parts || []) || '<span class="none">—</span>',
                   composed ? '合成' : '輪郭']
                else
                  [h(e['ivs'].split(' ').map { |c| c.to_i(16) }.pack('U*')), 'IVS']
                end
  base = e['ivs'].empty? ? nil : [e['ivs'].split(' ').first.to_i(16)].pack('U')
  # 基底字が組み立ての部品として現れるなら、複数パーツの片方だけを拾っている（`⿰冫虫` に
  # 冫 の CID）。パーツを全部拾うようにしたので、残っていればまだ取りこぼしがある。
  wrong = base && e['ids'].length > 1 && e['ids'].each_char.any?(base)
  # 基底字と代用字が違うだけなら、異体字自身が符号位置を持つとき（倂/併）で正常。
  odd = base && !e['sub'].empty? && base != e['sub'] && !wrong
  # 代用字が 1 文字とは限らない。37 件は代用字自体が外字注記なので、中の字を解いて出す。
  sub_cell = if e['sub'].empty?
               # 2rem の — は 一 に見えてしまうので文言にする
               '<span class="none">代用字なし</span>'
             elsif e['sub'].start_with?('※')
               c = sub_char(e['sub'], jis2ucs)
               %(<span title="#{h e['sub']}">#{c ? h(c) : '<span class="asnote">※</span>'}</span>)
             else
               h(e['sub'])
             end
  search = [e['id'], e['sub'], e['desc'], e['ivs'], e['cid'], kind].join(' ')
  <<~HTML
    <div class="c#{wrong ? ' wrong' : (odd ? ' odd' : '')}#{composed ? ' cmpd' : ''}" data-s="#{h search}">
      <div class="pair"><b>#{sub_cell}</b><i>#{glyph}</i></div>
      <div class="m"><a href="#{h "tmp/gaiji_chuki.html##{e['id']}"}">#{e['id']}</a> #{kind}
        #{h(e['ivs'].empty? ? "CID #{e['cid']}" : e['ivs'])}</div>
      <div class="d">#{h e['desc']}</div>
    </div>
  HTML
end

ivs_n = targets.count { |e| !e['ivs'].empty? }
base_of = lambda { |e|
  return nil if e['ivs'].empty?

  [e['ivs'].split(' ').first.to_i(16)].pack('U')
}
part_of_ids = lambda do |e|
  b = base_of.call(e)
  b && e['ids'].length > 1 && e['ids'].each_char.any?(b)
end
wrong_n = targets.count { |e| part_of_ids.call(e) }
composed_n = targets.count { |e| composed?(outlines[e['id']]) }
odd_n = targets.count do |e|
  b = base_of.call(e)
  b && !e['sub'].empty? && b != e['sub'] && !part_of_ids.call(e)
end

html = <<~HTML
  <!DOCTYPE html>
  <html lang="ja">
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>外字注記辞書 ivs/cid 検分</title>
  <style>
    body { margin:0; background:#fbfaf7; color:#1c1a18; line-height:1.6;
           font-family:"Hiragino Mincho ProN","Yu Mincho",serif }
    header { padding:1.4rem 1.5rem; border-bottom:2px solid #1c1a18 }
    h1 { margin:0 0 .4rem; font-size:1.15rem }
    p { margin:0; font-size:.8rem; color:#6b6560; line-height:1.9; max-width:56rem }
    .tools { position:sticky; top:0; z-index:2; background:#fbfaf7; padding:.6rem 1.5rem;
             border-bottom:1px solid #d8d4cc; display:flex; gap:.8rem; align-items:center }
    #q { flex:1; max-width:26rem; font:inherit; font-size:.9rem; padding:.35rem .55rem;
         border:1px solid #d8d4cc; background:#fff; border-radius:3px }
    label { font-size:.8rem; color:#6b6560 }
    #count { font-size:.78rem; color:#6b6560 }
    main { display:flex; flex-wrap:wrap; gap:.5rem; padding:1rem 1.5rem 3rem }
    .c { width:8.6rem; border:1px solid #e7e3db; border-radius:4px; padding:.4rem .5rem; background:#fff }
    /* 枠色だけで区別する。地は白のまま——マスクを白で塗るので色を付けると出てしまう */
    .c.odd { border-color:#d9c68a }
    .c.wrong { border-color:#c0392b }
    .pair { display:flex; align-items:center; justify-content:center; gap:.5rem;
           font-size:2rem; line-height:1.25; height:2.7rem }
    .pair .asnote { font-size:1.1rem; color:#b8860b }
    /* 複数パーツで組む字。読み順に、縦積みか横並びで置く */
    .cmp { display:inline-flex; line-height:.92; font-size:1rem }
    .cmp.v { flex-direction:column }
    .cmp.r { flex-direction:row; align-items:center }
    .cmp i { font-style:normal }
    .pair b { font-weight:normal; color:#a9a29a }
    .pair i { font-style:normal }
    .pair svg { width:2rem; height:2rem; display:block; fill:#1c1a18 }
    .pair svg .mask { fill:#fff }
    .none { color:#c9c4bc; font-size:.68rem; line-height:1.3 }
    .m { font-size:.66rem; color:#6b6560; font-family:ui-monospace,Menlo,monospace;
         white-space:nowrap; overflow:hidden; text-overflow:ellipsis }
    .m a { color:#6b6560 }
    .d { font-size:.68rem; color:#6b6560; height:2.6em; overflow:hidden }
    [hidden] { display:none !important }
  </style>

  <header>
    <h1>外字注記辞書 — <code>ivs</code> / <code>cid</code> の検分</h1>
    <p>
      左が<b>代用字</b>（包摂される側の標準の字）、右が<b>この辞書が示している字形</b>。
      同じ字の書き分けに見えていれば結び付けは正しい。まったく別の字に見えたら、CID を
      隣のエントリのものと取り違えている。<br>
      全 #{targets.size} 件（IVS #{ivs_n}・輪郭 #{targets.size - ivs_n}）。
      <b>IVS はブラウザのフォント任せ</b>で、Adobe-Japan1 の異体字セレクタに対応していないと
      左右が同じ形に見える——その場合この一覧では判断できない（macOS の Hiragino は対応する）。
      輪郭のほうは PDF から取った実際の描画なので、フォントに依らない。<br>
      <b style="color:#c0392b">赤い枠は誤り #{wrong_n} 件</b>——1 文字を複数のパーツで組んでいる字で、
      片方のパーツの CID を拾っている（<code>⿰冫虫</code> に 冫 の CID）。<br>
      <b>合成 #{composed_n} 件</b>は部品を組み立てて作ってある字。辞書は「部品を描いては白い矩形で
      消す」手順で組んでおり、その通りに重ねて描いている。<br>
      黄色い枠は IVS の基底字が代用字と違う #{odd_n} 件。異体字自身が符号位置を持つ場合
      （倂 と 併）で、それ自体は正常。
    </p>
  </header>

  <div class="tools">
    <input id="q" type="search" placeholder="id・代用字・説明・CID で絞り込む" autocomplete="off">
    <label><input type="checkbox" id="oddonly"> 印の付いたものだけ（赤・黄）</label>
    <label><input type="checkbox" id="cmponly"> 合成しているものだけ</label>
    <span id="count"></span>
  </div>

  <main>#{cells.join}</main>

  <script>
  const q = document.getElementById('q'), only = document.getElementById('oddonly');
  const cmp = document.getElementById('cmponly');
  const count = document.getElementById('count');
  const cells = [...document.querySelectorAll('.c')];
  function run() {
    const t = q.value.trim();
    let n = 0;
    for (const c of cells) {
      const hit = (!t || c.dataset.s.includes(t))
        && (!only.checked || c.classList.contains('odd') || c.classList.contains('wrong'))
        && (!cmp.checked || c.classList.contains('cmpd'));
      c.hidden = !hit;
      if (hit) n++;
    }
    count.textContent = n + ' / ' + cells.length + ' 件';
  }
  // 絞り込みを URL で渡せるようにする（file:// では replaceState が弾かれるので握り潰す）
  const param = new URLSearchParams(location.search).get('q');
  if (param) q.value = param;
  q.addEventListener('input', () => {
    run();
    try { history.replaceState(null, '', q.value ? '?q=' + encodeURIComponent(q.value) : location.pathname); } catch (_) {}
  });
  only.addEventListener('change', run);
  cmp.addEventListener('change', run);
  run();
  </script>
  </html>
HTML

File.write(OUT, html)
warn "#{OUT}  #{(File.size(OUT) / 1024.0).round} KB  #{targets.size} 件（IVS #{ivs_n} / 輪郭 #{targets.size - ivs_n}）"
