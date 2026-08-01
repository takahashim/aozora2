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
  # sx / sy は、その run の全角 1 文字ぶんの大きさ（フォントサイズに CTM の伸縮をかけたもの）。
  # 辞書の字形は部品を横 0.7 倍などに潰して組んであり、そこを知らないと部品が重なる。
  Run = Struct.new(:font, :x, :y, :cids, :sx, :sy)

  TOKEN = /
      \/(?<name>[A-Za-z0-9\#+._-]+)
    | (?<num>-?\d*\.?\d+)
    | \[(?<arr>[^\]]*)\]
    | \((?<str>(?:\\.|[^)\\])*)\)
    | <(?<hex>[0-9A-Fa-f\s]*)>
    | (?<op>[A-Za-z*'"]+)
  /x

  IDENTITY = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0].freeze

  # 行列の積（PDF の 2×3 表現）。`cm` は今の CTM の**前**に掛かる。
  def self.mul(m, n)
    [m[0] * n[0] + m[1] * n[2], m[0] * n[1] + m[1] * n[3],
     m[2] * n[0] + m[3] * n[2], m[2] * n[1] + m[3] * n[3],
     m[4] * n[0] + m[5] * n[2] + n[4], m[4] * n[1] + m[5] * n[3] + n[5]]
  end

  # ページのテキスト描画を Run の列で返す。座標は PDF ユーザ空間（左下原点）。
  #
  # **CTM もテキスト行列も伸縮まで追う。** 辞書は字形を組むときに `.7 0 0 1 0 0 cm` のように
  # 部品を横へ潰す。平行移動だけ見ていると、部品の大きさが分からず並べたときに重なる。
  def self.of(pdf, page)
    data = pdf.content(page)
    stack = []
    ctm = IDENTITY.dup
    tm = IDENTITY.dup   # テキスト行列
    tlm = IDENTITY.dup  # テキスト行の行列。Td はここに積み、Tm は両方を置き換える
    font = nil
    size = 0.0
    args = []
    runs = []

    data.scan(TOKEN) do
      m = Regexp.last_match
      if (op = m[:op])
        case op
        when 'q' then stack.push(ctm)
        when 'Q' then ctm = stack.pop || IDENTITY.dup
        when 'cm'
          ctm = mul(args[-6..].map(&:to_f), ctm) if args.size >= 6
        when 'BT'
          tm = IDENTITY.dup
          tlm = IDENTITY.dup
        when 'Td', 'TD'
          if args.size >= 2
            tlm = mul([1.0, 0.0, 0.0, 1.0, args[-2].to_f, args[-1].to_f], tlm)
            tm = tlm.dup
          end
        when 'Tm'
          if args.size >= 6
            tlm = args[-6..].map(&:to_f)
            tm = tlm.dup
          end
        when 'Tf'
          if args.size >= 2
            font = args[-2]
            size = args[-1].to_f
          end
        when 'TJ', 'Tj'
          cids = Array(args.last).grep(Array).flatten + Array(args.last).grep(String)
          cids = cids.grep(/\Ahex:/).flat_map { |h| h[4..].scan(/..../).map { |c| c.to_i(16) } }
          unless cids.empty?
            trm = mul(tm, ctm)
            runs << Run.new(font, trm[4], trm[5], cids, (size * trm[0]).abs, (size * trm[3]).abs)
          end
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
