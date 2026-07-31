#!/usr/bin/env ruby
# frozen_string_literal: true

# 青空文庫・外字注記辞書（PDF）を機械可読な TSV にする。
#
#   https://www.aozora.gr.jp/gaiji_chuki/gaiji_chuki.pdf
#   青空文庫外字注記辞書編集グループ・改訂第八版訂正版（2011-08-06）
#   利用条件は青空文庫本体と同じ。
#
#   ruby tools/extract_gaiji_chuki.rb gaiji_chuki.pdf crates/aozora-core/data
#
# poppler-utils（pdftotext・pdftocairo）が要る。
#
# **表示字形は文字として取らない。** PDF の外字は ToUnicode を持たない埋め込みサブセット
# フォント（GT2000・花園明朝など）で描かれており、pdftotext は BMP 外の字を別の字に
# 取り違える（実測 4021 件中 186 件が注記中の U+ と食い違い、うち 141 件が BMP 外）。
# 正は注記本文——`※［＃「説明」、第3水準 1-15-15］`——であり、これはかなと常用漢字と
# ASCII だけなので確実に取れる。
#
# **符号位置を持たない字だけ、字形をベクタで残す。** 面区点や U+ があれば字形は符号位置で
# 決まるのでフォントから描ける。辞書の字形が唯一の情報源なのは、コードを持たない
# 1139 件——包摂適用・デザイン差・統合適用、つまり「符号位置が同じで字形だけ違う」
# 例示字形の問題そのもの——に限られる。そこだけ pdftocairo の SVG からアウトラインを
# 取り出す。色（赤＝包摂される字／黒＝包摂する字／緑＝2004 で例示字形変更／水色＝2004
# 追加）は fill 属性としてそのまま残る。
#
# 出力:
#   gaiji_chuki.tsv         1 行 1 エントリ（後述の列）
#   gaiji_chuki_glyphs.tsv  符号位置を持たない字の輪郭。1 行 1 パーツ（id・part・fill・
#                           dx・d）。1 文字が複数パーツで組まれることがあるので、
#                           同じ id の行を dx だけ右にずらして重ねると 1 文字になる。
#
# gaiji_chuki.tsv の列:
#   id       エントリの識別子（p<ページ>-<ページ内連番>）
#   page     PDF のページ番号
#   radical  部首（節見出し 「いち【 一 】」 の 一）
#   strokes  画数
#   desc     注記の説明（「…」の中身）。両方向の結合キー
#   jis      面区点（例 1-15-15。無ければ空）
#   level    水準（3 または 4。無ければ空）
#   unicode  U+ の値（例 55DB。無ければ空）
#   sub      代用してよい字（→［包摂適用 …］等。無ければ空）
#   sub_kind 代用の種別（包摂適用/統合適用/デザイン差/78互換包摂）
#   sub_rule 包摂規準・UCV の番号
#   cross    別部首への再掲（★）なら 1
#   glyph    1   = gaiji_chuki_glyphs.tsv に輪郭がある
#            pdf = 符号位置が無く輪郭も PDF から取れない（page を見るしかない）
#            空  = 符号位置で字形が決まる（フォントから描ける）

require 'rexml/document'
require 'open3'
require 'tmpdir'

PDF = ARGV[0] or abort 'usage: extract_gaiji_chuki.rb <pdf> <outdir>'
OUTDIR = ARGV[1] or abort 'usage: extract_gaiji_chuki.rb <pdf> <outdir>'

SUB_KINDS = '包摂適用|統合適用|デザイン差|78ï¼?互換包摂|78 ?互換包摂'
XHTML = 'http://www.w3.org/1999/xhtml'
SVGNS = 'http://www.w3.org/2000/svg'

# --- PDF から読む ---------------------------------------------------------

Word = Struct.new(:text, :x0, :y0, :x1, :y1)

