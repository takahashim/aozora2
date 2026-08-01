# frozen_string_literal: true

# 字形欄に何が描いてあるかを、1 ページぶんの PDF から解く。
#
# 呼び出し側は「この枠に何がある？」だけを聞く。中でやっているのは 3 つ:
#
#   1. content stream の CID を見る。非埋め込みの Ryumin-Light は Adobe-Japan1 なので、
#      CID がそのまま字の同定になる（IVD で異体字セレクタ列に落ちる）
#   2. パーツが 1 つのときだけ、その CID で字が決まる。複数パーツの字は「部品を描いては
#      白い矩形で消す」手順で組んであり、どれか 1 つの CID も、並べた列も字を表さない
#   3. 複数パーツなら輪郭に回す。マスクごと拾い、描いた順に並べて返す
#
# ページ単位の重い読み取り（content stream・pdftocairo の SVG）はインスタンスが抱えて
# 遅延で 1 回だけ行う。

require 'rexml/document'
require 'open3'
require 'set'
require 'tmpdir'
require_relative 'pdf_text_runs'
require_relative 'gaiji_glyph_svg'

class GaijiGlyphResolver
  # 字形と注記の間に入る空白の CID（Adobe-Japan1）。パーツと数えないよう除く。
  SPACE_CID = 633

  # 枠と run／パーツの照合に許す食い違い。字送りが詰まっても 1.5 は離れない。
  TOLERANCE = 1.5

  Result = Struct.new(:ivs, :cid, :parts, keyword_init: true)

  def initialize(pdf, pdf_page, page, path, ivd)
    @pdf = pdf
    @pdf_page = pdf_page
    @page = page
    @path = path
    @ivd = ivd
    @height = pdf.resolve(pdf_page[:MediaBox])[3]
  end

  # box は [x0, x1, y0, y1]（pdftotext の単語 bbox の座標系）。
  # 何も描いていなければ nil。
  def resolve(box)
    hits = runs_in(box)
    seq = single_ivs(hits)
    return Result.new(ivs: seq, cid: cids_of(hits), parts: []) if seq

    parts = outline_parts(box, hits)
    return nil if parts.empty?

    Result.new(ivs: '', cid: cids_of(hits), parts: parts)
  end

  private

  def cids_of(hits)
    hits.map { |r| r.cids[0] }.join(' ')
  end

  def runs_in(box)
    x0, x1, y0, y1 = box
    runs.select do |r|
      r.cids.size == 1 && r.cids[0] != SPACE_CID && r.x >= x0 && r.x <= x1 &&
        top_down(r) > y0 - 2 && top_down(r) < y1 + 0.5
    end
  end

  def top_down(run)
    @height - run.y
  end

  # パーツが 1 つで、非埋め込みの Ryumin なら CID が字を指す。それ以外は nil。
  def single_ivs(hits)
    return nil unless hits.size == 1
    return nil unless ryumin?(hits[0])

    seq = @ivd[hits[0].cids[0]].to_s
    seq.empty? ? nil : seq
  end

  def ryumin?(run)
    base, embedded = fonts[run.font] || ['', false]
    !embedded && base.include?('Ryumin')
  end

  # 描いた順のまま、枠に入るものを取る。マスクは枠と重なるかで見る。
  def outline_parts(box, hits)
    x0, x1, y0, y1 = box
    in_box = ops.select do |op|
      if op[:kind] == :glyph
        op[:x] >= x0 && op[:x] <= x1 && op[:y] > y0 - 2 && op[:y] < y1 + 0.5 && defs[op[:ref]]
      else
        b = GaijiGlyphSvg.path_box(op[:d])
        b && b[0] < x1 + 2 && b[2] > x0 - 2 && b[1] < y1 + 4 && b[3] > y0 - 12
      end
    end
    glyph_ops = in_box.select { |op| op[:kind] == :glyph }
    return [] if glyph_ops.empty?

    origin = glyph_ops.map { |op| op[:x] }.min
    baseline = glyph_ops.first[:y]
    used = Set.new
    in_box.each_with_index.map do |op, n|
      op[:kind] == :mask ? mask_part(op, n, origin, baseline) : glyph_part(op, n, origin, baseline, hits, used)
    end
  end

  def mask_part(op, part, origin, baseline)
    # マスクはページ座標のまま。原点とベースラインぶん寄せれば字形の座標系に乗る。
    { part: part, fill: op[:fill], dx: (-origin).round(3), dy: (-baseline).round(3),
      d: op[:d], ivs: '', box: '' }
  end

  def glyph_part(op, part, origin, baseline, hits, used)
    run = nearest_run(op, hits, used)
    # 非埋め込みの Ryumin は poppler が代替フォントで CID を解決できず**別の字を描く**。
    # 輪郭を残すと二重写しになるので捨てて、異体字セレクタ列と枠だけ持つ。表示側が
    # 読み手の Adobe-Japan1 フォントに描かせる。
    seq = run && ryumin?(run) ? @ivd[run.cids[0]].to_s : ''
    # 枠は content stream の変形から出す。辞書はパーツを横 0.7 倍などに潰して組むので
    # （`.7 0 0 1 0 0 cm`）、等倍で置くと隣のパーツと重なる。
    box = seq.empty? ? '' : [0, -(run.sy * 0.88), run.sx, run.sy * 0.12].map { |v| v.round(3) }.join(' ')
    { part: part, fill: op[:fill] || 'black', dx: (op[:x] - origin).round(3),
      dy: (op[:y] - baseline).round(3), d: seq.empty? ? defs[op[:ref]] : '', ivs: seq, box: box }
  end

  # パーツに対応する run。x だけでは縦に積む字（同じ x）を取り違えるので y も見る。
  # さらに**同じ run を二度使わない**——`竹かんむり／擧` は 2 つのパーツが 0.17 しか離れて
  # おらず、近さだけで選ぶと両方が同じ run に当たる。近い順に 1 つずつ取る。
  def nearest_run(op, hits, used)
    run, idx = hits.each_with_index
                   .reject { |_, i| used.include?(i) }
                   .min_by { |r, _| (r.x - op[:x]).abs + (top_down(r) - op[:y]).abs }
    return nil if run.nil?
    return nil if (run.x - op[:x]).abs > TOLERANCE || (top_down(run) - op[:y]).abs > TOLERANCE

    used << idx
    run
  end

  # --- ページ単位の読み取り（遅延・1 回だけ） -------------------------------

  def runs
    @runs ||= PdfTextRuns.of(@pdf, @pdf_page)
  end

  def fonts
    @fonts ||= PdfTextRuns.fonts(@pdf, @pdf_page)
  end

  def defs
    load_svg
    @defs
  end

  def ops
    load_svg
    @ops
  end

  # pdftocairo の SVG から、`<defs>` の輪郭と、描いた順の操作列を読む。
  # `<use>` だけでなく**白い塗りつぶしも拾う**。マスクを落とすと部品が全部重なってつぶれる。
  def load_svg
    return if @ops

    tmp = File.join(Dir.tmpdir, "gaiji_p#{@page}.svg")
    system('pdftocairo', '-svg', '-f', @page.to_s, '-l', @page.to_s, @path, tmp,
           out: File::NULL, err: File::NULL)
    doc = REXML::Document.new(File.read(tmp))
    @defs = {}
    REXML::XPath.match(doc, "//*[local-name()='g'][@id]").each do |g|
      next unless g['id'].start_with?('glyph-')

      path = REXML::XPath.first(g, "*[local-name()='path']")
      @defs[g['id']] = path['d'] if path
    end
    @ops = []
    collect(doc.root, nil)
    File.delete(tmp) if File.exist?(tmp)
  end

  def collect(el, fill)
    f = el.attributes['fill'] || fill
    el.elements.each do |c|
      case c.name
      when 'use'
        href = (c.attributes['xlink:href'] || c.attributes['href']).to_s.delete('#')
        @ops << { kind: :glyph, x: c.attributes['x'].to_f, y: c.attributes['y'].to_f,
                  ref: href, fill: f }
      when 'path'
        # 塗りは <g> ではなく <path> 自身に付いている。親の値で見ると拾えない。
        pf = c.attributes['fill'] || f
        d = c.attributes['d'].to_s
        @ops << { kind: :mask, d: d, fill: pf } if pf.to_s.delete(' ') == GaijiGlyphSvg::MASK_FILL && !d.empty?
      end
      collect(c, f)
    end
  end
end
