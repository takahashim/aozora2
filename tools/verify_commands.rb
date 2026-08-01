#!/usr/bin/env ruby
# frozen_string_literal: true

# 青空文庫記法コマンドの網羅・一致検証ツール。
#
# 参照実装（aozora2html, Ruby）が認識する全コマンドを列挙し、aozora2（Rust）の出力と
# 突き合わせて「対応漏れ」「誤実装」を検出する。
#
# 2 段階で検証する:
#
#   1. 静的カバレッジ: 参照の *_COMMAND 定数・INDENT_TYPE・command_table.yml から全
#      コマンド名を抽出し、Rust ソースに文字列として現れるか確認する。
#      → コマンドが「丸ごと欠けている」ケースを検出する。ただし「存在するが誤配線」
#        （例: 割書→warichu）は検出できない点に注意。
#
#   2. 挙動比較（本命）: 各コマンドの最小フィクスチャを参照 Ruby と aozora2 の両方に
#      通し、本文（main_text）の出力を正規化して差分する。
#      → 誤実装を確実に検出する。
#
#   ruby tools/verify_commands.rb [--ruby-dir DIR] [--a2 PATH] [-v]
#
# 省略時は本リポジトリ隣の ../aozora2html と target/release/aozora2 を使う。

require 'set'
require 'optparse'
require 'open3'
require 'tmpdir'

HERE = __dir__
DEFAULT_RUBY = File.expand_path(File.join(HERE, '..', '..', 'aozora2html'))
DEFAULT_A2 = File.expand_path(File.join(HERE, '..', 'target', 'release', 'aozora2'))

# 各コマンドの最小フィクスチャ（本文に差し込む 1〜複数行）。
# 参照 Ruby と aozora2 の両方に同じ入力を与えて main_text の出力を比較する。
FIXTURES = {
  # インライン装飾（後方参照 「対象」に/は<装飾>）
  '傍点' => '対象［＃「対象」に傍点］',
  '白ゴマ傍点' => '対象［＃「対象」に白ゴマ傍点］',
  '丸傍点' => '対象［＃「対象」に丸傍点］',
  '白丸傍点' => '対象［＃「対象」に白丸傍点］',
  '黒三角傍点' => '対象［＃「対象」に黒三角傍点］',
  '白三角傍点' => '対象［＃「対象」に白三角傍点］',
  '二重丸傍点' => '対象［＃「対象」に二重丸傍点］',
  '蛇の目傍点' => '対象［＃「対象」に蛇の目傍点］',
  'ばつ傍点' => '対象［＃「対象」にばつ傍点］',
  '傍線' => '対象［＃「対象」に傍線］',
  '二重傍線' => '対象［＃「対象」に二重傍線］',
  '鎖線' => '対象［＃「対象」に鎖線］',
  '破線' => '対象［＃「対象」に破線］',
  '波線' => '対象［＃「対象」に波線］',
  '太字' => '対象［＃「対象」は太字］',
  '斜体' => '対象［＃「対象」は斜体］',
  '下付き小文字' => '対象［＃「対象」は下付き小文字］',
  '上付き小文字' => '対象［＃「対象」は上付き小文字］',
  '行右小書き' => '対象［＃「対象」は行右小書き］',
  '行左小書き' => '対象［＃「対象」は行左小書き］',
  # ブロック/インラインスタイル
  '割書' => '［＃割書］夏期演説［＃割書終わり］',
  '横組み' => '［＃横組み］12［＃横組み終わり］',
  'キャプション' => '［＃キャプション］図［＃キャプション終わり］',
  '罫囲み' => '［＃罫囲み］囲［＃罫囲み終わり］',
  '割り注' => '本文［＃割り注］注記［＃割り注終わり］',
  # 見出し（大中小 × 通常/同行/窓）
  '大見出し' => "「章」は大見出し\r\n章題",
  '中見出し' => "「節」は中見出し\r\n節題",
  '小見出し' => "「項」は小見出し\r\n項題",
  '同行大見出し' => '「甲」は同行大見出し',
  '同行中見出し' => '「乙」は同行中見出し',
  '同行小見出し' => '「丙」は同行小見出し',
  '窓大見出し' => '「ａ」は窓大見出し',
  '窓中見出し' => '「ｂ」は窓中見出し',
  '窓小見出し' => '「ｃ」は窓小見出し',
  # ブロック字下げ系
  '字下げ' => "［＃ここから２字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
  '地付き' => '［＃地付き］末尾',
  '字詰め' => "［＃ここから10字詰め］\r\n内容\r\n［＃ここで字詰め終わり］",
  '字上げ' => "［＃ここから２字上げ］\r\n内容\r\n［＃ここで字上げ終わり］",
  '折り返して' => "［＃ここから１字下げ、折り返して３字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
  'この行' => '［＃この行２字下げ］行内容',
  '天付き' => "［＃天付き、折り返して２字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
  '大きな文字' => "［＃ここから２段階大きな文字］\r\n大\r\n［＃ここで大きな文字終わり］",
  '小さな文字' => "［＃ここから２段階小さな文字］\r\n小\r\n［＃ここで小さな文字終わり］",
  # その他インライン
  '縦中横' => '12［＃「12」は縦中横］',
  '返り点' => '學而時習之［＃「而」の左に返り点レ］',
  '訓点送り仮名' => '學［＃「學」の右に訓点送り仮名ブ］',
  '注記付き' => '呼吸［＃「呼吸」の注記付き］',
  '写真' => 'キャプション（fig001_01.png、横100×縦200）入る'
}.freeze

