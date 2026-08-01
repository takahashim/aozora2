#!/usr/bin/env ruby
# frozen_string_literal: true

# 青空文庫・外字注記辞書（PDF）を機械可読な TSV にする。
#
#   https://www.aozora.gr.jp/gaiji_chuki/gaiji_chuki.pdf
#   青空文庫外字注記辞書編集グループ・改訂第八版訂正版（2011-08-06）
#   利用条件は青空文庫本体と同じ。
#
# データの構成と、取り込みで引っかかったことは
# crates/aozora-core/data/gaiji_chuki.md。
#
#   curl -O https://www.unicode.org/ivd/data/2022-09-13/IVD_Sequences.txt
#   ruby tools/extract_gaiji_chuki.rb gaiji_chuki.pdf IVD_Sequences.txt crates/aozora-core/data
#
# poppler-utils（pdftotext・pdftocairo）が要る。PDF 自体の読み取りは tools/mini_pdf.rb
# （標準ライブラリのみ）。
#
# **表示字形は文字として取らない。** PDF の外字は ToUnicode を持たない埋め込みサブセット
# フォント（GT2000・花園明朝など）で描かれており、pdftotext は BMP 外の字を別の字に
# 取り違える（実測 4021 件中 186 件が注記中の U+ と食い違い、うち 141 件が BMP 外）。
# 正は注記本文——`※［＃「説明」、第3水準 1-15-15］`——であり、これはかなと常用漢字と
# ASCII だけなので確実に取れる。
#
# **字形の同定は content stream の CID で行う。** 字形欄は埋め込みフォント（GT2000 など）と
# 非埋め込みフォント（Ryumin-Light）が混在し、後者は poppler が代替フォントで描けず
# .notdef（□に×）になる。実測で 1045 件中 644 件がこれで、輪郭を取っても意味が無い。
# 一方 PDF には「どの CID を描くか」が書かれており、Ryumin-Light は Adobe-Japan1 なので
# Unicode の IVD（Adobe-Japan1 コレクション）で異体字セレクタ列に落ちる。字形の同定は
# そちらで行う（tools/extract_gaiji_cids.py）。
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
#                           dx・dy・d・ivs・box）。組み立ては tools/gaiji_glyph_resolver.rb。字形は「部品を描いては白い矩形で消す」
#                           手順で組んであるので、行の順に dx・dy だけずらして重ねる。
#                           白い塗りつぶし（マスク）も 1 行として入っている。
#                           非埋め込みの Ryumin で描かれたパーツは poppler が別の字を
#                           描いてしまうので、d を空にして ivs（異体字セレクタ列）を持つ。
#                           表示側はそれを文字として置く。
#
# gaiji_chuki.tsv の列:
#   id       エントリの識別子（p<ページ>-<ページ内連番>）
#   page     PDF のページ番号
#   radical  部首（節見出し 「いち【 一 】」 の 一）
#   strokes  画数
#   desc     注記の説明の全文。両方向の結合キーなので、`「尅」の「寸」に代えて「土」`
#            のように外側にもう一段ある書き方も切り詰めずに持つ
#   jis      面区点（例 1-15-15。無ければ空）
#   level    水準（3 または 4。無ければ空）
#   unicode  U+ の値（例 55DB。無ければ空）
#   sub      代用してよい字（→［包摂適用 …］等。無ければ空）
#   sub_kind 代用の種別（包摂適用/統合適用/デザイン差/78互換包摂）
#   sub_rule 包摂規準・UCV の番号
#   cross    別部首への再掲（★）なら 1
#   ivs      字形の Adobe-Japan1 異体字セレクタ列（例 `51E1 E0101`）
#   cid      字形の CID（ivs が引けなかったときの手がかり）
#   glyph    1   = gaiji_chuki_glyphs.tsv に輪郭がある（埋め込みフォントで描かれた字）
#            空  = 符号位置で字形が決まるか、字形欄が空

require 'rexml/document'
require 'open3'
require_relative 'pdf_text_runs'
require_relative 'gaiji_ids'
require_relative 'gaiji_glyph_resolver'

