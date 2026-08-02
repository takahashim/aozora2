#!/usr/bin/env ruby
# frozen_string_literal: true

# 外字注記辞書の HTML 版を 1 枚に組む。
#
#   ruby tools/build_gaiji_chuki_html.rb [出力先.html]
#
# 入力は crates/aozora-core/data の 4 つ（`gaiji_chuki.md` 参照）。外部リソースを参照
# しない自己完結の 1 ファイルにする。字形は `gaiji_chuki_glyphs.tsv` の輪郭をインライン
# SVG で埋めるので、符号位置を持たない字もフォント無しで見える——PDF 版に対する主な利点で、
# あとは検索・アンカー・注記のコピーができること。
#
# BMP 外の字（U+207C2 など Ext-B 以降）もフォントに無いことが多いので、GlyphWiki から
# SVG を取って <symbol> で焼き込む。初回だけ通信が要る（524 字・2 分ほど）。取ったものは
# GLYPHWIKI_CACHE（既定 tmp/glyphwiki）に置いて使い回す。引けなければフォント任せに戻る。

require 'json'
require 'net/http'
require 'set'
require_relative 'gaiji_chuki_data'
require_relative 'gaiji_glyph_svg'

DATA = File.expand_path('../crates/aozora-core/data', __dir__)
OUT = ARGV[0] || File.expand_path('../tmp/gaiji_chuki.html', __dir__)
# GlyphWiki から取った SVG の置き場。一度取れば以後は使い回す。
CACHE = ENV.fetch('GLYPHWIKI_CACHE', File.expand_path('../tmp/glyphwiki', __dir__))

# 字 → その字を描く SVG 片。BMP 外の字はフォントに無いことが多いので、ここに入れて
# 焼き込む。組み立てはデータを読んだ後（下の「字形を焼き込む」）。
SVG_GLYPHS = {}

entries = GaijiChuki.read_tsv(File.join(DATA, 'gaiji_chuki.tsv'))
outlines = GaijiChuki.read_tsv(File.join(DATA, 'gaiji_chuki_glyphs.tsv')).group_by { |g| g['id'] }
jis2ucs = JSON.parse(File.read(File.join(DATA, 'jis2ucs.json')))
mj_path = File.join(DATA, 'gaiji_chuki_mj.tsv')
mj = File.exist?(mj_path) ? GaijiChuki.read_tsv(mj_path).to_h { |m| [m['id'], m] } : {}

# --- 字形 -----------------------------------------------------------------

# 辞書の凡例の色。原色のままだと薄い背景で読めないので、意味を保ったまま濃くする。
LEGEND = {
  'rgb(0%, 100%, 0%)' => '#1a7f37',   # JIS X 0213:2004 で例示字形変更
  'rgb(100%, 0%, 0%)' => '#b3261e',   # 包摂される字（底本の字形）
  'rgb(50%, 50%, 50%)' => '#6b6560'   # 参考掲載
}.freeze

def codepoints(hex_list)
  hex_list.split(' ').map { |c| c.to_i(16) }.pack('U*')
end

