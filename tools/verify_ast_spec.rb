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
#   - フィクスチャが一度も作らない構成子（＝例が無く、上の照合が効かない）
#
# 「フィールドの型が正しいか」までは見ない。そこは人が読む。

require 'json'
require 'set'

ROOT = File.expand_path('..', __dir__)
FIXTURES = File.join(ROOT, 'crates/aozora-core/data/conformance')

SPECS = {
  'raw_ast' => File.join(ROOT, 'docs/spec-rawast-json.md'),
  'aozora_ast' => File.join(ROOT, 'docs/spec-aozora-ast-json.md')
}.freeze

# 仕様書は 2 通りの書き方で構成子を定義している。どちらも拾う。
#
#   1. 表          `| \`Text\` | \`{ a: X }\` | … |`
#   2. 擬似 BNF    ```  Block = | Line { inline: … } | Nested { … }  ```
#                  ```  Break = Br | None | NoNewline                ```
#                  ```  RawLine = { line_no: Nat, … }                ```
#
# 同じ名前が別の型の構成子として何度も出る（`Midashi` は Inline でもあり
# BlockKind でもある、`Normal` は MidashiStyle の値）。そこで「名前 → 許される
# 形の集合」として持ち、実装の出す形がどれかに当たれば一致とみなす。
# 形は フィールド名の集合、または nil（値を直接入れる／値を持たない）。

def add(out, name, fields)
  (out[name] ||= Set.new) << fields
end

# `{ base: Inline*, ruby: Inline*, direction, keep_gaiji_notes_in_base: Bool }` から
# フィールド名を拾う。仕様は型を省いて名前だけ書くことがある（`direction`）。
def field_names(body)
  body.split(',').filter_map { |part| part.strip[/\A(\w+)/, 1] }.to_set
end

