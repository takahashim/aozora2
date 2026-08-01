# frozen_string_literal: true

# 外字注記辞書のデータ規約。TSV の読み方と、注記の綴り方を 1 か所に置く。
#
# `crates/aozora-core/data/gaiji_chuki*.tsv` を読むツールが 4 つあり、どれも同じ約束の上に
# 立っている。分散させると、たとえば `split("\t", -1)` の `-1`（末尾の空欄を落とさない）を
# 1 つで落としたときに静かに壊れる。

module GaijiChuki
  # 末尾の空欄を落とさないこと。列は末尾ほど埋まらないので、落とすと行ごとに列数が変わる。
  def self.read_tsv(path)
    rows = File.readlines(path, chomp: true).map { |l| l.split("\t", -1) }
    head = rows.shift
    rows.map { |r| head.zip(r).to_h }
  end

  # TSV の列から注記を組み直す。
  #
  # 説明は全文なので、外側にもう一段ある書き方（`「尅」の「寸」に代えて「土」`）は既に
  # 「」 で始まっている。括り直さない。
  #
  # `ページ数-行数` は「ここに底本のページ-行を書く」という辞書のプレースホルダ。付き方は
  # PDF から数えた規則で、面区点があれば付かず、U+ なら必ず付き、どちらも無いときは代用字
  # （→［…］）が続くなら付かない。それでも 72 件は辞書側が揺れていて再現できない。
  #
  # **表示にも検証にも同じ規則を使う。** 一見あぶなく見えるが、この規則自体は
  # `verify_gaiji_chuki.rb` が PDF 本文と 1 件ずつ突き合わせて確かめている（10210 件中
  # そのまま出てくるもの 10189）。規則が間違っていれば検証が落ちるので、共有してよい。
  # 検証が抽出器から独立していることとは別の話で、そちらは今も守られている。
  def self.note(entry)
    return nil if entry['desc'].empty?

    body = entry['desc'].start_with?('「') ? entry['desc'] : "「#{entry['desc']}」"
    tail = if !entry['jis'].empty?
             "、第#{entry['level']}水準#{entry['jis']}"
           elsif !entry['unicode'].empty?
             "、U+#{entry['unicode']}、ページ数-行数"
           elsif !entry['sub'].empty?
             ''
           else
             '、ページ数-行数'
           end
    "※［＃#{body}#{tail}］"
  end

  def self.substitution(entry)
    return nil if entry['sub'].empty?

    rule = entry['sub_rule'].empty? ? '' : " #{entry['sub_rule']}"
    "→［#{entry['sub_kind']} #{entry['sub']}］#{rule}"
  end
end