# ページの単語を行ごとに返す。`-layout` のテキストではなく bbox を使うのは、字形を
# 置いてある位置を知るため（SVG 側の `<use x y>` と突き合わせる）。
def page_lines(page)
  xml, = Open3.capture2('pdftotext', '-f', page.to_s, '-l', page.to_s, '-bbox-layout', PDF, '-')
  # ToUnicode を持たない外字は制御文字（U+001E 等）として出てくる。XML としては不正なので
  # 落とす。位置だけ使うので、字そのものが読めなくてよい。
  doc = REXML::Document.new(xml.gsub(/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/, ''))
  REXML::XPath.match(doc, "//*[local-name()='line']").map do |line|
    REXML::XPath.match(line, "*[local-name()='word']").map do |w|
      Word.new(w.text.to_s, w['xMin'].to_f, w['yMin'].to_f, w['xMax'].to_f, w['yMax'].to_f)
    end
  end
end

# ページのグリフ輪郭。`<defs>` の `<g id="glyph-N-M"><path d>` を `<use x y>` が参照する。
def page_glyphs(page)
  tmp = File.join(Dir.tmpdir, "gaiji_p#{page}.svg")
  system('pdftocairo', '-svg', '-f', page.to_s, '-l', page.to_s, PDF, tmp, out: File::NULL,
                                                                          err: File::NULL)
  doc = REXML::Document.new(File.read(tmp))
  defs = {}
  REXML::XPath.match(doc, "//*[local-name()='g'][@id]").each do |g|
    next unless g['id'].start_with?('glyph-')

    path = REXML::XPath.first(g, "*[local-name()='path']")
    defs[g['id']] = path['d'] if path
  end
  uses = []
  walk = lambda do |el, fill|
    f = el.attributes['fill'] || fill
    el.elements.each do |c|
      if c.name == 'use'
        href = (c.attributes['xlink:href'] || c.attributes['href']).to_s.delete('#')
        uses << [c.attributes['x'].to_f, c.attributes['y'].to_f, href, f]
      end
      walk.call(c, f)
    end
  end
  walk.call(doc.root, nil)
  File.delete(tmp) if File.exist?(tmp)
  [defs, uses]
end

# --- エントリを組み立てる -------------------------------------------------