# 付録は JSON ではなく Rust 側の型との対応表なので、照合の対象から外す。
def without_appendix(text)
  i = text.index(/^## 付録/)
  i ? text[0...i] : text
end

def constructors_from_table(text, out)
  text.each_line do |line|
    next unless line.start_with?('|')

    cells = line.split('|').map(&:strip)
    next if cells.size < 3

    names = cells[1].scan(/`([A-Z][A-Za-z]*)`/).flatten
    next if names.empty?

    fields = cells[2].include?('{') ? field_names(cells[2][/\{(.*)\}/m, 1].to_s) : nil
    names.each { |n| add(out, n, fields) }
  end
end

# ``` で囲まれたブロックの中の定義を拾う。`X = A | B { … }` は複数行に折り返される。
def constructors_from_bnf(text, out)
  in_block = false
  buffer = nil
  flush = lambda do
    next if buffer.nil?

    name, body = buffer
    # `RawLine = { … }`（構成子名が付かない素のレコード）
    if (m = body.match(/\A\{(.*)\}\z/m))
      add(out, name, field_names(m[1]))
    else
      body.split(/\|(?![^{]*\})/).map(&:strip).reject(&:empty?).each do |alt|
        if (m = alt.match(/\A([A-Z][A-Za-z]*)\s*\{(.*)\}\z/m))
          add(out, m[1], field_names(m[2]))
        elsif (m = alt.match(/\A([A-Z][A-Za-z]*)\z/))
          add(out, m[1], nil)
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

def spec_constructors(path)
  text = without_appendix(File.read(path))
  out = {}
  constructors_from_bnf(text, out)
  constructors_from_table(text, out)
  out
end

# フィクスチャの JSON を歩いて、実際に現れた 構成子 → フィールド集合 を集める。
# 交換形式の構成子は `{"kind": 名前, "value": 内容}` の形（value 省略あり）。
def actual_constructors(node, out = {})
  case node
  when Hash
    if node['kind'].is_a?(String)
      value = node['value']
      fields = value.is_a?(Hash) ? value.keys.to_set : nil
      (out[node['kind']] ||= Set.new) << fields
    end
    node.each_value { |v| actual_constructors(v, out) }
  when Array
    node.each { |v| actual_constructors(v, out) }
  end
  out
end

def collect_actual
  found = { 'raw_ast' => {}, 'aozora_ast' => {} }
  Dir[File.join(FIXTURES, '*.json')].sort.each do |path|
    fixture = JSON.parse(File.read(path))
    found.each_key { |tree| actual_constructors(fixture[tree], found[tree]) }
  end
  found
end

# 木のてっぺんの器（`RawLine` など）は `kind` を持たないので、別に拾って渡す。
TOP_LEVEL = {
  'raw_ast' => ['RawLine', ->(f) { f.dig('raw_ast', 'lines')&.first }],
  'aozora_ast' => [nil, nil]
}.freeze

def top_level_fields(name_and_pick)
  name, pick = name_and_pick
  return nil unless name

  Dir[File.join(FIXTURES, '*.json')].sort.each do |path|
    sample = pick.call(JSON.parse(File.read(path)))
    return [name, sample.keys.to_set] if sample
  end
  nil
end

# 実装が出した形 `fields`（フィールド名の集合、または nil）が、仕様に書かれた
# いずれかの形に当たるか。当たらなければ理由を文にして返す。
def shape_problems(name, documented_shapes, fields)
  return [] if documented_shapes.include?(fields)

  # フィールドを持つ形どうしなら、差分を出したほうが直しやすい。
  with_fields = documented_shapes.compact
  if fields && with_fields.size == 1
    documented = with_fields.first
    return (fields - documented).sort.map { |f| "#{name}: 仕様に無いフィールド `#{f}`" } +
           (documented - fields).sort.map { |f| "#{name}: 仕様にあるが出ない `#{f}`" }
  end

  shown = fields ? "{ #{fields.sort.join(', ')} }" : '値を直接入れる形'
  ["#{name}: 実装は #{shown} だが、仕様にその形が無い"]
end

# 文書全体の器（`{"format": …, "lines": …}`）のキーが仕様どおりか。
# 仕様の「## 3. 文書全体」にある JSON ブロックを期待値として読む。
def envelope_problems(tree, spec_path)
  spec = without_appendix(File.read(spec_path))
  block = spec[/## 3\. 文書全体.*?```json\n(.*?)```/m, 1]
  return ['文書全体の例が仕様に見当たらない'] unless block

  documented = block.scan(/^\s*"(\w+)":/).flatten.to_set
  sample = Dir[File.join(FIXTURES, '*.json')].sort.first
  actual = JSON.parse(File.read(sample))[tree].keys.to_set

  (actual - documented).sort.map { |k| "文書全体: 仕様に無いキー `#{k}`" } +
    (documented - actual).sort.map { |k| "文書全体: 仕様にあるが出ないキー `#{k}`" }
end

def report(tree, spec_path)
  spec = spec_constructors(spec_path)
  actual = collect_actual[tree]
  problems = envelope_problems(tree, spec_path)

  if (top = top_level_fields(TOP_LEVEL[tree]))
    name, fields = top
    if spec.key?(name)
      problems.concat(shape_problems(name, spec[name], fields))
    else
      problems << "#{name}: 仕様に記述が無い"
    end
  end

  actual.sort.each do |name, field_sets|
    unless spec.key?(name)
      problems << "#{name}: 実装が吐いているが仕様に無い"
      next
    end
    field_sets.each { |fields| problems.concat(shape_problems(name, spec[name], fields)) }
  end

  uncovered = spec.keys - actual.keys - [TOP_LEVEL[tree][0]].compact
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
    puts "  （フィクスチャに現れない構成子: #{uncovered.join(' ')}）" unless uncovered.empty?
    puts
  end
  warn '仕様と実装が食い違っています。' unless status.zero?
  exit status
end

main if __FILE__ == $PROGRAM_NAME
