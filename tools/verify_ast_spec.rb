#!/usr/bin/env ruby
# 交換形式の仕様（docs/spec-rawast-json.md・docs/spec-aozora-ast-json.md）が、
# 実装が実際に吐く JSON と食い違っていないかを検証する。
#
#   ruby tools/verify_ast_spec.rb
#
# 突き合わせの材料は data/conformance/*.json（`cargo test --features serde
# --test conformance` が実装から再生成する正規の例）なので、**実装の実出力**と
# 仕様書を比べることになる。ソースの構文解析ではないため、serde の属性
# （rename・flatten・skip 等）による表れ方の違いもそのまま拾える。
#
# 検出するもの:
#   - 仕様に無い構成子（実装が吐いているのに表に無い）
#   - 仕様に無いフィールド／仕様にあるのに出ないフィールド（構成子ごと）
#   - フィールドの JSON の型が仕様の注釈と合わない
#   - 文書全体の器のキーの食い違い
#   - フィクスチャが一度も作らない構成子（＝例が無く、上の照合が効かない）
#
# 型は JSON の型（数値・文字列・真偽・列・オブジェクト・null）まで見る。
# `Nat` が負でないか、`Text` の中身が何かまでは見ない。そこは人が読む。

require 'json'
require 'set'

ROOT = File.expand_path('..', __dir__)
FIXTURES = File.join(ROOT, 'crates/aozora-core/data/conformance')

SPECS = {
  'raw_ast' => File.join(ROOT, 'docs/spec-rawast-json.md'),
  'aozora_ast' => File.join(ROOT, 'docs/spec-aozora-ast-json.md')
}.freeze

# 木のてっぺんの器（`RawLine` など）は `kind` を持たないので、別に拾って渡す。
TOP_LEVEL = {
  'raw_ast' => ['RawLine', ->(f) { f.dig('raw_ast', 'lines')&.first }],
  'aozora_ast' => [nil, nil]
}.freeze

# --- 仕様書を読む -----------------------------------------------------------

