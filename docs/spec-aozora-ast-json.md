# Aozora AST 交換形式（JSON）

改訂: 2026-08 v0.1

この文書はたたき台である。
青空文庫形式のテキストを解析した木を、実装をまたいでやり取りするための形式を提案するもので、決定版ではない。
構成子の名前・粒度・互換メタデータの持ち方は議論の対象とする。

[RawAST 交換形式](spec-rawast-json.md)と対をなす。
この 2 文書で閉じており、他の文書を読まなくても実装できることを意図している。

---

## 1. 2 つの木

青空文庫形式のテキストからは、性格の違う 2 つの木が得られる。

| | [RawAST](spec-rawast-json.md) | Aozora AST（本書） |
|---|---|---|
| 性格 | 構文の木 | 意味の木 |
| 記法との関係 | 記法をそのまま写す | 記法を解決した正規形 |
| 前方参照 | 未解決のまま持つ | 解決済み |
| ブロック | 平坦なマーカー | 入れ子の木 |
| 単位 | 行の列 | ブロックの木 |
| 原文 | 各行が `source` を持ち復元できる | 復元できない |
| 主な用途 | フォーマッタ、記法リンタ、エディタ支援 | 描画、変換、本文の解析 |

両者に分かれているのは用途の違いであって、主従の関係にはない。

本書で「元の変換系」と呼ぶのは、青空文庫の公開 HTML を作ってきた変換器（aozora2html）である。既存の公開 HTML と一致する出力を作るための規則はこれに由来する。

### Aozora AST の不変条件

1. 参照は解決済み
   - 前方参照は解決され、未解決の参照は残らない
   - 解決できなかったものは編集者注（`Note`）になる。
2. 入れ子: ブロックは `Nested` / `LineWrap` の木。開始・終了のマーカーは消える。
3. 型付き・マーカーレス
   - 記法は構成子の型で表され、生の `［＃…］` 文字列は残らない
   - 編集者注の中身も解決済みのインライン列として持つ（原文は `raw` に併置するが、描画には使わない）
   - この木を消費する側は、記法のパーサを持たなくてよい
4. 出力に依存しない
   - HTML でもプレーンテキストでも、同じ木から状態を持たずに描画できる
   - 生成時の選択にも原則依存しない。例外は未閉じ `〔` の扱い（6 章）の 1 つだけ。
5. 行番号を保持
   - 各ブロックは由来する行の番号 `line` を持つ（原点は 4 章）
   - char 単位の精度が要るときは RawAST を使う。

## 2. 共通の規約

もう一方の交換形式と同一（両文書に同じ内容を載せる）。

### 構成子の表し方

- 構成子（直和の枝）は `{"kind": "名前", "value": ...}`
  - `value` は構成子の内容。
- 内容を持たない構成子は `value` を省く。
- 木のノード（RawAST の `Node`、Aozora AST の `Inline`）は、この 2 つのキーを親の
  オブジェクトに平坦化して持つ。
  - `{"kind": "Text", "value": "…", "span": {…}}` の形になる（`Inline` はさらに
    `range_form` が並ぶ）。

### フィールドと値

- フィールド名は snake_case。キーの順序に意味は無い。
- 省略可の値が無いときは `null` を書く（フィールド自体は省かない）。
- 列は空でも `[]` を書く。
- `Nat` は 0 以上の整数、`Text` は文字列、`Bool` は真偽値。
- 列挙値は文字列で書く。ただし値を持つ変種がある列挙（`CloseKind` `BlockKind`
  `RefSpec` など）は構成子と同じ `{"kind": …, "value": …}` の形になる。

  | 列挙 | 値 |
  |---|---|
  | `MidashiLevel` | `"O"`（大）, `"Naka"`（中）, `"Ko"`（小） |
  | `MidashiStyle` | `"Normal"`（通常）, `"Dogyo"`（同行）, `"Mado"`（窓） |
  | `FontSizeType` | `"Dai"`（大きな文字）, `"Sho"`（小さな文字） |
  | `RubyDirection` | `"Right"`（右ルビ）, `"Left"`（左ルビ） |

