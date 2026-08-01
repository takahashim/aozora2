# frozen_string_literal: true

# 外字注記辞書の説明文を IDS（漢字構成記述文字列）に変換する。
#
# 説明文は部品の組み立てを表す小さな言語になっている:
#
#     ＋   左右に並べる      口＋畢 → ⿰口畢
#     ／   上下に重ねる      髟／舌 → ⿱髟舌
#     ＜   囲む              囗＜力 → ⿴囗力（囲む側で演算子が変わる）
#     −    部品を取り除く    尓−小（IDS に対応する演算子が無いので変換できない）
#     （）  グループ化
#
# 部品は文字そのもののほか、「にんべん」のような呼び名や「膠のつくり」のような他の字
# からの参照で書かれる。前者は語彙表、後者は CHISE の IDS 表の分解で解く。
#
# 規則の妥当性は、符号位置を持つエントリで検算できる（説明 → IDS → IDS 表 → 符号位置が、
# エントリ自身の U+ と一致するか）。2769 件中 2728 件が一致した（98.5%）。
#
# IDS 表: https://raw.githubusercontent.com/cjkvi/cjkvi-ids/master/ids.txt

module GaijiIds
  # 囲む側の部品ごとに IDS の演算子が違う
  ENCLOSE = {
    '囗' => '⿴',                                              # 全体を囲む
    '門' => '⿵', '鬥' => '⿵', '勹' => '⿵', '气' => '⿵', '冂' => '⿵', # 上から囲む
    '匚' => '⿷', '匸' => '⿷',                                  # 左から囲む
    '凵' => '⿶'                                                # 下から囲む
  }.freeze

  # 部品の呼び名。辞書に出てくるものだけ。
  LEXICON = {
    'にんべん' => '亻', 'さんずい' => '氵', 'くさかんむり' => '艹', 'りっしんべん' => '忄',
    'ころもへん' => '衤', 'こざとへん' => '阝', 'おおざと' => '阝', 'てへん' => '扌',
    'つちへん' => '土', '土へん' => '土', 'きへん' => '木', '木へん' => '木',
    'いとへん' => '糸', '糸へん' => '糸', 'かねへん' => '金', '金へん' => '金',
    'ひへん' => '火', '火へん' => '火', 'めへん' => '目', '目へん' => '目',
    'くちへん' => '口', '口へん' => '口', 'にすい' => '冫', 'うかんむり' => '宀',
    'たけかんむり' => '竹', 'あめかんむり' => '雨', 'れんが' => '灬', 'れっか' => '灬',
    'しんにょう' => '辶', '二点しんにょう' => '辶', 'ぎょうにんべん' => '彳',
    'のぎへん' => '禾', 'あしへん' => '𧾷', '足へん' => '𧾷',
    'うまへん' => '馬', '馬へん' => '馬', 'さかなへん' => '魚', '魚へん' => '魚',
    'とりへん' => '鳥', '鳥へん' => '鳥', 'かいへん' => '貝', '貝へん' => '貝',
    'ひとやね' => '𠆢', '人がしら' => '𠆢', 'なべぶた' => '亠', 'まだれ' => '广',
    'やまいだれ' => '疒', 'しかばね' => '尸', 'もんがまえ' => '門', 'ぎょうがまえ' => '行',
    'おんなへん' => '女', '女へん' => '女', 'つきへん' => '月', '月へん' => '月',
    'にちへん' => '日', '日へん' => '日', 'たへん' => '田', '田へん' => '田',
    'いしへん' => '石', '石へん' => '石', 'しめすへん' => '礻', 'のごめへん' => '釆',
    'ふるとり' => '隹', 'おおがい' => '頁', 'ちから' => '力'
  }.freeze

  Unconvertible = Class.new(StandardError)

  # CHISE の IDS 表を読む。[IDS → 字, 字 → IDS]
  def self.load_table(path)
    to_code = {}
    of_code = {}
    File.foreach(path, encoding: 'UTF-8') do |line|
      next if line.start_with?('#')

      _, char, ids = line.chomp.split("\t", 4)
      next if ids.nil? || char.nil?

      ids = ids.split('[').first.to_s.strip
      next if ids.empty? || ids == char

      to_code[ids] ||= char
      of_code[char] ||= ids
    end
    [to_code, of_code]
  end

  class Converter
    def initialize(of_code)
      @of_code = of_code
    end

    # 説明文 → IDS。変換できない書き方は Unconvertible を投げる。
    def convert(desc)
      raise Unconvertible, '引き算や自然言語混じり' if desc.match?(/[−「」、]/)

      expr(desc)
    end

    private

    def expr(str)
      str = str.strip
      return expr(str[1...-1]) if str.start_with?('（') && matching(str) == str.length - 1

      depth = 0
      (str.length - 1).downto(0) do |i|
        case str[i]
        when '）' then depth += 1
        when '（' then depth -= 1
        else
          next unless depth.zero? && '＋／＜'.include?(str[i])

          left = expr(str[0...i])
          right = expr(str[(i + 1)..])
          return "⿰#{left}#{right}" if str[i] == '＋'
          return "⿱#{left}#{right}" if str[i] == '／'

          op = ENCLOSE[left] or raise Unconvertible, "囲み方が分からない: #{left}"
          return "#{op}#{left}#{right}"
        end
      end
      part(str)
    end

    # 部品ひとつを 1 文字に解決する
    def part(text)
      text = text.strip
      return text if text.length == 1
      return LEXICON[text] if LEXICON.key?(text)

      if (m = text.match(/\A(.+?)の(つくり|へん)\z/))
        base = m[1].length > 1 ? part(m[1]) : m[1]
        ids = @of_code[base]
        if ids && ids.length == 3 && ids[0] == '⿰'
          return m[2] == 'つくり' ? ids[2] : ids[1]
        end

        raise Unconvertible, "#{text} を分解できない"
      end
      raise Unconvertible, "部品 #{text.inspect} が引けない"
    end

    def matching(str)
      depth = 0
      str.each_char.with_index do |c, i|
        depth += 1 if c == '（'
        if c == '）'
          depth -= 1
          return i if depth.zero?
        end
      end
      -1
    end
  end

  # IDS → GlyphWiki の合成グリフ名（⿰旬力 → u2ff0-u65ec-u529b）
  def self.glyphwiki_name(ids)
    ids.each_char.map { |c| format('u%04x', c.ord) }.join('-')
  end
end