USAGE = 'usage: extract_gaiji_chuki.rb <pdf> <IVD_Sequences.txt> <ids.txt> <outdir>'
PDF = ARGV[0] or abort USAGE
IVD_PATH = ARGV[1] or abort USAGE
IDS_PATH = ARGV[2] or abort USAGE
OUTDIR = ARGV[3] or abort USAGE

SUB_KINDS = '包摂適用|統合適用|デザイン差|78ï¼?互換包摂|78 ?互換包摂'

# --- PDF から読む ---------------------------------------------------------

Word = Struct.new(:text, :x0, :y0, :x1, :y1)

# ページの単語を行ごとに返す。`-layout` のテキストではなく bbox を使うのは、字形を
# 置いてある位置を知るため（SVG 側の `<use x y>` と突き合わせる）。
#
# **poppler の <line> は使わない。** 組版の隙間で 1 行が切れる。`2． 丈 →［デザイン差 丈］
# UCV235` は同じ y 帯にありながら `2．丈` / `→［デザイン差` / `丈］UCV235` の 3 つに分かれ、
# しかも <line> の並び順は y 順ですらない。そのまま読むと注記の始まらない行になって
# エントリごと落ちる（15 件ほどそうなっていた）。単語を y の帯でまとめ直す。
def page_lines(page)
  xml, = Open3.capture2('pdftotext', '-f', page.to_s, '-l', page.to_s, '-bbox-layout', PDF, '-')
  # ToUnicode を持たない外字は制御文字（U+001E 等）として出てくる。XML としては不正なので
  # 落とす。位置だけ使うので、字そのものが読めなくてよい。
  doc = REXML::Document.new(xml.gsub(/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/, ''))
  words = REXML::XPath.match(doc, "//*[local-name()='word']").map do |w|
    Word.new(w.text.to_s, w['xMin'].to_f, w['yMin'].to_f, w['xMax'].to_f, w['yMax'].to_f)
  end
  bands(words)
end

# 単語を横の帯にまとめる。中心が今の帯の下端より上なら同じ行と見なす。字形は本文より
# 背が高いことがあるので、下端は伸ばしながら見る。
def bands(words)
  out = []
  words.sort_by { |w| [(w.y0 + w.y1) / 2, w.x0] }.each do |w|
    if out.last && (w.y0 + w.y1) / 2 < out.last[:bottom]
      out.last[:words] << w
      out.last[:bottom] = [out.last[:bottom], w.y1].max
    else
      out << { bottom: w.y1, words: [w] }
    end
  end
  out.map { |b| b[:words].sort_by(&:x0) }
end

# --- エントリを組み立てる -------------------------------------------------

# 先頭の 「 に対応する 」 の位置。入れ子があるので数える。閉じが無ければ -1。
def matching_quote(str)
  depth = 0
  str.each_char.with_index do |c, i|
    depth += 1 if c == '「'
    if c == '」'
      depth -= 1
      return i if depth.zero?
    end
  end
  -1
end

# 注記を「説明」と「符号位置」に割る。説明は**全文**を残す。`「尅」の「寸」に代えて「土」`
# の 「尅」 だけを採ると、尅 そのものを指す別のエントリと同じ desc になってしまう
# （そういう取りこぼしが 128 種あった）。desc は結合キーなので、一意に読める形で持つ。
#
# 外側の 「」 は、それが全体を包んでいるときだけ外す（`「口＋畢」` → `口＋畢`）。上のように
# 外側にもう一段ある書き方では、包んでいないので全文がそのまま残る。
#
# 面区点と U+ を末尾からだけ取るのは、説明が別の字の注記を引用することがあるため。
# `「馬＋「柳の本字、第4水準2-14-72」のつくり」、U+99F5` の 2-14-72 は引用された柳の本字の
# もので、この字（U+99F5）のものではない。
def split_spec(spec)
  code = spec[/、(?:第[34]水準[0-9-]+|U\+[0-9A-F]{4,6})\z/] || ''
  desc = spec[0...(spec.length - code.length)]
  # 辞書 PDF の誤植で 」 が落ちている注記が 12 件ある（`※［＃「りっしんべん＋乞、
  # 第 4水準 2-12-32］`）。区切りの 、 が残るので落とし、開いたままの 「 も外す。
  desc = desc.sub(/、\z/, '')
  return [desc, code] unless desc.start_with?('「')

  close = matching_quote(desc)
  desc = close == desc.length - 1 ? desc[1...-1] : (close.negative? ? desc[1..] : desc)
  [desc, code]