- `StyleType`（装飾の種類）は記法語と次のように対応する。記法語の頭に「左に」が
  付くと右列の構成子になる。

  | 記法語 | 構成子 | 「左に◯◯」の構成子 |
  |---|---|---|
  | 傍点 | `SesameDot` | `SesameDotAfter` |
  | 白ゴマ傍点 | `WhiteSesameDot` | `WhiteSesameDotAfter` |
  | 丸傍点 | `BlackCircle` | `BlackCircleAfter` |
  | 白丸傍点 | `WhiteCircle` | `WhiteCircleAfter` |
  | 黒三角傍点 | `BlackTriangle` | `BlackTriangleAfter` |
  | 白三角傍点 | `WhiteTriangle` | `WhiteTriangleAfter` |
  | 二重丸傍点 | `Bullseye` | `BullseyeAfter` |
  | 蛇の目傍点 | `Fisheye` | `FisheyeAfter` |
  | ばつ傍点 | `Saltire` | `SaltireAfter` |
  | 傍線 | `UnderlineSolid` | `OverlineSolid` |
  | 二重傍線 | `UnderlineDouble` | `OverlineDouble` |
  | 鎖線 | `UnderlineDotted` | `OverlineDotted` |
  | 破線 | `UnderlineDashed` | `OverlineDashed` |
  | 波線 | `UnderlineWave` | `OverlineWave` |
  | 太字 | `Bold` | — |
  | 斜体 | `Italic` | — |
  | 下付き小文字 | `Subscript` | — |
  | 上付き小文字 | `Superscript` | — |

### 位置

- `Span` は `{"start": Nat, "end": Nat}`。行内の位置で、`end` は終端の次を指す半開区間。
- 単位は char。バイトでも UTF-16 コードユニットでも書記素クラスタでもなく、
  Unicode スカラー値を 1 と数える（`𥥔` U+25954 は BMP 外だが 1 char）。
- 0 起点。

### 文字列

- 文字列は Unicode の文字列で、入力にあった文字をそのまま持つ。
- ソースが Shift_JIS のとき、Shift_JIS ⇄ Unicode の対応表は実装ごとに違いうる。
  - 本形式は入力時はどちらの表も許し、Shift_JISへの出力時は同一視する
  - 次の 7 つの符号位置は、同じ文字の別の綴りとして扱う。

  | 区点 | 綴り A（WHATWG／CP932 系） | 綴り B（JIS X 0208 本来） | Shift_JIS |
  |---|---|---|---|
  | 1-01-29 | U+2015 ― | U+2014 — | `81 5C` |
  | 1-01-33 | U+FF5E ～ | U+301C 〜 | `81 60` |
  | 1-01-34 | U+2225 ∥ | U+2016 ‖ | `81 61` |
  | 1-01-61 | U+FF0D － | U+2212 − | `81 7C` |
  | 1-01-81 | U+FFE0 ￠ | U+00A2 ¢ | `81 91` |
  | 1-01-82 | U+FFE1 ￡ | U+00A3 £ | `81 92` |
  | 1-02-44 | U+FFE2 ￢ | U+00AC ¬ | `81 CA` |

- 木を実装間で突き合わせるときは、この 7 組を等価とみなす
  - または比較の前にどちらかへ正規化する
  - それ以外の文字は 1 対 1 に対応するので、揺れはこの 7 組に限られる。
- 揺れを許せるのは、Shift_JIS へ書き戻すときに両方の綴りが同じ符号位置に収束するからである。どちらの綴りで持っていても、出力したバイト列は一致する。
- 推奨（必須ではない）: 新しく実装するなら WHATWG Encoding Standard の shift_jis
  （＝上表の綴り A）に合わせるとよい。処理系ごとの指定名は次のとおり。

  | 処理系 | 指定 |
  |---|---|
  | JavaScript | `new TextDecoder('shift_jis')`（WHATWG そのもの） |
  | Rust | `encoding_rs::SHIFT_JIS` |
  | Ruby | `Windows-31J`（`Shift_JIS` は綴り B になる） |
  | Python | `cp932`（`shift_jis` は綴り B 寄りになる） |
  | Java | `windows-31j` / `MS932`（`Shift_JIS` は綴り B 寄りになる） |