# 最小の妥当な青空文庫ファイル（CRLF・SJIS）を組み立てる
def build_doc(body)
  "テスト\r\nテスト\r\n\r\n#{body}\r\n底本：テスト\r\n".encode('CP932', invalid: :replace, undef: :replace)
end

# main_text セクションの中身だけを取り出し、CR を除いて正規化する
def main_text(html)
  m = html.match(%r{main_text">(.*?)</div>\s*<div class="bibliographical}m)
  (m ? m[1].strip : html).delete("\r")
end

def with_source(body)
  Dir.mktmpdir do |dir|
    src = File.join(dir, 'in.txt')
    File.binwrite(src, build_doc(body))
    yield src, dir
  end
end

def run_ruby(ruby_dir, body)
  with_source(body) do |src, dir|
    dst = File.join(dir, 'out.html')
    Open3.capture3('ruby', '-Ilib', 'bin/aozora2html', src, dst, chdir: ruby_dir)
    next '' unless File.exist?(dst)

    File.binread(dst).force_encoding('CP932').encode('UTF-8', invalid: :replace, undef: :replace)
  end
end

def run_a2(a2, body)
  with_source(body) do |src, _dir|
    out, = Open3.capture2(a2, 'html', src, binmode: true)
    out.force_encoding('CP932').encode('UTF-8', invalid: :replace, undef: :replace)
  end
end

# 参照が認識するコマンド名を、定数・INDENT_TYPE・command_table.yml から集める
def reference_commands(ruby_dir)
  rb = File.read(File.join(ruby_dir, 'lib', 'aozora2html.rb'), encoding: 'UTF-8')
  cmds = rb.scan(/[A-Z_]+_COMMAND\s*=\s*'([^']+)'/).flatten.to_set
  if (m = rb.match(/INDENT_TYPE\s*=\s*\{(.*?)\}/m))
    cmds |= m[1].scan(/:\s*'([^']+)'/).flatten
  end
  yml = File.join(ruby_dir, 'yml', 'command_table.yml')
  if File.exist?(yml)
    File.foreach(yml, encoding: 'UTF-8') do |line|
      line = line.chomp
      cmds << line[0..-2].strip if line.match?(/\A\S.*:\z/)
    end
  end
  cmds
end

def rust_sources(a2_bin)
  root = File.expand_path(File.join(File.dirname(a2_bin), '..', '..', 'crates'))
  Dir.glob(File.join(root, '**', '*.rs')).map { |f| File.read(f, encoding: 'UTF-8') }.join
end

def static_coverage(ruby_dir, rust_src)
  reference_commands(ruby_dir).sort.reject do |c|
    # 見出しは大/中/小 + 同行/窓 の合成で処理されるため、部分文字列で判定する。
    rust_src.include?(c) || (c.include?('見出し') && rust_src.include?('見出'))
  end
end

def main(argv)

  opts = { ruby_dir: DEFAULT_RUBY, a2: DEFAULT_A2, verbose: false }
  OptionParser.new do |o|
    o.on('--ruby-dir DIR', '参照実装 aozora2html の場所') { |v| opts[:ruby_dir] = v }
    o.on('--a2 PATH', 'aozora2 の release バイナリ') { |v| opts[:a2] = v }
    o.on('-v', '--verbose', '一致したものも表示') { opts[:verbose] = true }
  end.parse!(argv)

  unless File.exist?(opts[:a2])
    warn "aozora2 バイナリが見つかりません: #{opts[:a2]}"
    return 2
  end
  ruby_ok = File.exist?(File.join(opts[:ruby_dir], 'bin', 'aozora2html'))

  if ruby_ok
    missing = static_coverage(opts[:ruby_dir], rust_sources(opts[:a2]))
    puts '== 静的カバレッジ（コマンド名がRustソースに存在するか）=='
    puts missing.empty? ? '  全コマンド文字列を確認' : "  ★未出現: #{missing.inspect}"
    puts
  end

  puts '== 挙動比較（参照Ruby vs aozora2, main_text 差分）=='
  unless ruby_ok
    puts "  参照Rubyが無いためスキップ（--ruby-dir 指定）: #{opts[:ruby_dir]}"
    return 0
  end

  diffs = []
  FIXTURES.each do |cmd, body|
    ref = main_text(run_ruby(opts[:ruby_dir], body))
    ours = main_text(run_a2(opts[:a2], body))
    ok = ref == ours
    diffs << [cmd, ref, ours] unless ok
    puts "  #{ok ? '✓' : '✗'} #{cmd}" if opts[:verbose] || !ok
  end
  puts "\n  差分 #{diffs.size}/#{FIXTURES.size} 件"
  diffs.each do |cmd, ref, ours|
    puts "\n### #{cmd}"
    puts "  参照: #{ref[0, 200]}"
    puts "  我々: #{ours[0, 200]}"
  end
  diffs.empty? ? 0 : 1
end

exit(main(ARGV)) if __FILE__ == $PROGRAM_NAME
