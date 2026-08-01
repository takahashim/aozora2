#!/usr/bin/env ruby
# frozen_string_literal: true

# 外字注記辞書の TSV・HTML が PDF と食い違っていないかを見る。
#
#   ruby tools/verify_gaiji_chuki.rb <gaiji_chuki.pdf> [--html tmp/gaiji_chuki.html] [-v]
#
# 抽出器（tools/extract_gaiji_chuki.rb）を通さずに確かめるのが要点。抽出器のロジックを
# 使い回して検算しても、同じ思い違いをなぞるだけで意味が無い。ここでは逆向きに、**TSV の
# 列から注記を組み直して、それが PDF の本文に出てくるか**を見る。desc・面区点・水準・U+・
# 代用字とその種別・規準番号が一度に効くので、1 文字でもずれれば落ちる。
#
#   TSV:  desc=尅の寸に代えて土  unicode=99F5 …
#   組み直し:  ※［＃「尅」の「寸」に代えて「土」、ページ数-行数］
#   PDF p34:   7． ※［＃「尅」の「寸」に代えて「土」、ページ数-行数］     ← 出てくれば ✓
#
# 6 つ見る。
#
#   1. 件数    PDF のエントリの頭（★ と画数）と、TSV の cross・strokes が合うか
#   2. 部首    TSV の部首の並びが PDF の節見出しの順に沿っているか
#   3. 復元    上のとおり。合わないものは中身を出す
#   4. 付合    輪郭・MJ の TSV が id で正しい行を指しているか
#   5. ivs     基底字が字全体を指しているか（部品を拾っていないか）
#   6. HTML    --html を渡したとき、TSV の全 id が記事として出ているか
#
# 注記の復元は tools/build_gaiji_chuki_html.rb と同じ規則。あちらが表示のために使い、
# こちらが検証に使う。

require 'optparse'
require 'open3'

DATA = File.expand_path('../crates/aozora-core/data', __dir__)

def read_tsv(path)
  rows = File.readlines(path, chomp: true).map { |l| l.split("\t", -1) }
  head = rows.shift
  rows.map { |r| head.zip(r).to_h }
end

# TSV に持たない飾りは両側から落として比べる。
#
#   ページ数-行数    「ここに底本のページ-行を書く」という指示。付け方が辞書側で揺れている
#                  （符号位置も代用字も無い 232 件は 162 対 70）
#   補助のみ/補助漢字と共通  補助漢字での扱いを示す注記。代用字の ］ と規準番号の間に挟まる
def normalize(str)
  str.gsub(/\s+/, '').gsub('、ページ数-行数', '').gsub(/補助のみ|補助漢字と共通/, '')
end

# PDF のページ本文。pdftotext の空白の入れ方は版で揺れるので、比較前に全部落とす。
def page_text(pdf, page)
  out, _err, st = Open3.capture3('pdftotext', '-f', page.to_s, '-l', page.to_s, '-layout', pdf, '-')
  st.success? ? normalize(out) : nil
end

# TSV の列から注記を組み直す。build_gaiji_chuki_html.rb と同じ規則:
# 説明は全文で、外側にもう一段ある書き方（`「尅」の「寸」に代えて「土」`）は括り直さない。
# `ページ数-行数` は面区点が無いときだけ付く。
def note(e)
  return nil if e['desc'].empty?

  body = e['desc'].start_with?('「') ? e['desc'] : "「#{e['desc']}」"
  tail = if !e['jis'].empty?
           "、第#{e['level']}水準#{e['jis']}"
         elsif !e['unicode'].empty?
           "、U+#{e['unicode']}"
         else
           ''
         end
  normalize("※［＃#{body}#{tail}］")
end

def substitution(e)
  return nil if e['sub'].empty?

  normalize("→［#{e['sub_kind']}#{e['sub']}］#{e['sub_rule']}")
end

# PDF のページの生の行。エントリは「画数．」で始まる（★ は別部首からの再掲）。
def page_lines(pdf, page)
  out, _err, st = Open3.capture3('pdftotext', '-f', page.to_s, '-l', page.to_s, '-layout', pdf, '-')
  st.success? ? out.lines : []
end