- 青空文庫形式のテキストは ASCII と JIS X 0208 の範囲に閉じるのが本来の姿である。
  その外（CP932 の拡張文字、JIS X 0201 の半角カナ、JIS X 0213 の第 3・第 4 水準など）は
  記法上は外字注記 `※［＃…］` で書くべきもので、直接現れたら不適合として扱ってよい。

### 行

- 青空文庫形式のテキストは CRLF 区切り。単独の LF や CR は本文の文字として扱う。

## 3. 文書全体

```json
{
  "format": "aozora-ast",
  "version": "0.1",
  "header": HeaderInfo,
  "main_text": [ Block, ... ],
  "after_text": [ Block, ... ],
  "bibliographical": [ Block, ... ]
}
```

節ごとに木を分けて持つ。元の変換系が本文と後付け（本文終わり後・底本情報）を別の
規則で出す以上、木の側でも分けるのが素直である。

- `after_text` と `bibliographical` は無ければ空（`[]`）。
- 節の切れ目は元のテキストの記法で決まる。`［＃本文終わり］` があれば以降はすべて
  `after_text` に入り（底本行も含む）、`bibliographical` は空になる。`［＃本文終わり］`
  が無く `底本：` があれば、そこから `bibliographical` に入る。
- 凡例（ヘッダを閉じる空行の直後に罫線＝ `-` だけからなる行で始まり、次の罫線で終わる
  「記号についての注記」の範囲）は、どの節にも入らない。
- 元の変換系はヘッダの直後に改行を 1 つ出す。それに合わせ、本文が（凡例を挟まず）空行
  以外で始まる場合は、`main_text` の先頭に空の `Line` を 1 つ補う。この行には対応する
  原文が無い（`line` の値は未決、9 章）。本文が 1 行も無いまま底本情報に入る文書でも
  同様に 1 つ補う。

### ヘッダ

```
HeaderInfo = {
  title: Text?, author: Text?, subtitle: Text?,
  original_title: Text?, original_subtitle: Text?,
  translator: Text?, editor: Text?, henyaku: Text?
}
```

題名・著者はヘッダ行から抽出するもので、本文の木には入らない。この形式は原文を
保持しない（1 章）ので、抽出した結果をここに持つ。行数によって解釈が変わる規則
（2〜6 行だけを解釈し、1 行と 7 行以上は題名だけにする）は抽出の側にあり、この形式は
結果だけを運ぶ。

木から導けないのはこれだけである。フッタ「表記について」に出す「くの字点」の項目は、
元の変換系が生のソース行を走査して決めるが、木は注記の原文（`Note` の `raw`）まで
保つので、木の中の文字列をすべて見れば同じ結果になる。入力が改行で終わっていたか
どうかも、その改行が作る空行が節の最後の行として木に入るので、別に持つ必要はない。

原文そのものは戻らない。可逆性が要るなら [RawAST 交換形式](spec-rawast-json.md)を使う。

## 4. Block

```
Block =
  | Line     { inline: Inline*, brk: Break, line: Nat }
  | Nested   { kind: BlockKind, children: Block*, close: CloseKind, open: OpenKind, line: Nat }
  | LineWrap { kinds: BlockKind*, inline: Inline*, line: Nat }
```

| 構成子 | 意味 | 由来する記法の例 |
|---|---|---|
| `Line` | 内容の 1 行 | 本文の行 |
| `Nested` | 複数行を包むブロック | `［＃ここから２字下げ］` … `［＃ここで字下げ終わり］` |
| `LineWrap` | 同じ行に本文がある行スコープの包み | `［＃３字下げ］本文の行` |

- `line` は由来する行の原文（ファイル）における行番号（0 起点。ヘッダも数える）。
  RawAST の `line_no` と同じ原点なので、節をまたいで比べられ、RawAST の行と突き合わせ
  られる。本文の先頭に補われた行（3 章）だけは対応する原文が無い（値は未決、9 章）。
  位置メタデータで、構造の比較には含めない。
- `Nested` は複数行を包み、開閉の出力形（`open` / `close`）を持つ。`LineWrap` は 1 行を
  包み、開閉は行内で完結する。
