# frozen_string_literal: true

# `gaiji_chuki_glyphs.tsv` のパーツを 1 つの SVG に組む。
#
# 辞書の字形は**部品を描いては白い矩形で消す**手順で作ってある。行の順に `dx`・`dy` だけ
# ずらして重ねると 1 文字になる。パーツは 3 種類:
#
#   輪郭   `d` にパスがある。埋め込みフォントで描かれたもの
#   字     `d` が空で `ivs` と `box` がある。非埋め込みの Ryumin は poppler が代替フォントで
#          CID を解決できず別の字を描くので、輪郭を捨てて異体字セレクタ列だけ持つ。
#          `box` は content stream の変形から出した em の箱で、そこに字を流し込む
#   マスク  白い塗りつぶし。地の色で塗るので、色は CSS（`.mask`）に任せる
#
# `dx`・`dy` は**描画と外接矩形の両方**に効かせること。片方だけだと重なるか、上が切れる。

module GaijiGlyphSvg
  # 全角 1 文字は em の縦 0.88 上・0.12 下に収まるものとして扱う。
  BASELINE_RATIO = 0.12

  MASK_FILL = 'rgb(100%,100%,100%)'

  module_function

  def mask?(part)
    part['fill'].to_s.delete(' ') == MASK_FILL
  end

  # path の全座標から外接矩形を出す。M/L/C はすべて絶対座標。
  def path_box(d)
    xs = []
    ys = []
    d.to_s.scan(/[MLC]([^MLCZ]*)/) do
      Regexp.last_match(1).scan(/-?\d*\.?\d+/).map(&:to_f).each_slice(2) do |x, y|
        next if y.nil?

        xs << x
        ys << y
      end
    end
    xs.empty? ? nil : [xs.min, ys.min, xs.max, ys.max]
  end

  # 字として置くパーツの em の箱。`x0 y0 x1 y1`。
  def em_box(part)
    b = part['box'].to_s.split(' ').map(&:to_f)
    b.size == 4 ? b : nil
  end

  # そのパーツが占める範囲。マスクは字より大きいので数えない（塗るだけ）。
  def extent(part)
    return nil if mask?(part)

    dx = part['dx'].to_f
    dy = part['dy'].to_f
    if part['ivs'].to_s.empty?
      b = path_box(part['d']) or return nil
      return [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy]
    end

    b = em_box(part) or return nil
    [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy]
  end

  def part_svg(part, fill, escape)
    dx = part['dx']
    dy = part['dy']
    return %(<path class="mask" d="#{escape.call(part['d'])}" transform="translate(#{dx},#{dy})"/>) if mask?(part)

    if part['ivs'].to_s.empty?
      return %(<path d="#{escape.call(part['d'])}" fill="#{fill}" transform="translate(#{dx},#{dy})"/>)
    end

    b = em_box(part) or return ''
    ch = part['ivs'].split(' ').map { |c| c.to_i(16) }.pack('U*')
    size = (b[3] - b[1]).round(3)
    y = (b[3] - size * BASELINE_RATIO + dy.to_f).round(3)
    %(<text x="#{(b[0] + dx.to_f).round(3)}" y="#{y}" font-size="#{size}" ) +
      %(textLength="#{(b[2] - b[0]).round(3)}" lengthAdjust="spacingAndGlyphs" fill="#{fill}">#{escape.call(ch)}</text>)
  end

  # parts を 1 つの `<svg>` にする。fill は「パーツ → 色」、escape は「文字列 → HTML」。
  # 色の決め方は使う側で違う（辞書の凡例を使うか、単色か）ので外から渡す。
  def render(parts, fill:, escape:)
    boxes = parts.filter_map { |p| extent(p) }
    return nil if boxes.empty?

    x0 = boxes.map(&:first).min
    y0 = boxes.map { |b| b[1] }.min
    x1 = boxes.map { |b| b[2] }.max
    y1 = boxes.map(&:last).max
    pad = [(x1 - x0), (y1 - y0)].max * 0.06
    view = [x0 - pad, y0 - pad, (x1 - x0) + pad * 2, (y1 - y0) + pad * 2]
    body = parts.map { |p| part_svg(p, fill.call(p), escape) }.join
    %(<svg viewBox="#{view.map { |v| v.round(3) }.join(' ')}" role="img">#{body}</svg>)
  end

  # 部品を組み立てて作ってある字か。マスク以外のパーツが 2 つ以上あればそう。
  def composed?(parts)
    parts.to_a.count { |p| !mask?(p) } > 1
  end
end