# 面区点 → 字。jis2ucs の値は実体参照（`&#x3000;`）。
def jis_char(jis, table)
  men, ku, ten = jis.split('-').map(&:to_i)
  ent = table[format('%d-%02d-%02d', men, ku, ten)] or return nil
  ent.scan(/&#x([0-9A-Fa-f]+);/).flatten.map { |c| c.to_i(16) }.pack('U*')
end

def h(str)
  str.to_s.gsub('&', '&amp;').gsub('<', '&lt;').gsub('>', '&gt;').gsub('"', '&quot;')
end

# 表示するテキスト。焼き込んだ字はそちらに差し替える。属性値には使わない（検索用の
# data-s は生の字のままにする——打った字で当たってほしいので）。
def t(str)
  str.to_s.each_char.map { |c| SVG_GLYPHS[c] || h(c) }.join
end

# 字形の SVG。色は辞書の凡例に対応させる（原色は薄い背景で読めないので濃くしてある）。
def glyph_svg(parts)
  GaijiGlyphSvg.render(parts, fill: ->(p) { LEGEND.fetch(p[%q{fill}], %q{#1c1a18}) }, escape: method(:h))
end

# その字が今どういう立場にあるか。字形の欄の地色で見分ける。
#
#   coded  面区点か U+ が決まっている。そのまま打てる
#   sub    符号位置は無いが、包摂適用などで代用してよい字がある
#   bare   どちらも無い。注記でしか書けない
def standing(entry)
  return 'coded' if !entry['jis'].empty? || !entry['unicode'].empty?

  entry['sub'].empty? ? 'bare' : 'sub'
end

# 字形の欄に何を出すか。確かなものから順に落とす。
def glyph_cell(e, outlines, jis2ucs)
  if (parts = outlines[e['id']]) && (svg = glyph_svg(parts))
    return [svg, '輪郭']
  end
  if !e['jis'].empty? && (c = jis_char(e['jis'], jis2ucs))
    return [t(c), '面区点']
  end
  return [t(codepoints(e['unicode'])), 'U+'] unless e['unicode'].empty?
  return [t(codepoints(e['ivs'])), 'IVS'] unless e['ivs'].empty?
  return [t(e['ids_char']), 'IDS'] unless e['ids_char'].empty?

  ['<span class="none">—</span>', nil]
end

# --- 注記の復元 -----------------------------------------------------------

# --- 字形を焼き込む -------------------------------------------------------

# 異体字セレクタとタグは不可視の制御文字。字ではないので焼き込まない——GlyphWiki には
# `ue0101` に「VS18」という枠付きの代替グリフがあり、そのまま出すと本文に見えてしまう。
INVISIBLE = (0xE0000..0xE01EF).freeze

# 表示に出る BMP 外の字を集める。Ext-B 以降を持つフォントは珍しく、U+207C2 のような字は
# たいてい豆腐になる。desc・IDS・代用字・U+ 欄のどこに出るものも対象。
def beyond_bmp(entries)
  seen = Set.new
  entries.each do |e|
    texts = [e['desc'], e['ids'], e['ids_char'], e['sub']]
    texts << codepoints(e['unicode']) unless e['unicode'].empty?
    texts << codepoints(e['ivs']) unless e['ivs'].empty?
    texts.each do |s|
      s.to_s.each_char { |c| seen << c if c.ord > 0xFFFF && !INVISIBLE.cover?(c.ord) }
    end
  end
  seen
end

# GlyphWiki の SVG。グリフは登録時に著作権が GlyphWiki に譲渡され、事実上パブリック
# ドメインとして扱えるので焼き込んでよい（GlyphWiki:データ・記事のライセンス）。
def fetch_glyphwiki(names)
  Dir.mkdir(CACHE) unless Dir.exist?(CACHE)
  missing = names.reject { |n| File.exist?(File.join(CACHE, "#{n}.svg")) }
  return if missing.empty?

  warn "GlyphWiki から #{missing.size} 字を取得"
  http = Net::HTTP.new('glyphwiki.org', 443)
  http.use_ssl = true
  http.start do
    missing.each_with_index do |name, i|
      res = http.get("/glyph/#{name}.svg", { 'User-Agent' => 'aozora2 build_gaiji_chuki_html.rb' })
      File.write(File.join(CACHE, "#{name}.svg"), res.code == '200' ? res.body : '')
      warn "  #{i + 1}/#{missing.size}" if ((i + 1) % 100).zero?
      sleep 0.05
    end
  end
rescue StandardError => e
  warn "GlyphWiki を引けなかった（#{e.class}: #{e.message}）。BMP 外の字はフォント任せになる。"
end

# 取れた SVG を <symbol> にして、字 → <use> の対応を作る。
def build_glyphs(chars)
  names = chars.to_h { |c| [c, format('u%04x', c.ord)] }
  fetch_glyphwiki(names.values.uniq)
  symbols = []
  names.each do |char, name|
    src = begin
      File.read(File.join(CACHE, "#{name}.svg"))
    rescue StandardError
      ''
    end
    inner = src[%r{<g\b[^>]*>(.*?)</g>}m, 1] or next
    next if inner.strip.empty?

    view = src[/viewBox="([^"]+)"/, 1] || '0 0 200 200'
    symbols << %(<symbol id="#{name}" viewBox="#{h view}">#{inner}</symbol>)
    SVG_GLYPHS[char] = %(<svg class="gw"><use href="##{name}"/></svg>)
  end
  symbols
end

symbols = build_glyphs(beyond_bmp(entries))

# --- 組み立て -------------------------------------------------------------

# 部首は PDF の並び順（最初に出たページ順）を保つ。画数は部首内の小見出し。
radicals = entries.group_by { |e| e['radical'] }
sections = radicals.map do |radical, list|
  [radical, list.sort_by { |e| [e['page'].to_i, e['id'][/-(\d+)\z/, 1].to_i] }]
end

# GlyphWiki へのリンク。**符号位置が分かるならそちらへ向ける。** 合成グリフ名
# （`u2ff0-u65ec-u529b`）は部品が増えるほど未登録で、ページはあってもグリフが無いことが
# 多い。字が分かっているなら `u<符号位置>` は必ず引ける。
def glyphwiki_link(e)
  char = if !e['ids_char'].empty?
           e['ids_char']
         elsif !e['unicode'].empty? && !e['unicode'].include?(' ')
           codepoints(e['unicode'])
         end
  return %(<a href="https://glyphwiki.org/wiki/u#{format('%04x', char.ord)}">GlyphWiki</a>) if char
  return nil if e['glyphwiki'].empty?

  # 字が分からないときだけ合成名。未登録のことがあるのでその旨を添える。
  %(<a class="unreg" title="合成グリフ名。GlyphWiki に未登録のことがある" ) +
    %(href="https://glyphwiki.org/wiki/#{h e['glyphwiki']}">GlyphWiki?</a>)
end

def entry_html(e, outlines, jis2ucs, mj)
  glyph, source = glyph_cell(e, outlines, jis2ucs)
  meta = []
  meta << %(<b>第#{e['level']}水準 #{h e['jis']}</b>) unless e['jis'].empty?
  meta << %(<b>U+#{h e['unicode']}</b>) unless e['unicode'].empty?
  meta << "IVS #{h e['ivs']}" unless e['ivs'].empty?
  meta << "CID #{h e['cid']}" unless e['cid'].empty? || !e['ivs'].empty?
  unless e['ids'].empty?
    ids = t(e['ids'])
    ids += " #{glyphwiki_link(e)}" if glyphwiki_link(e)
    meta << "IDS #{ids}"
  end
  meta << "MJ #{h mj[e['id']]['mj']}" if mj[e['id']]
  meta << %(<span class="src">#{source}</span>) if source
  meta << %(<a class="pg" href="##{e['id']}">#{e['id']}</a>)
  meta << %(<a class="pg" href="#{h "https://www.aozora.gr.jp/gaiji_chuki/gaiji_chuki.pdf#page=#{e['page']}"}">PDF p#{e['page']}</a>)
  # ★ は「本来の部首以外にも足した項目」の印。行を立てるほどの情報ではないので出典の末尾に置く。
  meta << %(<span class="star" title="本来の部首以外にも足した項目">★</span>) unless e['cross'].empty?

  lines = []
  lines << %(<code class="note">#{t GaijiChuki.note(e)}</code>) if GaijiChuki.note(e)
  lines << %(<code class="subst">#{t GaijiChuki.substitution(e)}</code>) if GaijiChuki.substitution(e)
  lines << %(<div class="meta">#{meta.join(' ・ ')}</div>)

  # 打った通りに当たるように、面区点と U+ は表示と同じ綴りも入れておく。
  search = [
    e['desc'], e['radical'], e['sub'], e['sub_kind'], e['ids'], e['ids_char'], e['id'],
    e['jis'].empty? ? nil : "#{e['jis']} 第#{e['level']}水準",
    e['unicode'].empty? ? nil : "U+#{e['unicode']}"
  ].compact.reject(&:empty?).join(' ')
  <<~HTML
    <article class="e" id="#{e['id']}" data-s="#{h search}">
      <div class="g #{standing(e)}">#{glyph}</div>
      <div class="b">#{lines.join}</div>
      <div class="k">#{h e['strokes']}画</div>
    </article>
  HTML
end

body = sections.map do |radical, list|
  items = list.map { |e| entry_html(e, outlines, jis2ucs, mj) }.join
  %(<section class="r" id="r-#{h radical}"><h2>#{h radical}<span>#{list.size}</span></h2>#{items}</section>)
end.join

index = sections.map { |radical, list| %(<a href="#r-#{h radical}" title="#{list.size} 件">#{h radical}</a>) }.join

standings = entries.group_by { |e| standing(e) }
coded = standings.fetch('coded', []).size
sub_count = standings.fetch('sub', []).size
bare = standings.fetch('bare', []).size
drawn = entries.count { |e| outlines.key?(e['id']) }

html = <<~HTML
  <!DOCTYPE html>
  <html lang="ja">
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>青空文庫 外字注記辞書</title>
  <style>
    :root { --line:#d8d4cc; --dim:#6b6560; --bg:#fbfaf7; }
    * { box-sizing:border-box }
    body { margin:0; background:var(--bg); color:#1c1a18; font-family:"Hiragino Mincho ProN","Yu Mincho",serif; line-height:1.7 }
    .wrap { display:flex; align-items:flex-start }
    /* 索引は 202 部首あるので、脇柱そのものを縦スクロールさせる */
    aside { flex:0 0 19rem; position:sticky; top:0; height:100vh; overflow-y:auto;
            padding:1.5rem 1.2rem; background:#f5f3ee; border-right:1px solid var(--line) }
    main { flex:1; min-width:0 }
    h1 { margin:0 0 .5rem; font-size:1.2rem; letter-spacing:.05em; line-height:1.5 }
    .lead { margin:0 0 1rem; color:var(--dim); font-size:.76rem; line-height:1.8 }
    .lead a { color:inherit }
    #q { width:100%; font:inherit; font-size:.9rem; padding:.4rem .55rem;
         border:1px solid var(--line); background:#fff; border-radius:3px }
    #count { display:block; margin:.35rem 0 1rem; color:var(--dim); font-size:.75rem }
    nav { font-size:1rem; line-height:1.9 }
    nav a { display:inline-block; width:1.5em; text-align:center; color:#1c1a18; text-decoration:none }
    nav a:hover { background:#1c1a18; color:#f5f3ee }
    /* 画面の外の節はレイアウトを飛ばす。9264 件を一度に組むと表示切り替えのたびに
       数百 ms 止まる。auto を付けておくと一度描いた高さを覚えるので、スクロールバーも暴れない */
    section.r { padding:0 1.5rem; content-visibility:auto; contain-intrinsic-size:auto 600px }
    section.r h2 { position:sticky; top:0; background:var(--bg); margin:1.6rem 0 .4rem; padding:.3rem 0;
                   font-size:1.35rem; border-bottom:1px solid var(--line); z-index:1 }
    section.r h2 span { font-size:.7rem; color:var(--dim); margin-left:.6rem; font-weight:normal }
    .e { display:grid; grid-template-columns:3.4rem 1fr 3rem; gap:.8rem; align-items:start;
         padding:.45rem 0; border-bottom:1px solid #efece6 }
    /* 飛び先は部首の見出しのぶん下げる。見出しは top:0 に貼り付くので、真上に着けると隠れる */
    .e { scroll-margin-top:3.4rem }
    .e:target { background:#fff5d6 }
    .g { font-size:2rem; line-height:1.3; text-align:center; min-height:1.3em;
         border-radius:3px; padding:.1rem 0 }
    /* 字の立場を地色で。符号位置が決まっているものが大半なので、そこは無地にする */
    .g.sub { background:#fdf3e0 }    /* 代用してよい字がある（包摂適用など） */
    .g.bare { background:#e9eef4 }   /* 符号位置も代用字も無い。注記でしか書けない */
    .g svg { width:2.1rem; height:2.1rem; display:block; margin:0 auto }
    .g svg .mask { fill:var(--bg) }
    .g.sub svg .mask { fill:#fdf3e0 }
    .g.bare svg .mask { fill:#e9eef4 }
    .g .none { color:#c9c4bc; font-size:1.2rem }
    .b code { display:block; font-family:ui-monospace,"SF Mono",Menlo,monospace; font-size:.82rem;
              line-height:1.6; word-break:break-all }
    .b .note { color:#1a4f63 }       /* 外字注記。本文に貼れる形 */
    .b .subst { color:#7a5c2e }      /* 代用してよい字 */
    .meta { font-size:.72rem; color:var(--dim); font-family:ui-monospace,Menlo,monospace }
    .meta a { color:var(--dim) }
    .meta a.unreg { color:#a9a29a }
    .meta b { color:#1c1a18 }
    .meta .src { border:1px solid var(--line); padding:0 .25rem; border-radius:2px }
    .k { font-size:.72rem; color:var(--dim); text-align:right; padding-top:.2rem }
    .star { color:#b8860b }
    /* 字だけの一覧。**DOM はそのままで CSS だけ差し替える。** 9264 件を組み直すと
       切り替えのたびに数百 ms 止まるし、二重に持つとファイルが倍になる */
    #chart { width:100%; margin:.6rem 0 0; font:inherit; font-size:.8rem; padding:.35rem;
             border:1px solid var(--line); background:#fff; border-radius:3px; cursor:pointer }
    #chart:hover { background:#1c1a18; color:#f5f3ee }
    body.chart .e { display:inline-block; border:0; padding:0; vertical-align:top; cursor:pointer }
    body.chart .b, body.chart .k, body.chart footer { display:none }
    body.chart .g { width:2.4rem; font-size:1.7rem; margin:1px }
    body.chart .g svg { width:1.8rem; height:1.8rem }
    body.chart .e:hover .g { outline:2px solid #b8860b }
    body.chart section.r { padding:0 1.5rem .8rem }
    footer { padding:2rem 1.5rem; color:var(--dim); font-size:.78rem; border-top:1px solid var(--line) }
    .empty { margin:2rem 1.5rem; color:var(--dim); font-size:.85rem; max-width:44rem }
    .empty code { background:#fff; border:1px solid var(--line); padding:0 .2rem }
    /* 焼き込んだ字。BMP 外はフォントに無いことが多いので SVG で置く */
    .gw { width:1em; height:1em; vertical-align:-.12em; fill:#2f5d8a }
    #defs { position:absolute; width:0; height:0; overflow:hidden }
    [hidden] { display:none !important }
    /* 脇柱を畳んで頭に載せる。索引の高さ制限もここで外す */
    @media (max-width:820px) {
      .wrap { display:block }
      aside { position:static; height:auto; border-right:0; border-bottom:1px solid var(--line) }
      nav a { width:1.7em }
    }
  </style>

  <svg id="defs" aria-hidden="true">#{symbols.join}</svg>

  <div class="wrap">
  <aside>
    <h1>青空文庫<br>外字注記辞書</h1>
    <p class="lead">
      青空文庫外字注記辞書編集グループ・改訂第八版訂正版（2011-08-06）を機械可読にしたものからの生成。
      原典は <a href="https://www.aozora.gr.jp/gaiji_chuki/gaiji_chuki.pdf">gaiji_chuki.pdf</a>。
      利用条件は青空文庫本体と同じ。<br>
      全 #{entries.size} 項目（符号位置あり #{coded}、字形の輪郭あり #{drawn}）。
    </p>
    <input id="q" type="search" placeholder="説明・面区点・U+・代用字・IDS" autocomplete="off">
    <span id="count"></span>
    <button id="chart" type="button">例示字形をならべる</button>
    <nav>#{index}</nav>
  </aside>

  <main>
  <p id="empty" hidden class="empty">見つかりません。説明の綴りは辞書のもの——「にんべん」「さんずい」のような呼び名、<code>＋</code>（左右）<code>／</code>（上下）<code>＜</code>（囲む）<code>−</code>（取り除く）で書かれています。</p>

  #{body}

  <footer>
    字形の欄の地色はその字の立場を表す——無地は面区点か U+ が決まっていてそのまま打てる字（#{coded}）、
    <b style="background:#fdf3e0">薄橙</b>は符号位置は無いが包摂適用などで代用してよい字があるもの（#{sub_count}）、
    <b style="background:#e9eef4">薄青</b>はどちらも無く注記でしか書けないもの（#{bare}）。
    <code style="color:#1a4f63">※［＃…］</code>が外字注記、<code style="color:#7a5c2e">→［＃…］</code>が代用してよい字。<br>
    輪郭の色は辞書の凡例に対応する——<b style="color:#1a7f37">緑</b>は JIS X 0213:2004 で例示字形が変わった字、
    <b style="color:#b3261e">赤</b>は包摂される字（底本の字形）、<b style="color:#6b6560">灰</b>は参考掲載。
    原典は原色だが、薄い背景で読めないので濃くしてある。
    「輪郭」以外の字形は環境のフォント任せで、IVS は対応フォントでないと基底字に見える。<br>
    BMP 外の #{symbols.size} 字（U+207C2 など Ext-B 以降）はフォントに無いことが多いので、
    <a href="https://glyphwiki.org/">GlyphWiki</a> のグリフを SVG で焼き込んである。
    フォントで出ている字と区別できるよう <b style="color:#2f5d8a">青</b>にしてある。<br>
    IDS 脇の GlyphWiki リンクは、字が分かっていればその字のページへ。分からないときだけ
    合成グリフ名へ向けていて（<span style="color:#a9a29a">GlyphWiki?</span>）、そちらは未登録のことがある。
  </footer>
  </main>
  </div>

  <script>
  const q = document.getElementById('q'), count = document.getElementById('count');
  const empty = document.getElementById('empty');
  const items = [...document.querySelectorAll('.e')];
  const secs = [...document.querySelectorAll('section.r')];
  // a.hash は非 ASCII を percent-encode して返すので、id と突き合わせる前に戻す。
  const links = new Map([...document.querySelectorAll('nav a')]
    .map(a => [decodeURIComponent(a.hash.slice(1)), a]));
  const total = items.length;
  function show(sec, on) {
    sec.hidden = !on;
    const a = links.get(sec.id);
    if (a) a.hidden = !on;
  }
  function run() {
    const t = q.value.trim();
    if (!t) {
      items.forEach(e => e.hidden = false);
      secs.forEach(s => show(s, true));
      count.textContent = total + ' 項目';
      empty.hidden = true;
      return;
    }
    let n = 0;
    for (const e of items) {
      const hit = e.dataset.s.includes(t);
      e.hidden = !hit;
      if (hit) n++;
    }
    for (const s of secs) show(s, !!s.querySelector('.e:not([hidden])'));
    count.textContent = n + ' / ' + total + ' 項目';
    empty.hidden = n > 0;
  }
  // 絞り込みを URL に残す。file:// では replaceState が弾かれることがあるので黙って諦める。
  const param = new URLSearchParams(location.search).get('q');
  if (param) q.value = param;
  q.addEventListener('input', () => {
    run();
    try {
      history.replaceState(null, '', q.value ? '?q=' + encodeURIComponent(q.value) : location.pathname);
    } catch (_) { /* file:// */ }
  });
  run();

  // 字だけの一覧。切り替えは body のクラス 1 つで、DOM には触らない。字を押すと
  // その項目に戻る（hash を立てるので :target の下地も付く）。
  const chart = document.getElementById('chart');
  function setChart(on) {
    document.body.classList.toggle('chart', on);
    chart.textContent = on ? '一覧をとじる' : '例示字形をならべる';
  }
  // 今どの部首を見ているか。頭に貼り付いている見出しの節＝画面の上端をまたぐ最初の節。
  function topSection() {
    for (const s of secs) if (!s.hidden && s.getBoundingClientRect().bottom > 4) return s;
    return null;
  }
  chart.addEventListener('click', () => {
    const here = topSection();
    setChart(!document.body.classList.contains('chart'));
    // 組み直しで位置がまるごと変わるので、見ていた部首へ連れ直す。content-visibility で
    // 画面の外の節は高さが見積もりのままなので、実寸が入った次のフレームで置き直す。
    if (!here) return window.scrollTo(0, 0);
    here.scrollIntoView();
    requestAnimationFrame(() => here.scrollIntoView());
  });
  document.querySelector('main').addEventListener('click', ev => {
    if (!document.body.classList.contains('chart')) return;
    const e = ev.target.closest('.e');
    if (!e) return;
    setChart(false);
    location.hash = e.id;
  });
  // 一覧では説明が見えないので、触れたところだけ後から title を付ける。
  document.querySelector('main').addEventListener('mouseover', ev => {
    const e = ev.target.closest('.e');
    if (!e || e.title) return;
    const note = e.querySelector('code');
    if (note) e.title = note.textContent;
  });
  </script>
  </html>
HTML

File.write(OUT, html)
warn "#{OUT}  #{(File.size(OUT) / 1024.0 / 1024).round(2)} MB  #{entries.size} 項目 / 輪郭 #{drawn}"