- `LineWrap.kinds` が列なのは、行スコープの字下げを 1 行に複数書けるため
  （`［＃２字下げ］あ［＃５字下げ］い`）。外側から内側の順に並ぶ。
- アクセント記法の途中で行末に達した行は、内容の末尾に `UnclosedAccentBreak`（6 章）が入る。
  行スコープの包みでは、それが閉じ `</div>` より前の素の改行になる。

```json
{
  "kind": "Nested",
  "value": {
    "kind": { "kind": "Jisage", "value": { "width": 2 } },
    "children": [ { "kind": "Line", "value": { ... } } ],
    "close": { "kind": "Newline" },
    "open": "Newline",
    "line": 0
  }
}
```

（`close` は値を持つ変種がある列挙なので `{"kind": …}` の形、`open` は素の文字列。
2 章「フィールドと値」のとおり。）

## 5. BlockKind

```
BlockKind =
  | Jisage   { width: Nat? }          // 空幅は null（不正な CSS を出す Quirk の対象）
  | Chitsuki { width: Nat }
  | Jizume   { width: Nat }
  | Burasage { wrap_width: Nat?, width: Nat? }
  | Midashi  { level: MidashiLevel, style: MidashiStyle }
  | Keigakomi | Yokogumi | Caption | Futoji | Shatai
  | FontSize { size_type: FontSizeType, level: Nat }
```

| 構成子 | 記法 |
|---|---|
| `Jisage` | `N 字下げ` |
| `Chitsuki` | `地付き` / `地から N 字上げ` |
| `Jizume` | `N 字詰め` |
| `Burasage` | `N 字下げ、折り返して M 字下げ` |
| `Midashi` | `大見出し` / `中見出し` / `小見出し` |
| `Keigakomi` / `Yokogumi` / `Caption` | `罫囲み` / `横組み` / `キャプション` |
| `Futoji` / `Shatai` | `太字` / `斜体` |
| `FontSize` | `N 段階大きな文字` / `N 段階小さな文字` |

- `MidashiLevel` = `"O"` / `"Naka"` / `"Ko"`（大・中・小）
- `MidashiStyle` = `"Normal"` / `"Dogyo"` / `"Mado"`（通常・同行・窓）
- `FontSizeType` = `"Dai"` / `"Sho"`（大・小）
- `Burasage` の `text-indent` は `width - wrap_width` で求める。両方が `null` のときは 0。

## 6. Inline

```
Inline = { kind: 構成子名, value: 内容?, span: Span, range_form: Bool }
```

- `span` は行内の char 位置。ただし `Note` / `Okurigana` の `content` の中だけは
  注記文字列内の相対位置になる（`raw` の先頭を 0 とする）。注記は合成されることが
  あり（解決に失敗した前方参照が注記になる場合など）、原文の部分文字列とは限らないため、
  行内の絶対位置に写せない。行内の位置が要るときは注記自身の `span` を使う。
- 行マージした行では、前半のインラインの `span` の原点が `Line.line` の行と違う。
  未閉じ `〔` の行とぶら下げを開く行は、内容が次の行と 1 つの `Line` にまとまる。
  このとき前半のインラインの `span` は前の行の位置、`Line.line` は後の行の番号になる。
  未閉じ `〔` のマージは行の内容に `UnclosedAccentBreak` が入るので検出できるが、
  ぶら下げのマージには印が無い。char 位置を行に対して使う消費者は、この 2 つの
  場合があることを前提にすること。
- `range_form` は範囲形（`［＃太字］…［＃太字終わり］`）由来かどうか。後方参照形
  （`［＃「…」は太字］`）なら `false`。互換メタデータ（後述）。

