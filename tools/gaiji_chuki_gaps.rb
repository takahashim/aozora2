#!/usr/bin/env ruby
# frozen_string_literal: true

# 外字注記辞書のうち「字が同定できていない」エントリを段階分けして出す。
#
#   ruby tools/gaiji_chuki_gaps.rb [crates/aozora-core/data/gaiji_chuki.tsv] [--all]
#
# 同定の強さは
#   符号位置（jis / unicode）> 代用字（sub）> 字形（ivs / cid / glyph）
#   > 別字形の親字（ids が 1 文字）> 組み立て式（ids が 2 文字以上）> 説明文だけ
# の順。上ほど字形が一意に決まる。既定では下 3 段だけを並べる。

require 'set'
require_relative 'gaiji_chuki_data'

path = ARGV.find { |a| !a.start_with?('-') } ||
       File.expand_path('../crates/aozora-core/data/gaiji_chuki.tsv', __dir__)
all = ARGV.include?('--all')

rows = GaijiChuki.read_tsv(path)
val = ->(row, name) { row.fetch(name).to_s }
have = ->(row, name) { !val.call(row, name).empty? }

# ids が 1 文字なら組み立てではなく「その字の別字形」の意味（gaiji_chuki.md 参照）。
tier = lambda do |row|
  ids = val.call(row, 'ids')
  if have.call(row, 'jis') || have.call(row, 'unicode') then :code
  elsif have.call(row, 'sub') then :sub
  elsif have.call(row, 'ivs') || have.call(row, 'cid') || have.call(row, 'glyph') then :shape
  elsif ids.length == 1 then :parent
  elsif have.call(row, 'ids_char') then :ids_char
  elsif !ids.empty? then :ids_only
  else :desc_only
  end
end

LABELS = {
  code: '符号位置あり（面区点 or U+）',
  sub: '代用字あり（包摂適用・デザイン差など）',
  shape: '字形情報あり（IVS / CID / 輪郭）',
  ids_char: 'IDS から字を引けた',
  parent: '別字形——親字だけ分かる（ids が 1 文字）',
  ids_only: '組み立て式だけ（IDS で字を引けず）',
  desc_only: '説明文しかない'
}.freeze

grouped = rows.group_by(&tier)
puts "#{path}: #{rows.size} 件"
LABELS.each_key do |key|
  printf("  %-40s %5d\n", LABELS[key], grouped.fetch(key, []).size)
end

show = all ? LABELS.keys : %i[parent ids_only desc_only]
show.each do |key|
  entries = grouped.fetch(key, [])
  next if entries.empty?

  puts "\n=== #{LABELS[key]} (#{entries.size}) ==="
  entries.each do |row|
    printf("  %-9s p%-4s %-3s %2s画  ids=%-12s %s\n",
           val.call(row, 'id'), val.call(row, 'page'), val.call(row, 'radical'),
           val.call(row, 'strokes'), val.call(row, 'ids'), val.call(row, 'desc'))
  end
end

# 全列が空同然のもの——抽出漏れであって「符号位置が無い字」ではない。
meaty = %w[desc jis level unicode sub sub_kind sub_rule cross ivs cid glyph ids ids_char glyphwiki]
broken = rows.reject { |row| meaty.any? { |k| !row[k].to_s.empty? } }
return if broken.empty?

puts "\n=== 抽出漏れの疑い（desc 以降が全部空） (#{broken.size}) ==="
broken.each { |row| puts "  #{val.call(row, 'id')} p#{val.call(row, 'page')} #{val.call(row, 'radical')}" }