# PDF はエントリの頭に ★ と画数を印字している（`★ 16． 坪`）。そこを直に読めば、件数と
# `cross`・`strokes` を一度に照合できる。字形が複数行にわたるときは ★ と注記が別の行に
# なるので、注記の有無では数えない——`9． 煕熙熈 入力可能` だけを外す。
def pdf_heads(pdf, page)
  page_lines(pdf, page).filter_map do |l|
    next if l.include?('入力可能')

    m = l.match(/\A\s*(★\s*)?(\d+)．/) or next
    [m[1] ? '1' : '', m[2].to_i]
  end
end

def check_counts(entries, pdf, verbose)
  puts '== 件数・★・画数（PDF のエントリの頭 vs TSV）=='
  bad = []
  entries.group_by { |e| e['page'].to_i }.sort.each do |page, list|
    want = pdf_heads(pdf, page).sort
    have = list.map { |e| [e['cross'], e['strokes'].to_i] }.sort
    bad << [page, want, have] if want != have
    puts "  p#{page}: PDF #{want.size} / TSV #{have.size}" if verbose
  end
  bad.each do |page, want, have|
    puts "  ✗ p#{page}: PDF #{want.size} 件 / TSV #{have.size} 件"
    puts "      PDF のみ: #{(want - have).inspect}" unless (want - have).empty?
    puts "      TSV のみ: #{(have - want).inspect}" unless (have - want).empty?
  end
  puts "  ページ #{entries.map { |e| e['page'] }.uniq.size} 中 #{bad.size} 不一致"
  bad.size
end

# 部首は節見出しから状態として引き継ぐので、1 つ取りこぼすと以降がまとめてずれる。
# TSV に出る部首の順序が、PDF の見出しの順序の部分列になっているかを見る。
# 見出しがあってもエントリが 0 件の節はある（釆 は `0． 釆 采 入力可能` の 1 行だけ）
# ので、部分列であればよい。
def check_radicals(entries, pdf)
  puts '== 部首（TSV の並びが PDF の見出しの順に沿っているか）=='
  headings = entries.map { |e| e['page'].to_i }.uniq.flat_map do |page|
    page_lines(pdf, page).filter_map { |l| l[/【\s*(.+?)\s*】\s*部首・読み索引に戻る/, 1]&.slice(0) }
  end
  seen = entries.map { |e| e['radical'] }.chunk_while { |a, b| a == b }.map(&:first)
  rest = headings.dup
  stray = seen.reject { |r| (i = rest.index(r)) && rest = rest[(i + 1)..] }
  puts "  見出し #{headings.size} / TSV の部首の切り替わり #{seen.size} / 順に沿わないもの #{stray.size}"
  stray.first(10).each { |r| puts "    ✗ #{r}" }
  stray.size
end

def check_notes(entries, pdf, verbose)
  puts '== 復元（TSV から組み直した注記が PDF 本文に出てくるか）=='
  soft = []
  hard = []
  cache = {}
  entries.each do |e|
    page = e['page'].to_i
    text = (cache[page] ||= page_text(pdf, page) || '')
    [note(e), substitution(e)].compact.each do |s|
      next if text.include?(s)

      # 説明自体は出てくるなら、崩れているのは括りや並びのほう。PDF が注記を 2 行に
      # 折り返していると線形化で順序が入れ替わるし、辞書側の誤植（閉じ 」 落ち）でも
      # ここに落ちる。説明ごと見つからないなら、それは本当の食い違い。
      (text.include?(normalize(e['desc'])) ? soft : hard) << [e, s]
    end
  end
  hard.each do |e, s|
    puts "  ✗ #{e['id']} p#{e['page']} 説明ごと見つからない"
    puts "      組み直し: #{s}"
  end
  soft.each do |e, s|
    puts "  ? #{e['id']} p#{e['page']} 説明は出てくるが注記の形が違う（折り返し・誤植）"
    puts "      組み直し: #{s}"
  end if verbose || soft.size <= 30
  total = entries.sum { |e| [note(e), substitution(e)].compact.size }
  puts "  #{total} 件中 ✗ #{hard.size} 件 / ? #{soft.size} 件"
  hard.size
end