| 構成子 | 内容 | 記法 |
|---|---|---|
| `Text` | `Text` | 素の文字列 |
| `Ruby` | `{ base: Inline*, ruby: Inline*, direction, keep_gaiji_notes_in_base: Bool }` | `漢字《かんじ》` |
| `Style` | `{ children: Inline*, style_type: StyleType }` | 傍点・傍線・太字・斜体 |
| `Midashi` | `{ children: Inline*, level, style }` | 同行・窓見出し |
| `Gaiji` | `{ description: Text, unicode: Text?, jis_code: Text?, had_igeta: Bool }` | `※［＃…］` |
| `Accent` | `{ code: Text, name: Text, unicode: Text? }` | `〔e'〕` |
| `Img` | `{ filename: Text, alt: Text, is_photo: Bool, width: Nat?, height: Nat? }` | 図の注記 |
| `Tcy` / `Keigakomi` / `Yokogumi` / `Caption` / `Warigaki` | `{ children: Inline* }` | 縦中横・罫囲み・横組み・キャプション・割書 |
| `FontSize` | `{ children: Inline*, size_type, level: Nat }` | 大きな／小さな文字 |
| `Note` | `{ content: Inline*, raw: Text }` | `［＃…］`（編集者注） |
| `Okurigana` | `{ content: Inline*, raw: Text }` | `［＃（…）］`（訓点送り仮名） |
| `Kaeriten` | `Text` | `［＃レ］` などの返り点 |
| `DakutenKatakana` | `{ num: Text }` | 面区点 1-7-82〜85 |
| `AnnotationEnd` | `{ prefix: Text, content: Inline*, suffix: Text }` | 注記付き範囲の終了 |
| `Warichu` | `{ open: Bool, suppress_paren: Bool }` | 割り注（開閉マーカー） |
| `ChitsukiInline` | `{ width: Nat, children: Inline* }` | 行途中で開き行末で閉じる地付き |
| `BlockInline` | `{ kind: BlockKind, children: Inline* }` | 同一行で開閉するブロック形 |
| `UnclosedAccentBreak` | （値なし） | 未閉じ `〔` が行末に達した跡（記法ではない。互換） |

- `StyleType` の値は 2 章の一覧を参照。
- `Note` / `Okurigana` の `content` は解決済みのインライン列で、`raw` は元の注記文字列。
  描画には `content` を使う。`raw` は診断・エディタ支援のための添え物。
- `Accent` は `〔…〕` の中のアクセント分解表記（`e'` など）を 1 文字ずつ解決したもの。
  `code` は面区点（例 `1-09-24`）、`name` は文字名（例 `アキュートアクセント付きe`）、
  `unicode` は対応する Unicode 文字列（合成列のこともある。対応が無ければ `null`）。
- `Ruby` の `keep_gaiji_notes_in_base` は、親文字に画像化できない外字があるとき、その注記
  `［＃…］` を親文字の中に残すか。`［＃注記付き］…終わり` の範囲ルビ由来なら真。偽なら
  描画時に注記をルビの外へ出す。
- `Img` の `is_photo` は図の説明に「写真」を含むか（描画のクラス分けに使う）。
- `AnnotationEnd` の `prefix` / `suffix` はマーカー原文の前後の文字列（`左に「` と
  `」の注記付き終わり` など）、`content` はその間の注記内容。
- `Warichu` の `suppress_paren` は、開き `（`・閉じ `）` を描画時に補うのを抑えるか。
  原文の直前に `（`（開き側）／直後に `）`（閉じ側）が既に書かれているとき真。
- `UnclosedAccentBreak` は記法ではない。青空文庫記法に「ここで改行」に当たる書き方は
  無く、これを生むのは対応する `〕` が無いまま行末に達した `〔` だけである（元の変換系は
  そこで `"<br />\r\n"` という文字列をバッファへ積む）。名前を効果ではなく原因にして
  あるのは、正当な記法と読み違えないため。実態の多くは誤記ですらなく、アクセント記法と
  同じ括弧を使う行をまたぐ亀甲括弧である（コーパス全 53 箇所のうち 47 箇所は後の行の
  `〕` で閉じる）。括弧はそのまま文字として出るので、行内で閉じた場合と見た目は変わらない。
- 位置づけは行末の改行ではなく行の内容の一部で、マージした 2 行の境目に入る。行末の
  改行を出すかどうかは `Line.brk`（互換メタデータ）が別に持つ。行がこれで終わっていて
  マージ単位の後半が空なら、閉じタグの前に改行が 2 つ並ぶ（元の変換系のとおり）。