def parse_entry(text)
  m = text.match(/\A\s*(★\s*)?(\d+)．\s*\S*?\s*(※［＃.*|→［.*)\z/m)
  return nil unless m

  star, strokes, rest = m[1], m[2].to_i, m[3]
  spec = rest[/※［＃([^］]*)］/, 1]
  # 代用字は 1 文字とは限らない。字形が符号化されていない字を代用に指す場合、代用字
  # 自体が外字注記になる（`→［包摂適用 ※［＃「（冫＋臣＋犯のつくり）／れんが」、
  # 第3水準1-87-58］］`）。注記のときは ］ が入れ子になるので先に試す。
  sub = rest.match(/→［(#{SUB_KINDS})\s*(※［＃[^］]*］)\s*］\s*([0-9A-Za-z]*)/) ||
        rest.match(/→［(#{SUB_KINDS})\s*([^］\s]+)\s*］\s*([0-9A-Za-z]*)/)
  return nil if spec.nil? && sub.nil?

  spec = spec.to_s.gsub(/、?ページ数-行数/, '').gsub(/\s+/, '')
  {
    strokes: strokes,
    desc: spec[/\A「([^」]*)」/, 1] || '',
    jis: spec[/第([34])水準([0-9]+-[0-9]+-[0-9]+)/, 2] || '',
    level: spec[/第([34])水準/, 1] || '',
    unicode: spec[/U\+([0-9A-F]{4,6})/, 1] || '',
    sub: sub ? sub[2] : '',
    sub_kind: sub ? sub[1].tr('ï¼', '').delete(' ') : '',
    sub_rule: sub ? sub[3] : '',
    cross: star ? '1' : ''
  }
end

# 注記が閉じないまま行が終わったら次の行と繋ぐ。字形は最初の行にあるので bbox は
# 引き継ぐ。
def entries_on_page(page)
  radical = nil
  pending = nil
  out = []
  page_lines(page).each do |words|
    text = words.map(&:text).join
    if (m = text.match(/\A\s*(\S+?)【\s*(.+?)\s*】\s*部首・読み索引に戻る/))
      radical = m[2].split(/\s+/).first
      next
    end
    # 字形は 「11．」 と注記本文の間にある。位置で決め打ちしないのは、★（別部首への
    # 再掲）で 1 つずれることと、字送りが詰まると pdftotext が 「．」 と字形を 1 単語に
    # まとめてしまうため（`．冫虫`）。だから x の範囲で挟む。
    dot = words.find { |w| w.text.include?('．') }
    note = words.find { |w| w.text.start_with?('※', '→') }
    glyph = dot && note ? [dot.x0 + 6.0, note.x0 - 0.5, dot.y0, dot.y1] : nil
    if pending
      text = pending[:text] + text.strip
      glyph = pending[:glyph]
      pending = nil
    end
    if text =~ /\A\s*(?:★\s*)?\d+．/ && text.include?('※［＃') &&
       !text.split('※［＃').last.include?('］')
      pending = { text: text, glyph: glyph }
      next
    end
    entry = parse_entry(text)
    next unless entry

    out << entry.merge(radical: radical, glyph_box: glyph)
  end
  out
end

# --- 出力 -----------------------------------------------------------------

COLUMNS = %i[id page radical strokes desc jis level unicode sub sub_kind sub_rule cross
             glyph].freeze

entries = []
glyphs = []
(1..Integer(`pdfinfo #{PDF}`[/Pages:\s*(\d+)/, 1])).each do |page|
  found = entries_on_page(page)
  next if found.empty?

  defs = uses = nil
  found.each_with_index do |e, i|
    e[:id] = "p#{page}-#{i}"
    e[:page] = page
    e[:glyph] = ''
    box = e.delete(:glyph_box)
    # 符号位置を持つものは字形をフォントから描けるので残さない。
    next if box.nil? || !e[:jis].empty? || !e[:unicode].empty?

    # 符号位置が無いのに字形も取り出せないことがある。その 82 件は非埋め込みフォント
    # （Ryumin-Light）で描かれており、アウトラインもテキストも PDF に入っていない
    # （ラスタライズしても空白になる）。取り繕わず `pdf` と記録して page 列に委ねる。
    e[:glyph] = 'pdf'
    defs, uses = page_glyphs(page) if defs.nil?
    x0, x1, y0, y1 = box
    # 1 文字が複数のパーツで描かれていることがある（単一のフォントに字が無く、部品を
    # 並べて組んでいる）。範囲に入る `<use>` をすべて拾い、先頭からの相対位置で持つ。
    parts = uses.select { |x, y, ref, _| x >= x0 && x <= x1 && y > y0 - 2 && y < y1 + 0.5 && defs[ref] }
                .sort_by(&:first)
    next if parts.empty?

    origin = parts.first[0]
    e[:glyph] = '1'
    parts.each_with_index do |(x, _, ref, fill), n|
      glyphs << { id: e[:id], part: n, fill: fill || 'black', dx: (x - origin).round(3),
                  d: defs[ref] }
    end
  end
  entries.concat(found)
  warn "p#{page}: #{found.size} 件（字形 #{found.count { |e| e[:glyph] == '1' }}）" if page % 20 == 0
end

File.open(File.join(OUTDIR, 'gaiji_chuki.tsv'), 'w') do |f|
  f.puts COLUMNS.join("\t")
  entries.each { |e| f.puts COLUMNS.map { |c| e[c].to_s }.join("\t") }
end
File.open(File.join(OUTDIR, 'gaiji_chuki_glyphs.tsv'), 'w') do |f|
  f.puts %w[id part fill dx d].join("\t")
  glyphs.each { |g| f.puts [g[:id], g[:part], g[:fill], g[:dx], g[:d]].join("\t") }
end
warn "エントリ #{entries.size} / 字形 #{glyphs.map { |g| g[:id] }.uniq.size}"\
     "（パーツ #{glyphs.size}）"