# `id` は `p<ページ>-<ページ内連番>` なので、本体にエントリが 1 つ増えると以降の連番が
# ずれる。輪郭も MJ も id で紐づいているから、本体だけ作り直すと黙って別の字を指す
# （実際 137 件中 25 件がずれていた）。中身で裏を取る。
def check_links(entries, verbose)
  puts '== 付き合わせ（輪郭・MJ の id が本体と合っているか）=='
  by_id = entries.to_h { |e| [e['id'], e] }
  bad = 0
  {
    'gaiji_chuki_glyphs.tsv' => ->(row, e) { e && !e['glyph'].empty? },
    'gaiji_chuki_mj.tsv' => lambda { |row, e|
      e && !e['glyph'].empty? && e['sub'] == row['sub'] && e['desc'].include?(row['desc'])
    }
  }.each do |name, ok|
    path = File.join(DATA, name)
    next puts "  #{name} が無いのでスキップ" unless File.exist?(path)

    rows = read_tsv(path)
    ng = rows.reject { |row| ok.call(row, by_id[row['id']]) }
    puts "  #{name}: #{rows.size} 行 / 合わない #{ng.size}"
    ng.first(verbose ? 100 : 5).each { |row| puts "    ✗ #{row['id']}" }
    bad += ng.size
  end
  bad
end

# `ivs`/`cid` は content stream の CID から取る。1 文字を複数のパーツで組んでいる字は
# ` / ` 区切りで全パーツを持っているので対象外。単独のはずなのに基底字が組み立ての「部品」
# として現れたら、まだ片方しか拾えていない。IDS は説明から別途導いたもので判定は独立。
def check_ivs(entries, verbose)
  puts '== ivs（基底字が字全体ではなく部品になっていないか）=='
  bad = entries.select do |e|
    next false if e['ivs'].empty? || e['ivs'].include?(' / ') || e['ids'].length < 2

    base = [e['ivs'].split(' ').first.to_i(16)].pack('U')
    e['ids'].each_char.any?(base)
  end
  multi = entries.count { |e| e['ivs'].include?(' / ') }
  puts "  ivs #{entries.count { |e| !e['ivs'].empty? }} 件（うち複数パーツ #{multi}）/ 部品を指している #{bad.size} 件"
  bad.first(verbose ? 100 : 6).each do |e|
    base = [e['ivs'].split(' ').first.to_i(16)].pack('U')
    puts "    ✗ #{e['id']} ids=#{e['ids']} 基底=#{base} desc=#{e['desc']}"
  end
  bad.size
end

def check_html(entries, path)
  puts '== HTML（TSV の全項目が出ているか）=='
  unless File.exist?(path)
    puts "  #{path} が無いのでスキップ"
    return 0
  end

  html = File.read(path)
  ids = html.scan(/<article class="e" id="([^"]+)"/).flatten
  missing = entries.map { |e| e['id'] } - ids
  extra = ids - entries.map { |e| e['id'] }
  puts "  記事 #{ids.size} / TSV #{entries.size}"
  puts "  ✗ HTML に無い: #{missing.first(10).join(' ')}#{missing.size > 10 ? " ほか#{missing.size - 10}" : ''}" unless missing.empty?
  puts "  ✗ TSV に無い: #{extra.first(10).join(' ')}" unless extra.empty?
  missing.size + extra.size
end

def main(argv)
  opts = { html: File.expand_path('../tmp/gaiji_chuki.html', __dir__), verbose: false }
  parser = OptionParser.new do |o|
    o.banner = 'usage: verify_gaiji_chuki.rb <gaiji_chuki.pdf> [--html PATH] [-v]'
    o.on('--html PATH') { |v| opts[:html] = v }
    o.on('--tsv PATH') { |v| opts[:tsv] = v }
    o.on('-v', '--verbose') { opts[:verbose] = true }
  end
  parser.parse!(argv)
  pdf = argv.shift
  unless pdf && File.exist?(pdf)
    warn parser.banner
    return 2
  end

  entries = read_tsv(opts[:tsv] || File.join(DATA, 'gaiji_chuki.tsv'))
  puts "#{entries.size} 項目を #{File.basename(pdf)} と照合する"
  bad = check_counts(entries, pdf, opts[:verbose])
  bad += check_radicals(entries, pdf)
  bad += check_notes(entries, pdf, opts[:verbose])
  bad += check_ivs(entries, opts[:verbose])
  bad += check_links(entries, opts[:verbose])
  bad += check_html(entries, opts[:html])
  puts bad.zero? ? "\n食い違いなし" : "\n食い違い #{bad} 件"
  bad.zero? ? 0 : 1
end

exit(main(ARGV)) if __FILE__ == $PROGRAM_NAME