- 現れるかどうかは生産者の選択による（既定は元の変換系に一致＝現れる）。未閉じ `〔`
  をただの文字として扱う選択では、この構成子も行マージも現れない。ただし既定では
  現れるので、この形式を読む側は必ず扱えなければならない（出ない文書があるだけで、
  形式の定義から外れるわけではない）。互換の選択のうちこの木に届くのはこれだけで、
  残りは描画時にしか効かない。RawAST はどの選択からも独立している。

```json
{
  "kind": "Gaiji",
  "value": {
    "description": "「こざとへん＋井」、U+9631、133-8",
    "unicode": "阱",
    "jis_code": null,
    "had_igeta": true
  },
  "span": { "start": 1, "end": 27 },
  "range_form": false
}
```

## 7. 互換メタデータ

HTML 出力のために持つ情報。プレーンテキストなど他の出力しか作らない実装は無視してよい。

```
Break     = Br | None | NoNewline                     // 文字列
OpenKind  = Newline | NoBreak                          // 文字列
CloseKind = NoBreak | Newline | BareBreak              // 構成子（値を持つ変種があるため）
          | BurasageWrapped { wrap_width: Nat?, width: Nat? }
```

| 型 | 位置 | 意味 |
|---|---|---|
| `Break` | `Line.brk` | 行末に何を出すか（下表） |
| `OpenKind` | `Nested.open` | 開始タグの後に改行を出す（`Newline`）か出さない（`NoBreak`）か |
| `CloseKind` | `Nested.close` | 閉じタグの形（下記） |
| `range_form` | `Inline` | 範囲形由来か（ぶら下げの行包み判定に効く） |

- `Break` の各値が行末に出すもの:

  | 値 | 行末に出すもの |
  |---|---|
  | `Br` | `<br />` と改行 |
  | `None` | 改行のみ（`<br />` を出さない） |
  | `NoNewline` | 何も出さない（行の途中でブロックが閉じるときの前半。改行は閉じタグ側が出す） |

- `CloseKind` の各値が出す閉じの形: `NoBreak` は `</div>`、`Newline` は `</div>` と改行、
  `BareBreak` は `</div><br />` と改行。`BurasageWrapped` は閉じタグを外側のぶら下げの
  行 div で包んだ形（`<div class="burasage" …></div></div>` と改行）で、`wrap_width` /
  `width` はその外側のぶら下げの幅。
- これは既存の HTML（青空文庫が公開しているもの）とバイト単位で一致する出力を作るために
  要る情報である。元の変換系は 1 行ずつ状態を持って出力を決めており、その判断結果を
  木の側に畳んで持たせたのがこれらのフィールドにあたる。意味構造そのものではない。
- 木は入力だけで決まる。互換の選択のうち木に届くのは未閉じ `〔` の扱い（6 章）の 1 つ
  だけで、それ以外は描画の段で吸収する。