end

# 規準番号は代用字の ］ の後ろにある。ただし直後とは限らず `補助漢字と共通 83` のように
# 語が挟まるし、`128、143` と複数並ぶこともある。数字・英字が現れるまで読み飛ばして、
# 、で続く分まで取る。番号を持たない代用字（78互換包摂の 29 件など）では空になる。
def rule_after(tail)
  tail[/\A[^0-9A-Za-z]*([0-9A-Za-z]+(?:、[0-9A-Za-z]+)*)/, 1].to_s
end

def parse_entry(text)
  m = text.match(/\A\s*(★\s*)?(\d+)．\s*\S*?\s*(※［＃.*|→［.*)\z/m)
  return nil unless m

  star, strokes, rest = m[1], m[2].to_i, m[3]
  spec = rest[/※［＃([^］]*)］/, 1]
  # 代用字は 1 文字とは限らない。字形が符号化されていない字を代用に指す場合、代用字
  # 自体が外字注記になる（`→［包摂適用 ※［＃「（冫＋臣＋犯のつくり）／れんが」、
  # 第3水準1-87-58］］`）。注記のときは ］ が入れ子になるので先に試す。
  sub = rest.match(/→［(#{SUB_KINDS})\s*(※［＃[^］]*］)\s*］/) ||
        rest.match(/→［(#{SUB_KINDS})\s*([^］\s]+)\s*］/)
  return nil if spec.nil? && sub.nil?

  sub_text = sub ? sub[2] : ''
  sub_rule = sub ? rule_after(sub.post_match) : ''
  # `→［包摂適用 掲 150］` と、番号が括弧の内側に入っている行が 1 つある。単語の間の空白は
  # 落ちているので、代用字の末尾に数字が付いていたら番号として切り出す。
  if sub_rule.empty? && !sub_text.start_with?('※') && (m2 = sub_text.match(/\A(.+?)(\d+(?:、\d+)*)\z/))
    sub_text, sub_rule = m2[1], m2[2]
  end

  spec = spec.to_s.gsub(/、?ページ数-行数/, '').gsub(/\s+/, '')
  desc, code = split_spec(spec)
  {
    strokes: strokes,
    desc: desc,
    jis: code[/第([34])水準([0-9]+-[0-9]+-[0-9]+)/, 2] || '',
    level: code[/第([34])水準/, 1] || '',
    unicode: code[/U\+([0-9A-F]{4,6})/, 1] || '',
    sub: sub_text,
    sub_kind: sub ? sub[1].tr('ï¼', '').delete(' ') : '',
    sub_rule: sub_rule,
    cross: star ? '1' : ''
  }
end

# 注記が閉じないまま行が終わったら次の行と繋ぐ。字形は最初の行にあるので bbox は
# 引き継ぐ。
# 部首の見出しは「その部首が始まるページ」にしか無い。ページごとに捨てるとほとんどの
# エントリで部首が空になるので、呼び出し側が持つ状態を受け渡す。
def entries_on_page(page, state)
  pending = nil
  out = []
  page_lines(page).each do |words|
    text = words.map(&:text).join
    if (m = text.match(/\A\s*(\S+?)【\s*(.+?)\s*】\s*部首・読み索引に戻る/))
      # 【 己 已 巳 】のように異体を並べた見出しがある。単語 bbox から組み立てると
      # 空白が落ちるので、split ではなく先頭の 1 文字を取る。
      state[:radical] = m[2][0]
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

    out << entry.merge(radical: state[:radical], glyph_box: glyph)
  end
  out
end

# --- 出力 -----------------------------------------------------------------

COLUMNS = %i[id page radical strokes desc jis level unicode sub sub_kind sub_rule cross
             ivs cid glyph ids ids_char glyphwiki].freeze

pdf = MiniPdf.new(PDF)
ivd = PdfTextRuns.load_ivd(IVD_PATH)
ids_to_code, ids_of_code = GaijiIds.load_table(IDS_PATH)
ids_conv = GaijiIds::Converter.new(ids_of_code, ids_to_code)
entries = []
glyphs = []
state = { radical: nil }
pdf.pages.each_with_index do |pdf_page, page_index|
  page = page_index + 1
  found = entries_on_page(page, state)
  next if found.empty?

  resolver = GaijiGlyphResolver.new(pdf, pdf_page, page, PDF, ivd)
  found.each_with_index do |e, i|
    e[:id] = "p#{page}-#{i}"
    e[:page] = page
    e[:ivs] = e[:cid] = e[:glyph] = ''
    box = e.delete(:glyph_box)
    # 符号位置を持つものは字形が符号位置で決まるので、同定の手間をかけない。
    next if box.nil? || !e[:jis].empty? || !e[:unicode].empty?

    glyph = resolver.resolve(box) or next

    e[:ivs] = glyph.ivs
    e[:cid] = glyph.cid
    next if glyph.parts.empty?

    e[:glyph] = '1'
    glyph.parts.each { |part| glyphs << part.merge(id: e[:id]) }
  end
  # 説明文から組み立ての IDS を導く。符号位置の有無に関係なく引けるので、同定の
  # 独立した手がかりになる（符号位置を持つエントリでの正解率 98.1%）。
  found.each do |e|
    e[:ids] = e[:ids_char] = e[:glyphwiki] = ''
    next if e[:desc].to_s.empty?

    # 「に代えて」は 2 通りある。符号位置があるなら字は決まっていて、これは字形の細部の
    # 注記（`「てへん＋那」の「二」に代えて「はみ出た横棒二本」、U+632A` の字は 挪 のまま）。
    # 符号位置が無いときだけ、組み立てを変える指定として効かせる。
    coded = !e[:jis].empty? || !e[:unicode].empty?
    begin
      ids = ids_conv.convert(e[:desc], replace: !coded)
    rescue GaijiIds::Unconvertible
      next
    end
    e[:ids] = ids
    # 1 文字に落ちたものは `ids_char` に入れない。説明が 1 文字なら「その字の別字形」の
    # 意味で組み立てではないし、引き算の結果が 1 文字になった場合（`朽のつくり` → 丂）も、
    # 符号位置を持つ 283 件で照合すると一致は 79% しかない——辞書がこれらを外字として
    # 載せているのは、まさに部品そのものとは少し違う字形だからで、当てにならない。
    next if ids.length == 1

    e[:ids_char] = ids_conv.char_of(ids).to_s
    e[:glyphwiki] = GaijiIds.glyphwiki_name(ids)
  end
  entries.concat(found)
  warn "p#{page}: #{found.size} 件" if page % 20 == 0
end

File.open(File.join(OUTDIR, 'gaiji_chuki.tsv'), 'w') do |f|
  f.puts COLUMNS.join("\t")
  entries.each { |e| f.puts COLUMNS.map { |c| e[c].to_s }.join("\t") }
end
File.open(File.join(OUTDIR, 'gaiji_chuki_glyphs.tsv'), 'w') do |f|
  f.puts %w[id part fill dx dy d ivs box].join("\t")
  glyphs.each { |g| f.puts g.values_at(:id, :part, :fill, :dx, :dy, :d, :ivs, :box).join("\t") }
end
warn "エントリ #{entries.size} / IVS #{entries.count { |e| !e[:ivs].to_s.empty? }}"\
     " / 輪郭 #{glyphs.map { |g| g[:id] }.uniq.size}"\
     " / IDS #{entries.count { |e| !e[:ids].to_s.empty? }}"\
     " / IDS から字 #{entries.count { |e| !e[:ids_char].to_s.empty? }}"
