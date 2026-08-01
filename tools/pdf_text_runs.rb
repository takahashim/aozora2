# frozen_string_literal: true

# PDF の内容ストリームから「どのフォントの・どの CID を・どこに描いたか」を読む。
#
# 字形そのものではなく CID を読むのは、外字注記辞書の字形欄の 6 割が**非埋め込み**の
# Ryumin-Light で描かれているため。描画系は代替フォントで CID を解決できず .notdef
# （□に×）になるので輪郭を取っても意味が無いが、CID は Adobe-Japan1 のものなので
# Unicode の IVD 経由で異体字セレクタ列に落ちる（tools/extract_gaiji_chuki.rb を参照）。
#
# 平行移動しか追わない。この PDF は回転も拡大もしていない。

require_relative 'mini_pdf'

module PdfTextRuns
  Run = Struct.new(:font, :x, :y, :cids)

  TOKEN = /
      \/(?<name>[A-Za-z0-9\#+._-]+)
    | (?<num>-?\d*\.?\d+)
    | \[(?<arr>[^\]]*)\]
    | \((?<str>(?:\\.|[^)\\])*)\)
    | <(?<hex>[0-9A-Fa-f\s]*)>
    | (?<op>[A-Za-z*'"]+)
  /x

  # ページのテキスト描画を Run の列で返す。座標は PDF ユーザ空間（左下原点）。
  def self.of(pdf, page)
    data = pdf.content(page)
    stack = []
    ctm = [0.0, 0.0]
    font = nil
    lx = ly = 0.0
    args = []
    runs = []

    data.scan(TOKEN) do
      m = Regexp.last_match
      if (op = m[:op])
        case op
        when 'q' then stack.push(ctm)
        when 'Q' then ctm = stack.pop || [0.0, 0.0]
        when 'cm'
          ctm = [ctm[0] + args[-2].to_f, ctm[1] + args[-1].to_f] if args.size >= 6
        when 'BT' then lx = ly = 0.0
        when 'Td', 'TD'
          if args.size >= 2
            lx += args[-2].to_f
            ly += args[-1].to_f
          end
        when 'Tf' then font = args[-2] if args.size >= 2
        when 'TJ', 'Tj'
          cids = Array(args.last).grep(Array).flatten + Array(args.last).grep(String)
          cids = cids.grep(/\Ahex:/).flat_map { |h| h[4..].scan(/..../).map { |c| c.to_i(16) } }
          runs << Run.new(font, ctm[0] + lx, ctm[1] + ly, cids) unless cids.empty?
        end
        args = []
        next
      end
      args << if m[:hex] then "hex:#{m[:hex].gsub(/\s/, '')}"
              elsif m[:arr] then m[:arr].scan(/<([0-9A-Fa-f\s]*)>/).map { |h| "hex:#{h[0].gsub(/\s/, '')}" }
              elsif m[:name] then m[:name]
              else m[0]
              end
    end
    runs
  end

  # フォント資源名 => [BaseFont, 埋め込みか]
  def self.fonts(pdf, page)
    res = pdf.resolve(page[:Resources]) || {}
    (pdf.resolve(res[:Font]) || {}).to_h do |name, ref|
      f = pdf.resolve(ref) || {}
      desc = pdf.resolve(f[:DescendantFonts])
      fd = desc ? pdf.resolve(pdf.resolve(desc[0])[:FontDescriptor]) : nil
      embedded = fd ? %i[FontFile FontFile2 FontFile3].any? { |k| fd.key?(k) } : false
      [name.to_s, [f[:BaseFont].to_s, embedded]]
    end
  end

  # IVD の CID => 異体字セレクタ列（例 14041 => "51E1 E0101"）
  def self.load_ivd(path, collection: 'Adobe-Japan1')
    out = {}
    File.foreach(path) do |line|
      next if line.start_with?('#') || !line.include?(';')

      seq, coll, name = line.split(';').map(&:strip)
      next unless coll == collection && name.to_s.start_with?('CID+')

      out[name[4..].to_i] = seq
    end
    out
  end
end