- 形式上は必須フィールドだが、正しく埋めるには元の変換系の逐次モデルを再現する必要がある。
  バイト一致する HTML を目的にしない生産者がどう埋めるべきか（規定値・省略の可否）は未決
  （[9 節](#9-未決議論したい点)）。そうした用途の消費者はこれらに依存してはならない。

## 8. 例

[RawAST 交換形式](spec-rawast-json.md)の例と同じ入力（`題` / `著` / 空行 /
`本文［＃「本文」に傍点］`）を解決したもの。

```json
{
  "format": "aozora-ast",
  "version": "0.1",
  "header": {
    "title": "題",
    "author": "著",
    "subtitle": null,
    "original_title": null,
    "original_subtitle": null,
    "translator": null,
    "editor": null,
    "henyaku": null
  },
  "main_text": [
    {
      "kind": "Line",
      "value": {
        "inline": [],
        "brk": "Br",
        "line": 0
      }
    },
    {
      "kind": "Line",
      "value": {
        "inline": [
          {
            "kind": "Style",
            "value": {
              "children": [
                {
                  "kind": "Text",
                  "value": "本文",
                  "span": {
                    "start": 0,
                    "end": 2
                  },
                  "range_form": false
                }
              ],
              "style_type": "SesameDot"
            },
            "span": {
              "start": 0,
              "end": 12
            },
            "range_form": false
          }
        ],
        "brk": "Br",
        "line": 3
      }
    },
    {
      "kind": "Line",
      "value": {
        "inline": [],
        "brk": "Br",
        "line": 4
      }
    }
  ],
  "after_text": [],
  "bibliographical": []
}
```

- ヘッダの 2 行は木に入らず、抽出結果が `header` に入る。
- 本文の `Text` と `UnresolvedReference` の 2 ノードが 1 つの `Style` に畳まれている。
- `Style` の `span` は畳み込みに使ったマーカー（`［＃「本文」に傍点］`）まで覆う。
- `range_form` が `false` なのは、これが後方参照形（`［＃「…」は…］`）由来だからである。
- `main_text` の 2 番目の行の `line: 3` と末尾の空行の `line: 4` は原文の行番号
  （題が 0）。末尾の空行は入力末尾の改行が作る。先頭の空行は 3 章の規則で補われた行で、
  対応する原文が無い（この `line: 0` の値は未決、9 章）。

## 9. 未決（議論したい点）

- 構成子の粒度。`Keigakomi` / `Yokogumi` / `Caption` はブロックとインラインの両方にあり、
  `BlockInline` でも包める。3 通りの表し方があるのは冗長かもしれない。
- 互換メタデータの持ち方。`Break` / `CloseKind` / `OpenKind` / `range_form` は HTML 固有で、
  交換形式に混ぜるべきか、別の層（描画ヒント）に分けるべきか。生産者側の要件（バイト一致
  HTML を作らない実装は何を埋めるか）も未定で、/2 では注釈層として分離する案が有力。
- 割り注が開閉マーカーである。`Warichu { open }` は木の中で唯一の対マーカーで、容器
  `Warichu { children }` にすれば意味モデルとして素直になる。ただし元の変換系は孤立した
  「割り注終わり」でも `）</span>` を出すため、容器形にするなら不均衡（孤立終端）の表現が要る。
- `Gaiji` の `unicode` / `jis_code` は導出値（RawAST の `Gaiji` も同様）。権威は
  `description` にあり、導出は面区点 → Unicode 対応表の版に依存する。照合時に導出値まで
  比較するかは決めていない。
- 節の持ち方。`main_text` / `after_text` / `bibliographical` を別のキーにしたが、節を
  配列にして種類を各要素に持たせる案もある。節が増えたときはそちらが素直になる。
- 補われた行の `line`。本文先頭に補う空行（3 章）は原文に対応する行が無く、いまの実装は
  節内の位置を入れている（原文由来の行番号と原点が混ざる）。`null` にする案が有力。
- `Break` の `None` という名前。JSON では文字列 `"None"` になり、`null` と紛らわしい。
  /2 での改名を検討。
- `Note` の `raw` を必須にするか。解決済みの `content` があれば描画はできるが、
  往復（AST → 記法)を考えると原文が要る。
- 位置情報の必須性。`span` / `line` はエディタ支援のためのもので、変換だけなら不要。
- `format` の版の進め方。破壊的変更のたびに版を上げるか、細分するか。

---


## 付録 A. Rust 実装との対応（参考）

規範ではない。

| 本書の型 | Rust |
|---|---|
| 文書全体 | `struct AozoraDocument { format, version, header, main_text, after_text, bibliographical }`（`interchange.rs`。各節の木は `AozoraAst = Vec<Block>`） |
| `Block` | `enum Block` |
| `BlockKind` | `enum BlockKind` |
| `Inline` | `struct Inline { kind, span, range_form }` |
| `Inline` の構成子 | `enum InlineKind` |
| `Break` / `CloseKind` / `OpenKind` | 同名の enum |
| `Span` | `struct Span { start, end }`（`token.rs`） |

- JSON への写像は `serde` の隣接タグ（`#[serde(tag = "kind", content = "value")]`）。
  `Inline.kind` と `Node.kind` は `#[serde(flatten)]` で親に平坦化している。
- `Option<T>` は `null`、`Vec<T>` は配列、`u32` / `usize` は `Nat` に対応する。
- 6 章「生産者の選択」は `Quirks::unclosed_accent_break`（既定オン）。互換フラグのうち
  この木に届くのがそれだけであることは `tests/invariants.rs` の
  `only_the_unclosed_accent_quirk_reaches_the_aozora_ast` が検査する。