# 仕様書は 2 通りの書き方で構成子を定義している。どちらも拾う。
#
#   1. 表          `| \`Text\` | \`{ a: X }\` | … |`
#   2. 擬似 BNF    ```  Block = | Line { inline: … } | Nested { … }  ```
#                  ```  Break = Br | None | NoNewline                ```
#                  ```  RawLine = { line_no: Nat, … }                ```
#
# 同じ名前が別の型の構成子として何度も出る（`Midashi` は Inline でもあり BlockKind でも
# ある、`Normal` は MidashiStyle の値）。そこで「名前 → 許される形の集合」として持ち、
# 実装の出す形がどれかに当たれば一致とみなす。形はフィールド名の集合、または nil
# （値を直接入れる／値を持たない）。
#
# 仕様ごとに 1 つ作る。両仕様に同名の構成子があり（`Img` など）、まとめて持つと
# 片方の記述でもう片方を照合してしまう。
class Spec
  attr_reader :shapes, :field_types, :type_names

  def initialize(path)
    @shapes = {}          # 構成子名 → 形の集合
    @field_types = {}     # 構成子名 → フィールド名 → 仕様の型注釈（省略なら nil）
    @type_names = Set.new # `X = …` の左辺（型の名前。構成子ではない）
    text = without_appendix(File.read(path))
    read_bnf(text)
    read_tables(text)
  end

  private

  # 付録は JSON ではなく Rust 側の型との対応表なので、照合の対象から外す。
  def without_appendix(text)
    i = text.index(/^## 付録/)
    i ? text[0...i] : text
  end

  def add(name, fields)
    (@shapes[name] ||= Set.new) << fields
  end

  # `{ base: Inline*, direction, width: Nat? }` から 名前 → 型注釈 を拾う。
  # 仕様は型を省いて名前だけ書くことがある（`direction`）。その場合は nil。
  def parse_fields(body)
    body.split(',').each_with_object({}) do |part, out|
      m = part.strip.match(/\A(\w+)\s*(?::\s*(\S+))?/)
      out[m[1]] = m[2] if m
    end
  end

  def add_with_fields(name, body)
    fields = parse_fields(body)
    add(name, fields.keys.to_set)
    (@field_types[name] ||= {}).merge!(fields) { |_k, a, b| a || b }
  end

  def read_tables(text)
    text.each_line do |line|
      next unless line.start_with?('|')

      cells = line.split('|').map(&:strip)
      next if cells.size < 3

      names = cells[1].scan(/`([A-Z][A-Za-z]*)`/).flatten
      next if names.empty?

      if cells[2].include?('{')
        body = cells[2][/\{(.*)\}/m, 1].to_s
        names.each { |n| add_with_fields(n, body) }
      else
        names.each { |n| add(n, nil) }
      end
    end
  end

  # ``` で囲まれたブロックの中の定義を拾う。`X = A | B { … }` は複数行に折り返される。
  def read_bnf(text)
    in_block = false
    buffer = nil
    flush = lambda do
      next if buffer.nil?

      name, body = buffer
      if (m = body.match(/\A\{(.*)\}\z/m))
        # `RawLine = { … }`（構成子名が付かない素のレコード）
        add_with_fields(name, m[1])
      else
        body.split(/\|(?![^{]*\})/).map(&:strip).reject(&:empty?).each do |alt|
          if (m = alt.match(/\A([A-Z][A-Za-z]*)\s*\{(.*)\}\z/m))
            add_with_fields(m[1], m[2])
          elsif (m = alt.match(/\A([A-Z][A-Za-z]*)(?:\s+\S.*)?\z/m))
            # `Style StyleType` のように値を直接持つ変種と、値を持たない変種。
            # どちらも JSON では `value` がオブジェクトにならないので同じ形で扱う。
            add(m[1], nil)
          end
        end
      end
      buffer = nil
    end

    text.each_line do |line|
      if line.start_with?('```')
        flush.call
        in_block = !in_block
        next
      end
      next unless in_block

      # 行末コメントは連結する前に落とす（連結後だと `//` 以降が全部消える）。
      line = line.sub(%r{//.*}, '')

      if (m = line.match(/\A\s*(\w+)\s*=\s*(.*)$/))
        flush.call
        @type_names << m[1]
        buffer = [m[1], m[2].strip]
      elsif buffer && line.strip.start_with?('|')
        buffer[1] += " #{line.strip}"
      elsif buffer && !line.strip.empty? && buffer[1].count('{') > buffer[1].count('}')
        buffer[1] += " #{line.strip}"
      else
        flush.call
      end
    end
    flush.call
  end
end

# --- フィクスチャを読む -----------------------------------------------------

# 交換形式の構成子は `{"kind": 名前, "value": 内容}` の形（value 省略あり）。
class Actual
  attr_reader :shapes, :field_types, :string_values, :envelope

  def initialize(tree)
    @shapes = {}             # 構成子名 → 形の集合
    @field_types = {}        # 構成子名 → フィールド名 → 実際の JSON の型名の集合
    @string_values = Set.new # 文字列として出た列挙値（被覆の計算にだけ使う）
    @envelope = Set.new
    Dir[File.join(FIXTURES, '*.json')].sort.each do |path|
      doc = JSON.parse(File.read(path))[tree]
      @envelope |= doc.keys.to_set
      walk(doc)
    end
  end

  # 器（`RawLine` など）は `kind` を持たないので、呼び出し側から登録する。
  def register(name, value)
    (@shapes[name] ||= Set.new) << value.keys.to_set
    record_types(name, value)
  end

  private

  def walk(node)
    case node
    when Hash
      if node['kind'].is_a?(String)
        value = node['value']
        (@shapes[node['kind']] ||= Set.new) << (value.is_a?(Hash) ? value.keys.to_set : nil)
        record_types(node['kind'], value)
      end
      # 文字列になった列挙値（`"brk": "Br"`）は構成子として現れない。仕様の書き方
      # （`| MidashiLevel | "Naka"（中）… |`）とは照合できないので、被覆を数えるためだけに控える。
      node.each do |k, v|
        @string_values << v if k != 'kind' && v.is_a?(String) && v.match?(/\A[A-Z][A-Za-z]*\z/)
      end
      node.each_value { |v| walk(v) }
    when Array
      node.each { |v| walk(v) }
    end
  end

  def record_types(name, value)
    return unless value.is_a?(Hash)

    seen = (@field_types[name] ||= {})
    value.each { |k, v| (seen[k] ||= Set.new) << v.class.name }
  end
end

# --- 照合 -------------------------------------------------------------------

# 仕様の型注釈 → JSON で許される型。`X?` は null も許し、`X*` は列。
# それ以外の名前（`Span` `BlockKind` `MidashiLevel` …）は、フィールドを持つ型なら
# オブジェクト、フィールドを持たない列挙なら文字列になるのでどちらも許す。
def json_types_for(annotation)
  return nil if annotation.nil?

  base = annotation.delete('`')
  nullable = base.end_with?('?')
  base = base.chomp('?')
  types =
    if base.end_with?('*')
      ['Array']
    else
      case base
      when 'Nat' then ['Integer']
      when 'Text' then ['String']
      when 'Bool' then %w[TrueClass FalseClass]
      else %w[Hash String]
      end
    end
  nullable ? types + ['NilClass'] : types
end

# 実装が出した形が、仕様に書かれたいずれかの形に当たるか。
def shape_problems(name, documented_shapes, fields)
  return [] if documented_shapes.include?(fields)

  with_fields = documented_shapes.compact
  if fields && with_fields.size == 1
    documented = with_fields.first
    return (fields - documented).sort.map { |f| "#{name}: 仕様に無いフィールド `#{f}`" } +
           (documented - fields).sort.map { |f| "#{name}: 仕様にあるが出ない `#{f}`" }
  end

  shown = fields ? "{ #{fields.sort.join(', ')} }" : '値を直接入れる形'
  ["#{name}: 実装は #{shown} だが、仕様にその形が無い"]
end

def type_problems(spec, actual)
  problems = []
  actual.field_types.each do |name, seen_fields|
    documented = spec.field_types[name]
    next if documented.nil?

    documented.each do |field, annotation|
      allowed = json_types_for(annotation)
      seen = seen_fields[field]
      next if allowed.nil? || seen.nil?

      if seen.all? { |t| t == 'NilClass' } && !allowed.include?('NilClass')
        problems << "#{name}.#{field}: 仕様は `#{annotation}` だが、例が null しか無く確かめられない"
        next
      end
      bad = seen.reject { |t| allowed.include?(t) || t == 'NilClass' }
      next if bad.empty?

      problems << "#{name}.#{field}: 仕様は `#{annotation}`（#{allowed.join('/')}）" \
                  "だが、実際は #{bad.sort.join('/')}"
    end
  end
  problems
end

# 文書全体の器（`{"format": …, "lines": …}`）のキーが仕様どおりか。
def envelope_problems(spec_path, actual)
  block = File.read(spec_path)[/## 3\. 文書全体.*?```json\n(.*?)```/m, 1]
  return ['文書全体の例が仕様に見当たらない'] unless block

  documented = block.scan(/^\s*"(\w+)":/).flatten.to_set
  (actual.envelope - documented).sort.map { |k| "文書全体: 仕様に無いキー `#{k}`" } +
    (documented - actual.envelope).sort.map { |k| "文書全体: 仕様にあるが出ないキー `#{k}`" }
end

def register_top_level(tree, actual)
  name, pick = TOP_LEVEL[tree]
  return nil unless name

  sample = Dir[File.join(FIXTURES, '*.json')].sort
                                             .filter_map { |p| pick.call(JSON.parse(File.read(p))) }
                                             .first
  return nil unless sample

  actual.register(name, sample)
  name
end

def report(tree, spec_path)
  spec = Spec.new(spec_path)
  actual = Actual.new(tree)
  top_name = register_top_level(tree, actual)

  problems = envelope_problems(spec_path, actual)
  actual.shapes.sort.each do |name, field_sets|
    unless spec.shapes.key?(name)
      problems << "#{name}: 実装が吐いているが仕様に無い"
      next
    end
    field_sets.each { |fields| problems.concat(shape_problems(name, spec.shapes[name], fields)) }
  end
  problems.concat(type_problems(spec, actual))

  uncovered = spec.shapes.keys - actual.shapes.keys - actual.string_values.to_a -
              spec.type_names.to_a - [top_name].compact
  [problems.uniq, uncovered.sort]
end

def main
  status = 0
  SPECS.each do |tree, path|
    problems, uncovered = report(tree, path)
    puts "== #{File.basename(path)}"
    if problems.empty?
      puts '  食い違い なし'
    else
      status = 1
      problems.each { |p| puts "  [不一致] #{p}" }
    end
    unless uncovered.empty?
      puts "  （フィクスチャに現れない構成子: #{uncovered.join(' ')}）"
      puts '   ※ 名前が別の列挙と重なる構成子は、文字列値の一致で被覆と数えていることがある'
    end
    puts
  end
  warn '仕様と実装が食い違っています。' unless status.zero?
  exit status
end

main if __FILE__ == $PROGRAM_NAME
