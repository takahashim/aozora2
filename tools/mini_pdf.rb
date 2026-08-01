# frozen_string_literal: true

# 外字注記辞書を読むだけの、ごく小さな PDF リーダ。
#
# 欲しいのは「どのフォントの・どの CID を・どこに描いたか」だけで、字形も文字も要らない
# （tools/extract_gaiji_chuki.rb の説明を参照）。対象の PDF は 2011 年の dvipdfmx 製で、
# 古典的な xref 表・FlateDecode のみ・オブジェクトストリーム無しなので、この範囲なら
# 標準ライブラリだけで足りる。汎用の PDF ライブラリではない。
#
# 割り切っていること:
#   - xref 表は読まない。`N 0 obj` を総なめして表を作る（この PDF は増分更新が無い）
#   - 暗号化・オブジェクトストリーム・FlateDecode 以外のフィルタは扱わない
#   - 図形演算子は無視する。テキストの位置も平行移動しか追わない（この PDF は
#     回転・拡大をしていない）

require 'zlib'
require 'strscan'

class MiniPdf
  Ref = Struct.new(:num)

  def initialize(path)
    @raw = File.binread(path)
    @offsets = {}
    scan_objects
    @cache = {}
  end

  # MediaBox・Resources はページツリーの親から継承しうる。
  INHERITED = %i[MediaBox Resources].freeze

  # 文書中のページを順に返す（ページツリーを深さ優先でたどる）
  def pages
    root = resolve(trailer[:Root])
    out = []
    walk = lambda do |node, inherited|
      node = resolve(node)
      inherited = inherited.merge(node.slice(*INHERITED).compact)
      if node[:Type] == :Page
        out << inherited.merge(node)
      else
        (resolve(node[:Kids]) || []).each { |k| walk.call(k, inherited) }
      end
    end
    walk.call(root[:Pages], {})
    out
  end

  # 参照なら実体を返す。参照でなければそのまま返す。
  def resolve(value)
    return value unless value.is_a?(Ref)

    @cache[value.num] ||= parse_object(value.num)
  end

  # ページの内容ストリーム（複数に分かれていれば連結する）
  def content(page)
    list = resolve(page[:Contents])
    list = [page[:Contents]] unless list.is_a?(Array)
    list.map { |ref| stream_data(ref) }.join("\n")
  end

  private

  def scan_objects
    pos = 0
    while (i = @raw.index(/(\d+)\s+\d+\s+obj\b/, pos))
      @offsets[Regexp.last_match(1).to_i] = i
      pos = i + Regexp.last_match(0).length
    end
  end

  def trailer
    @trailer ||= parse_dict(StringScanner.new(@raw[@raw.rindex('trailer') + 7..]))
  end

  def object_body(num)
    start = @offsets[num] or return nil
    stop = @raw.index('endobj', start) || @raw.length
    @raw[start...stop].sub(/\A\d+\s+\d+\s+obj\b/, '')
  end

  def parse_object(num)
    body = object_body(num) or return nil
    parse_value(StringScanner.new(body))
  end

  # ストリーム本体を取り出して展開する。長さは /Length に頼らず endstream で切る
  # （/Length が間接参照のことがあるため）。
  def stream_data(ref)
    num = ref.is_a?(Ref) ? ref.num : nil
    body = num ? object_body(num) : nil
    return '' unless body && (i = body.index(/stream\r?\n/))

    data = body[(i + Regexp.last_match(0).length)...(body.rindex('endstream') || body.length)]
    dict = parse_value(StringScanner.new(body))
    filter = dict.is_a?(Hash) ? dict[:Filter] : nil
    filter == :FlateDecode ? Zlib::Inflate.inflate(data) : data
  end

  # --- PDF オブジェクトの構文 ---------------------------------------------

  def parse_value(scanner)
    skip_space(scanner)
    if scanner.scan(/<</) then parse_dict_body(scanner)
    elsif scanner.scan(/\[/) then parse_array(scanner)
    elsif scanner.scan(%r{/([A-Za-z0-9#+._-]+)}) then scanner[1].to_sym
    elsif scanner.scan(/(\d+)\s+(\d+)\s+R\b/) then Ref.new(scanner[1].to_i)
    elsif scanner.scan(/-?\d*\.?\d+/) then scanner.matched.include?('.') ? scanner.matched.to_f : scanner.matched.to_i
    elsif scanner.scan(/\((?:\\.|[^)\\])*\)/) then scanner.matched[1..-2]
    elsif scanner.scan(/<[0-9A-Fa-f\s]*>/) then scanner.matched
    elsif scanner.scan(/true|false|null/) then scanner.matched == 'true'
    else
      scanner.getch
      nil
    end
  end

  def parse_dict(scanner)
    skip_space(scanner)
    scanner.scan(/<</) ? parse_dict_body(scanner) : {}
  end

  def parse_dict_body(scanner)
    out = {}
    loop do
      skip_space(scanner)
      break if scanner.scan(/>>/) || scanner.eos?
      break unless scanner.scan(%r{/([A-Za-z0-9#+._-]+)})

      out[scanner[1].to_sym] = parse_value(scanner)
    end
    out
  end

  def parse_array(scanner)
    out = []
    loop do
      skip_space(scanner)
      break if scanner.scan(/\]/) || scanner.eos?

      out << parse_value(scanner)
    end
    out
  end

  def skip_space(scanner)
    scanner.skip(/(?:\s|%[^\n]*\n)+/)
  end
end
