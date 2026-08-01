# frozen_string_literal: true

# 外字注記辞書の説明文を IDS（漢字構成記述文字列）に変換する。
#
# 説明文は部品の組み立てを表す小さな言語になっている:
#
#     ＋   左右に並べる      口＋畢 → ⿰口畢（垂れ・繞は左の部品で演算子が変わる）
#     ／   上下に重ねる      髟／舌 → ⿱髟舌
#     ＜   囲む              囗＜力 → ⿴囗力（囲む側で演算子が変わる）
#     −    部品を取り除く    文−亠 → 乂（IDS に演算子が無いので、左を分解して引く）
#     （）  グループ化
#
# 部品は文字そのもののほか、「にんべん」のような呼び名や「膠のつくり」のような他の字
# からの参照で書かれる。前者は語彙表、後者は CHISE の IDS 表の分解で解く。
#
# 規則の妥当性は、符号位置を持つエントリで検算できる（説明 → IDS → IDS 表 → 符号位置が、
# エントリ自身の U+ と一致するか）。字が引けた 6130 件中 6011 件が一致した（98.1%）。
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

  # ＋ は普通 ⿰ だが、垂れ（广・疒）は ⿸、繞（辶・走）は ⿺ になる。どちらを取るかは
  # 部品で決まるので表にした。値は CHISE の IDS 表での多数決（疒 は 953 件すべて ⿸）。
  # 鹿 は表でも ⿸128 対 ⿰46 と割れていて、辞書の 6 件は ⿰ 側だったので入れていない。
  ATTACH = {
    '广' => '⿸', '疒' => '⿸', '尸' => '⿸', '厂' => '⿸', '戶' => '⿸', '虍' => '⿸',
    '麻' => '⿸',
    '辶' => '⿺', '廴' => '⿺', '走' => '⿺', '尢' => '⿺', '毛' => '⿺'
  }.freeze

  # IDS 演算子と項数
  ARITY = {
    '⿰' => 2, '⿱' => 2, '⿴' => 2, '⿵' => 2, '⿶' => 2, '⿷' => 2, '⿸' => 2,
    '⿹' => 2, '⿺' => 2, '⿻' => 2, '⿲' => 3, '⿳' => 3
  }.freeze

  # 三項から 1 つ抜けたら二項に落ちる
  DEMOTE = { '⿲' => '⿰', '⿳' => '⿱' }.freeze

  # CHISE は符号位置の無い部品を ①〜⑳ の代用記号で書く。これが入った IDS は組み立てを
  # 表していないので、そこまで分解したら諦める（`⿱⑥廾` は何も同定していない）。
  PLACEHOLDER = /[①-⑳]/

  # 同じ部品に符号位置が 2 つある組。IDS 表がどちらで書くかは字によって違うので、
  # 表を引くときだけ入れ替えて試す（龱 は ⿴囗㐅 で載っていて ⿴囗乂 では引けない）。
  VARIANTS = { '乂' => '㐅' }.freeze

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
    'ふるとり' => '隹', 'おおがい' => '頁', 'ちから' => '力',
    '竹かんむり' => '竹', '草かんむり' => '艹', '雨かんむり' => '雨'
  }.freeze

  # 辞書が名前で呼ぶ部品は、それ以上分解しない単位として扱う。
  ATOMIC = LEXICON.values.to_set.freeze

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
    def initialize(of_code, to_code = {})
      @of_code = of_code
      @to_code = to_code
    end

    # `「尅」の「寸」に代えて「土」`。位置指定つき（`の左の「人」に代えて`）や、A が入れ子の
    # 注記になっているものは、どの部品かを一意に決められないので拾わない。
    REPLACEMENT = /\A「([^「」]+)」((?:[の、]「[^「」]+」に代えて「[^「」]+」)+)\z/

    # 説明文 → IDS。変換できない書き方は Unconvertible を投げる。
    # replace が false なら「〜に代えて」は無視して土台だけ組む（呼び出し側の判断）。
    def convert(desc, replace: true)
      if (m = desc.match(REPLACEMENT))
        return convert(m[1]) unless replace

        pairs = m[2].scan(/[の、]「([^「」]+)」に代えて「([^「」]+)」/)
        return guard(with_bmp_retry { |b| build(m[1], pairs, b) })
      end
      raise Unconvertible, '自然言語混じり' if desc.match?(/[「」、]/)

      guard(with_bmp_retry { |b| build(desc, [], b) })
    end

    # IDS → 字。部品の異体（乂/㐅）は入れ替えて引き直す。
    def char_of(ids)
      return @to_code[ids] if @to_code.key?(ids)

      alt = ids.chars.map { |c| VARIANTS[c] || c }.join
      alt == ids ? nil : @to_code[alt]
    end

    private

    def guard(ids)
      raise Unconvertible, "符号位置の無い部品が残る: #{ids}" if ids.match?(PLACEHOLDER)

      ids
    end

    # 字が引けないなら BMP 外へ畳んだ得が無いので、読める形に組み直す。`⿱自儿` を
    # 𧠆 (U+27806) に替えても表現力は同じで、Ext-B の無いフォントで読めなくなるだけ。
    def with_bmp_retry
      ids = yield(false)
      return ids if ids.length == 1 || char_of(ids)

      yield(true)
    end

    def build(desc, pairs, bmp_only)
      @bmp_only = bmp_only
      ids = expr(desc, top: true)
      pairs.each { |from, to| ids = apply(ids, expr(from), expr(to)) }
      ids
    end

    def apply(ids, from, to)
      # 同じ部品に解決される組がある（`者` に代えて `睹のつくり`＝異体の 者）。IDS では
      # 書き分けられないので、木をばらさずそのままにする方が組み立てが残って得。
      return ids if from == to

      tree = tree_of(ids) or raise Unconvertible, "#{ids} を分解できない"
      swapped = swap(tree, from, to) or raise Unconvertible, "#{ids} に #{from} が無い"
      out = render(swapped)
      fold(out) || out
    end

    # 木の中の from を to に差し替える。drop と同じく、直下を見てから項を分解して潜る。
    def swap(node, from, to)
      return nil unless node.is_a?(Array)

      op, *kids = node
      kids.each_index do |i|
        next unless same?(kids[i], from)

        return [op, *kids.each_with_index.map { |k, j| j == i ? to : k }]
      end
      kids.each_index do |i|
        next if ATOMIC.include?(kids[i])

        sub = kids[i].is_a?(Array) ? kids[i] : tree_of(kids[i])
        next if sub.nil?
        next unless (reduced = swap(sub, from, to))

        return [op, *kids.each_with_index.map { |k, j| j == i ? reduced : k }]
      end
      nil
    end

    # top では畳まない。畳んだ結果が 1 文字だと、呼び出し側が「その字の別字形」の意味に
    # 読んでしまうため（`ids` が 1 文字のときの約束）。
    def expr(str, top: false)
      str = str.strip
      return expr(str[1...-1], top: top) if str.start_with?('（') && matching(str) == str.length - 1

      depth = 0
      (str.length - 1).downto(0) do |i|
        case str[i]
        when '）' then depth += 1
        when '（' then depth -= 1
        else
          next unless depth.zero? && '＋／＜−'.include?(str[i])

          left = expr(str[0...i])
          right = expr(str[(i + 1)..])
          return subtract(left, right) if str[i] == '−'

          op = case str[i]
               when '＋' then ATTACH.fetch(left, '⿰')
               when '／' then '⿱'
               else ENCLOSE[left] or raise Unconvertible, "囲み方が分からない: #{left}"
               end
          ids = "#{op}#{left}#{right}"
          return top ? ids : (fold(ids) || ids)
        end
      end
      part(str)
    end

    # 引き算。左を IDS に分解して right を 1 つ取り除き、残りを返す。
    def subtract(left, right)
      tree = tree_of(left) or raise Unconvertible, "#{left} を分解できない"
      rest = drop(tree, right) or raise Unconvertible, "#{left} から #{right} を取り除けない"
      ids = render(rest)
      fold(ids) || ids
    end

    # 部分 IDS を 1 文字に畳む。@bmp_only のときは BMP 外へは畳まない（convert 参照）。
    def fold(ids)
      c = char_of(ids)
      c if c && !(@bmp_only && c.ord > 0xFFFF)
    end

    # 1 文字なら IDS 表で分解し、既に IDS ならそのまま木にする
    def tree_of(str)
      if str.length == 1
        ids = @of_code[str]
        return nil if ids.nil? || ids.empty? || ids == str
      else
        ids = str
      end
      return nil if ids.match?(PLACEHOLDER)

      node, pos = parse(ids, 0)
      pos == ids.length ? node : nil
    end

    def parse(ids, pos)
      c = ids[pos]
      return [nil, pos] if c.nil?

      n = ARITY[c]
      return [c, pos + 1] unless n

      kids = []
      pos += 1
      n.times do
        kid, pos = parse(ids, pos)
        return [nil, pos] if kid.nil?

        kids << kid
      end
      [[c, *kids], pos]
    end

    def render(node)
      node.is_a?(Array) ? node[0] + node[1..].map { |k| render(k) }.join : node
    end

    # 木から target を 1 つ取り除く。まず直下の項を見て、無ければ項を分解して潜る。
    def drop(node, target)
      return nil unless node.is_a?(Array)

      op, *kids = node
      kids.each_index do |i|
        next unless same?(kids[i], target)

        rest = kids.reject.with_index { |_, j| j == i }
        return rest.size == 1 ? rest[0] : [DEMOTE.fetch(op, op), *rest]
      end
      kids.each_index do |i|
        # 辞書が部品として名前を持つもの（艹・亻…）はそれ以上ばらさない。ばらすと
        # `草−十` が 艹（＝十十）の側から引かれて ⿱十早 になってしまう。
        next if ATOMIC.include?(kids[i])

        sub = kids[i].is_a?(Array) ? kids[i] : tree_of(kids[i])
        next if sub.nil?
        next unless (reduced = drop(sub, target))

        return [op, *kids.each_with_index.map { |k, j| j == i ? reduced : k }]
      end
      nil
    end

    # 部品どうしの同一視。表記が IDS と 1 文字で食い違うことがあるので両向きに見る。
    def same?(node, target)
      s = render(node)
      return true if s == target
      return true if target.length == 1 && @of_code[target] == s
      return true if s.length == 1 && @of_code[s] == target

      false
    end

    # 部品ひとつを 1 文字に解決する
    def part(text)
      text = text.strip
      return text if text.length == 1
      return LEXICON[text] if LEXICON.key?(text)

      if (m = text.match(/\A(.+?)の(つくり|へん)\z/))
        base = m[1].length > 1 ? part(m[1]) : m[1]
        tree = tree_of(base)
        # へん・つくりは ⿰ の左右。項自体が組み立てのこともある（疆 = ⿰⿹弓土畺）。
        return render(m[2] == 'つくり' ? tree[2] : tree[1]) if tree.is_a?(Array) && tree[0] == '⿰'

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
