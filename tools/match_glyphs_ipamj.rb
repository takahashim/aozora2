#!/usr/bin/env ruby
# frozen_string_literal: true

# 外字注記辞書の字形（輪郭）を IPAmj明朝のグリフと重ね合わせて MJ 文字図形を特定する。
#
#   ruby tools/match_glyphs_ipamj.rb crates/aozora-core/data ipamjm.ttf mji.tsv \
#       > crates/aozora-core/data/gaiji_chuki_mj.tsv
#
# **任意の工程**。IPAmj明朝はライセンス同意の先にあり、MJ 文字情報一覧表も別途入手が
# 要るので、抽出器本体とは分けてある。対象は `glyph=1` の 222 件——埋め込みサブセット
# フォントで描かれていて CID が外から解釈できず、輪郭しか手がかりが無いもの。
#
# 候補は代用字（`→［包摂適用 X］` の X）から作る。X の異体字が MJ に何通りか載っている
# ので、それぞれを IPAmj明朝から描いて、辞書の字形と一番よく重なるものを選ぶ。
#
# 較正: 正解と分かっているペアの IoU は中央値 0.61、無関係なペアは 0.22 で分離する。
# IoU>=0.40 かつ 2 位と 0.05 以上の差があるものを「確信できる一致」とした（137 件、
# IoU 中央値 0.95）。
#
# IoU は塗り方の実装に敏感で、同じ入力でも走査線の丸めが違えば 0.02 ほど動く。どの
# MJ 図形を選ぶかは変わらない（Python で書いた版と共通 135 件すべてで一致した）が、
# 閾値ぎりぎりのものは出たり出なかったりする。数を当てにせず、iou と runner_up を
# 見て利用側で判断すること。
#
# MJ 文字情報一覧表: https://moji.or.jp/mojikiban/mjlist/ の mji.*.xlsx を
#   mj / ucs / ucs_impl / ivs の 4 列を持つ TSV にしたもの。

require 'csv'
require_relative 'mini_ttf'

N = 96          # 比較に使う正方形の一辺
MIN_IOU = 0.40  # 確信できる一致の下限
MIN_MARGIN = 0.05

# SVG のパスデータを折れ線の列にする。M/L/C/Z だけ扱う（cairo の出力はこれで足りる）。
def svg_contours(d)
  cmds = []
  d.scan(/([MLCZmlcz])|(-?\d+\.?\d*(?:e-?\d+)?)/) do
    if Regexp.last_match(1)
      cmds << [Regexp.last_match(1), []]
    elsif cmds.any?
      cmds.last[1] << Regexp.last_match(2).to_f
    end
  end
  out = []
  cur = []
  pos = [0.0, 0.0]
  close = lambda do
    out << cur if cur.size > 2
    cur = []
  end
  cmds.each do |op, v|
    case op
    when 'M', 'm'
      close.call
      cur = [[v[0], v[1]]]
      pos = cur[0]
      (2...v.size).step(2) { |i| cur << [v[i], v[i + 1]]; pos = cur.last }
    when 'L', 'l'
      (0...v.size).step(2) { |i| cur << [v[i], v[i + 1]]; pos = cur.last }
    when 'C', 'c'
      (0...v.size).step(6) do |i|
        p0 = pos
        1.upto(8) do |k|
          t = k / 8.0
          pos = [0, 1].map do |j|
            ((1 - t)**3 * p0[j]) + (3 * (1 - t)**2 * t * v[i + j]) +
              (3 * (1 - t) * t * t * v[i + 2 + j]) + (t**3 * v[i + 4 + j])
          end
          cur << pos
        end
      end
    when 'Z', 'z'
      close.call
    end
  end
  close.call
  out
end

# 外接矩形で正規化してから N×N に描く。偶奇規則で塗るので中抜きも残る。
# 1 行を 1 つの整数（ビット列）で持つ。
def raster(contours, flip_y)
  pts = contours.flatten(1)
  return nil if pts.empty?

  xs = pts.map(&:first)
  ys = pts.map(&:last)
  w = xs.max - xs.min
  h = ys.max - ys.min
  return nil if w <= 0 || h <= 0

  s = (N - 8) / [w, h].max.to_f
  ox = (N - (w * s)) / 2
  oy = (N - (h * s)) / 2
  edges = []
  contours.each do |c|
    scaled = c.map do |x, y|
      px = ((x - xs.min) * s) + ox
      py = ((y - ys.min) * s) + oy
      [px, flip_y ? N - py : py]
    end
    scaled.each_cons(2) { |a, b| edges << [a, b] }
    edges << [scaled.last, scaled.first]
  end

  rows = Array.new(N, 0)
  N.times do |row|
    y = row + 0.5
    xs_at = []
    edges.each do |(x1, y1), (x2, y2)|
      next if (y1 > y) == (y2 > y)

      xs_at << x1 + ((y - y1) / (y2 - y1) * (x2 - x1))
    end
    next if xs_at.size < 2

    xs_at.sort!
    bits = 0
    xs_at.each_slice(2) do |a, b|
      next unless b

      from = [a.ceil, 0].max
      to = [b.floor, N - 1].min
      next if to < from

      bits |= ((1 << (to - from + 1)) - 1) << from
    end
    rows[row] = bits
  end
  rows
end

def popcount(bits) = bits.to_s(2).count('1')

def iou(a, b)
  inter = union = 0
  N.times do |i|
    inter += popcount(a[i] & b[i])
    union += popcount(a[i] | b[i])
  end
  union.zero? ? 0.0 : inter.to_f / union
end

def main(datadir, font_path, mji_tsv)
  font = MiniTtf.new(font_path)

  by_ucs = Hash.new { |h, k| h[k] = [] }
  CSV.foreach(mji_tsv, col_sep: "\t", headers: true) do |m|
    u = m['ucs'].to_s.empty? ? m['ucs_impl'].to_s : m['ucs']
    next unless u.start_with?('U+')

    ch = begin
      [u[2..].to_i(16)].pack('U')
    rescue StandardError
      next
    end
    by_ucs[ch] << m
  end

  outlines = Hash.new { |h, k| h[k] = [] }
  CSV.foreach(File.join(datadir, 'gaiji_chuki_glyphs.tsv'), col_sep: "\t", headers: true) do |g|
    outlines[g['id']] << [g['dx'].to_f, g['d']]
  end

  puts %w[id desc sub mj ivs iou runner_up candidates].join("\t")
  CSV.foreach(File.join(datadir, 'gaiji_chuki.tsv'), col_sep: "\t", headers: true) do |r|
    next if r['glyph'].to_s.empty? || r['sub'].to_s.length != 1

    contours = outlines[r['id']].flat_map do |dx, d|
      svg_contours(d).map { |c| c.map { |x, y| [x + dx, y] } }
    end
    a = contours.empty? ? nil : raster(contours, false)
    next unless a

    scored = []
    by_ucs[r['sub']].each do |m|
      glyph = m['ivs'].to_s.empty? ? nil : font.glyph_for_ivs(m['ivs'])
      glyph ||= font.glyph_for_ivs("#{m['ucs'].to_s.sub('U+', '')}_E0100") if m['ucs'].to_s.start_with?('U+')
      next if glyph.nil? || glyph.contours.empty?

      b = raster(glyph.contours, true)
      scored << [iou(a, b), m] if b
    end
    next if scored.empty?

    scored.sort_by! { |s, _| -s }
    best, second = scored[0], (scored[1] ? scored[1][0] : 0.0)
    next if best[0] < MIN_IOU || best[0] - second < MIN_MARGIN

    puts [r['id'], r['desc'], r['sub'], best[1]['mj'], best[1]['ivs'],
          format('%.3f', best[0]), format('%.3f', second), scored.size].join("\t")
  end
end

main(*ARGV[0, 3]) if __FILE__ == $PROGRAM_NAME
