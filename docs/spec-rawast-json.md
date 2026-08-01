# RawAST 交換形式（JSON）

改訂: 2026-08 v0.1

この文書はたたき台である。
青空文庫形式のテキストを解析した木を、実装をまたいでやり取りするための形式を提案するもので、決定版ではない。
構成子の名前・粒度・マーカーの持ち方は議論の対象とする。

[Aozora AST 交換形式](spec-aozora-ast-json.md)と対をなす。
この 2 文書で閉じており、他の文書を読まなくても実装できることを意図している。

---

## 1. 2 つの木

青空文庫形式のテキストからは、性格の違う 2 つの木が得られる。

| | RawAST（本書） | [Aozora AST](spec-aozora-ast-json.md) |
|---|---|---|
| 性格 | 構文の木 | 意味の木 |
| 記法との関係 | 記法をそのまま写す | 記法を解決した正規形 |
| 前方参照 | 未解決のまま持つ | 解決済み |
| ブロック | 平坦なマーカー | 入れ子の木 |
| 単位 | 行の列 | ブロックの木 |
| 原文 | 各行が `source` を持ち復元できる | 復元できない |
| 主な用途 | フォーマッタ、記法リンタ、エディタ支援 | 描画、変換、本文の解析 |

両者に分かれているのは用途の違いであって、主従の関係にはない。
実装間で照合するときは「同じ入力から同じ RawAST が出るか」「同じ RawAST から同じAozora AST が出るか」を分けて見られるので、差がパースにあるのか解決・畳み込みにあるのかを切り分けられる。

本書で「元の変換系」と呼ぶのは、青空文庫の公開 HTML を作ってきた変換器（aozora2html）である。既存の公開 HTML と一致する出力を作るための規則はこれに由来する。

### RawAST の不変条件

1. ソース忠実
   - 1 ソース行 = 1 `RawLine`
   - `source` を保持し、各ノードは char 単位の `span` を持つ。
   - 行のノード列は行を隙間なく覆う（3 章「タイル」）。
2. 平坦
   - 入れ子は `BlockStart` / `BlockEnd` / `LineJisage` のマーカーで表し、木にしない。
3. 未解決
   - 前方参照（`［＃「対象」に傍点］` など）は `UnresolvedReference` のまま。
   - 編集者注 `［＃…］` の中身も文字列のまま。
4. 可逆
   - 位置と原文を残す
   - 行を連結すれば元のテキストに戻せる。

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
  "format": "aozora-rawast",
  "version": "0.1",
  "lines": [ RawLine, ... ]
}
```

```
RawLine = {
  line_no: Nat, // 0 起点の行番号
  source: Text, // その行の原文（記法を含む。改行は含まない）
  nodes: Node*  // 解析結果
}
```

- 対象はファイルの全行である。題名・著者などのヘッダ、凡例、底本情報も
  行として入る。入力が改行で終わっていれば最後に空行が 1 つ入る。
- したがって `source` を CRLF で連結すれば元のテキストがそのまま戻る
  （1 章の不変条件「可逆」）。ヘッダ・底本・末尾の改行の有無も、この行の列から導ける。
- どの行がどの節（本文・本文終わり後・底本情報）かは、行の内容で決まる。
  - ヘッダ: 先頭から最初の空行まで。
  - 凡例（記号についての注記）: 空行の直後に罫線（`-` だけからなる行）が来たら、
    そこから次の罫線まで。どの節にも入らず、出力にも現れない。
  - 底本情報: `底本：` で始まる行から。
  - 本文終わり後: `［＃本文終わり］` の次の行から。
- `source` を持つのは、位置情報の基準になるのと、診断で原文を示すため。
- この形式は生産者の互換オプションから独立している。 元の変換系のバグを再現する
  かどうかは、この木を畳むとき・描くときの選択であって、原文を写した木は変わらない。
- 行が持つのはこの 3 つだけで、特定の記法のための欄は無い。元の変換系はアクセント記法の
  途中で行末に達すると、その行を出力せず内容を次の行へ持ち越して 1 つの出力単位に
  するが、それは行の旗ではなく内容の末尾に置かれる `UnclosedAccentBreak` ノードで表す
  （元の変換系も、旗ではなく `"<br />\r\n"` という文字列をバッファへ積んでいる）。
  同一行で閉じられなかったアクセントの位置は診断であって木の一部ではないので、
  この形式では運ばない（実装は木とは別に返す）。

### ノード列は行を覆う（タイル）

本形式だけの不変条件（Aozora AST には無い）。

- 行のノード列は、その行を隙間なく覆う。トップレベルのノードを順に見ると
  `span` は `0` から始まり、次のノードの `start` は前のノードの `end` に等しく、最後の
  `end` は行の char 数になる（幅 0 のノードは間に挟まってよい）。子ノードも親の範囲を
  同じ規則で覆う。
- この不変条件があるので、各ノードに原文の写しを持たせる必要がない。 ノードに
  対応する原文が要るなら `source` を `span` で切ればよい。逆に言えば、覆えるのは
  位置であって文字列ではない。区切り（`《》`・`［＃…］`・`〔〕`・`｜`）は
  どのノードの値にも入っていないので、`source` を持たない木だけから原文を
  組み立て直すことはできない（それには記法ごとの綴り直しが要る。本書の範囲外）。全ノードに原文を持たせる
  案もあるが、`source` と二重に持つことになるうえ（実測で全書庫 +370KB）、二つが
  食い違ったときにどちらが正かを決められない。
- 破れやすいのはアクセントである。`〔…〕` はブロックとして木に残らず中身だけが
  行のノード列に並ぶので、素朴に実装すると区切りの 2 文字がどのノードにも属さない
  （全書庫 410 万行で隙間 9228 件、いずれもこれ）。区切りは構文としてこのブロックの
  ものなので、先頭ノードの始点と末尾ノードの終点をブロックの端まで広げて引き受ける。
  閉じ `〕` が無いまま行末に達した場合も同じで、終点が行末になるだけ。

## 4. Node

```
Node = { kind: 構成子名, value: 内容?, span: Span }
```

### 4.1 インライン

| 構成子 | 内容 | 記法 |
|---|---|---|
| `Text` | `Text` | 素の文字列 |
| `Ruby` | `{ children: Node*, ruby: Node*, direction: RubyDirection, keep_gaiji_notes_in_base: Bool }` | `漢字《かんじ》` |
| `Style` | `{ children: Node*, style_type: StyleType }` | 傍点・傍線・太字・斜体 |
| `Midashi` | `{ children: Node*, level: MidashiLevel, style: MidashiStyle }` | 見出し |
| `Gaiji` | `{ description: Text, unicode: Text?, jis_code: Text?, had_igeta: Bool }` | `※［＃…］` |
| `Accent` | `{ code: Text, name: Text, unicode: Text? }` | `〔e'〕` |
| `Img` | `{ filename: Text, alt: Text, is_photo: Bool, width: Nat?, height: Nat? }` | 図の注記 |
| `Tcy` / `Keigakomi` / `Yokogumi` / `Caption` | `{ children: Node* }` | 縦中横・罫囲み・横組み・キャプション |
| `FontSize` | `{ children: Node*, size_type: FontSizeType, level: Nat }` | 大きな／小さな文字 |
| `Note` | `Text` | `［＃…］`（編集者注。中身は未解決の文字列） |
| `Okurigana` | `Text` | `［＃（…）］`（訓点送り仮名） |
| `UnclosedAccentBreak` | （値なし） | 未閉じ `〔` が行末に達した跡（記法ではない） |
| `Kaeriten` | `Text` | `［＃レ］` などの返り点（直接形） |
| `DakutenKatakana` | `{ num: Text }` | 濁点付き片仮名（面区点 1-7-82〜85） |
| `AnnotationEnd` | `{ prefix: Text, content: Node*, suffix: Text }` | 注記付き範囲の終了 |

- `Gaiji` の `description` は `※［＃…］` の中身（先頭の `＃` を除く）。`unicode` と
  `jis_code` はそれを解析した導出値で、権威は `description` にある。導出には
  JIS X 0213 の面区点 → Unicode 対応表が要り、表の版によって結果が変わりうる。
- `had_igeta` は記法に `＃` があったかどうか。`※［…］`（`＃` 無し）も外字として扱うが、
  外字一覧に載せる名前が空になるなど後段の扱いが変わるため区別する。
- `Accent` は `〔…〕` の中のアクセント分解表記（`e'` など）を 1 文字ずつ解決したもの。
  `code` は面区点（例 `1-09-24`）、`name` は文字名（例 `アキュートアクセント付きe`）、
  `unicode` は対応する Unicode 文字列（合成列のこともある。対応が無ければ `null`）。
- `Ruby` の `children` が空なのは、親文字がまだ確定していない段階を表す（`｜` の無いルビは
  直前のテキストから親文字を切り出す処理が要る）。
- `Ruby` の `keep_gaiji_notes_in_base` は、親文字に画像化できない外字があるとき、その注記
  `［＃…］` を親文字の中に残すか。`［＃注記付き］…終わり` の範囲ルビ由来なら真。偽なら
  描画時に注記をルビの外へ出す。
- `Img` の `is_photo` は図の説明に「写真」を含むか（描画のクラス分けに使う）。
- `AnnotationEnd` の `prefix` / `suffix` はマーカー原文の前後の文字列（`左に「` と
  `」の注記付き終わり` など）、`content` はその間の注記内容。
- 前方参照を解決すると現れる構成子: `Midashi` `Tcy` `Keigakomi` `Yokogumi` `Caption`
  `FontSize` `DakutenKatakana` `AnnotationEnd` は `UnresolvedReference` を解決した結果と
  して生まれる。RawAST は前方参照を未解決のまま持つ形式（1 章の不変条件）なので、
  生成した直後の RawAST には現れない。解決を RawAST の上で行う消費者が作りうる
  ものとして表に挙げてある。
- `Kaeriten` と `Okurigana` は両方の由来を持つ。直接形（`［＃レ］`・`［＃（した）］`）は
  生成時に現れ、後方参照形（`「レ」は返り点`・`「した」は訓点送り仮名`）の解決からも
  生まれる。

### 4.2 マーカー

Aozora AST には残らない。ブロックの畳み込みと前方参照の解決が消費する。

| 構成子 | 内容 | 意味 |
|---|---|---|
| `BlockStart` | `{ block_type: BlockType, params: BlockParams }` | ブロックの開始（`ここから…`／範囲開始） |
| `BlockEnd` | `{ block_type: BlockType, params: BlockParams, explicit_close: Bool }` | ブロックの終了 |
| `LineJisage` | `{ width: Nat? }` | 行単位の字下げ `［＃N 字下げ］` |
| `UnresolvedReference` | `{ raw: Text, target: Text, spec: RefSpec }` | 前方参照（未解決） |

- `BlockType` は値を持たない列挙なので素の文字列で書く（`"Jisage"`）。
  コマンドに含まれる記法語と次のように対応する。

  | 構成子 | 記法語 | | 構成子 | 記法語 |
  |---|---|---|---|---|
  | `Jisage` | 字下げ | | `FontDai` | 大きな文字 |
  | `Chitsuki` | 地付き・地から・字上げ | | `FontSho` | 小さな文字 |
  | `Jizume` | 字詰め | | `Tcy` | 縦中横 |
  | `Burasage` | 折り返して（他の語より優先） | | `Warigaki` | 割書 |
  | `Midashi` | 見出（大・中・小見出し） | | `Warichu` | 割り注 |
  | `Keigakomi` | 罫囲み | | `Style` | 装飾（2 章の記法語） |
  | `Yokogumi` | 横組み | | `AnnotationRange` | 注記付き |
  | `Caption` | キャプション | | `LeftAnnotationRange` | 左に注記付き |
  | `Futoji` | 太字 | | `Shatai` | 斜体 |

- `explicit_close` は `ここで…終わり`（明示）か `…終わり`（`ここで` 無し）かの区別。
  出力の改行の扱いが変わる。

```json
{
  "kind": "BlockStart",
  "value": {
    "block_type": "Jisage",
    "params": {
      "width": 2, "wrap_width": null, "level": null, "midashi_style": null,
      "font_size": null, "style_type": null, "is_block": true,
      "has_open_paren": false, "has_close_paren": false, "annotation": null
    }
  },
  "span": { "start": 0, "end": 11 }
}
```

### 4.3 BlockParams

開始・終了マーカーが運ぶパラメータ。種類ごとに使うフィールドが違う（使わないものは
`null`）。全フィールドを常に書く。

| フィールド | 型 | 使う種類 |
|---|---|---|
| `width` | `Nat?` | 字下げ・地付き・字詰め・ぶら下げ |
| `wrap_width` | `Nat?` | ぶら下げ（折り返し幅） |
| `level` | `MidashiLevel?` | 見出し |
| `midashi_style` | `MidashiStyle?` | 見出し |
| `font_size` | `Nat?` | 大きな／小さな文字 |
| `style_type` | `StyleType?` | 装飾 |
| `is_block` | `Bool` | `ここから…`（複数行）なら `true`、範囲形なら `false` |
| `has_open_paren` | `Bool` | 割り注（直前に `（` があるか） |
| `has_close_paren` | `Bool` | 割り注（直後に `）` があるか） |
| `annotation` | `Text?` | 注記付き範囲の注記文字列 |

### 4.4 RefSpec

前方参照が何を指示しているか。

```
RefSpec =
  | Style           StyleType
  | Midashi         { level: MidashiLevel, style: MidashiStyle }
  | FontSize        { size_type: FontSizeType, level: Nat }
  | Inline          "Tcy" | "Keigakomi" | "Yokogumi" | "Caption" | "Kaeriten" | "Okurigana"
  | AnnotationRuby  { annotation: Text }
  | SideNote        { annotation: Text }
  | EmbeddedGaiji   { jis_code: Text, annotation_ruby: Node*? }
  | DakutenKatakana { num: Text }
```

```json
{
  "kind": "UnresolvedReference",
  "value": {
    "raw": "「対象」に傍点",
    "target": "対象",
    "spec": { "kind": "Style", "value": "SesameDot" }
  },
  "span": { "start": 2, "end": 12 }
}
```

- `target` は前方に探す対象の文字列、`raw` は注記の原文。
- 対象が見つからなければ、解決の段階で `Note(raw)` になる
  - 記法として解釈できなかったものは編集者注として出す、というのが青空文庫形式の扱い
- `AnnotationRuby` は `「対象」に「注記」の注記`。対象を親文字、注記をルビにする。
- `SideNote` は傍記。対象の各文字の脇に注記を 1 文字ずつ並べる。
- `EmbeddedGaiji` は面区点コード指定の前方参照（`「5」はローマ数字、1-13-25` の置換形と、
  `「対象」に「※［＃…］…」の注記` の注記形）。`annotation_ruby` は注記形のときの
  ルビ内容で、`Node` の列が入る（`RefSpec` の中に木の断片が入るのはここだけ）。
  置換形では `null`。
- `DakutenKatakana` は面区点 `1-7-8N`（N=2〜5）による濁点付き片仮名への置換。
  置き換える文字は面区点から一意に決まる。

## 5. 例

文書 1 本まるごと。入力は次の 4 行（CRLF 区切り、末尾も改行）。

```
題
著

本文［＃「本文」に傍点］
```

ヘッダ 2 行・空行・本文 1 行に加えて、末尾の改行が作る空行が 5 行目として入る。
`source` を CRLF で連結すれば元のテキストに戻る。

```json
{
  "format": "aozora-rawast",
  "version": "0.1",
  "lines": [
    {
      "line_no": 0,
      "source": "題",
      "nodes": [
        {
          "kind": "Text",
          "value": "題",
          "span": {
            "start": 0,
            "end": 1
          }
        }
      ]
    },
    {
      "line_no": 1,
      "source": "著",
      "nodes": [
        {
          "kind": "Text",
          "value": "著",
          "span": {
            "start": 0,
            "end": 1
          }
        }
      ]
    },
    {
      "line_no": 2,
      "source": "",
      "nodes": []
    },
    {
      "line_no": 3,
      "source": "本文［＃「本文」に傍点］",
      "nodes": [
        {
          "kind": "Text",
          "value": "本文",
          "span": {
            "start": 0,
            "end": 2
          }
        },
        {
          "kind": "UnresolvedReference",
          "value": {
            "target": "本文",
            "spec": {
              "kind": "Style",
              "value": "SesameDot"
            },
            "raw": "「本文」に傍点"
          },
          "span": {
            "start": 2,
            "end": 12
          }
        }
      ]
    },
    {
      "line_no": 4,
      "source": "",
      "nodes": []
    }
  ]
}
```

- 本文の行は `Text` と `UnresolvedReference` の 2 ノードに分かれている。前方参照は
  未解決のまま（1 章の不変条件）で、解決すると 1 つの `Style` に畳まれる
  （[Aozora AST 交換形式](spec-aozora-ast-json.md)の例が同じ入力の変換後）。
- ヘッダの行も本文と同じように解析されるが、元の変換系の出力では記法が効かない
  （ルビと `｜` を剥がした生の文字列になる）。どの行がヘッダかは行の位置で決まる。

## 6. Aozora AST への変換の概要

手順の詳細はこの形式の規定ではない（実装ごとに違う可能性がある）。何が起きるかだけ示す。

1. 前方参照の解決
   - `UnresolvedReference` の `target` を直前のノード列から探し、`spec` に応じた要素（`Style` / `Midashi` / `Ruby` など）に畳む
   - 見つからなければ `Note` とする
2. ブロックの畳み込み
   - `BlockStart` / `BlockEnd` / `LineJisage` のマーカーを対にして、行をまたぐ入れ子ブロックにする
   - 対にならないものは行末や文末で閉じる
3. 注記の中身の解決
   - `Note` / `Okurigana` の文字列を解析し直し、中のルビや外字を解決済みのインライン列にする。
4. 行の意味づけ
   - 各行を内容の行とブロックに包まれた行に分け、出力形のメタデータ（改行の扱いなど）を確定する。

この過程で位置情報の一部は失われるか粗くなり、原文の復元もできなくなる。可逆性が要る用途では RawAST を使う。

## 7. 未決（議論が必要な点）

- トークン列を含めるか。本形式は Node 以上だけを扱い、字句トークンは含めない。
  トークナイザの都合が交換形式に漏れるのを避けるためだが、字句規則の検証には要るかもしれない。
- `BlockParams` の平坦さ。種類ごとに使うフィールドが違うのに 1 つの構造で持っている。
  種類別の構成子に分ける案もあるが、そうすると「解決前の生の形」という性格から離れる。
- `source` の重複。各行が原文を持つので、文書全体としては入力の写しを 2 度持つことになる。
- `Note` の中身。RawAST では文字列、Aozora AST では解決済み。2 つの形式で同じ名前の
  構成子が違う中身を持つのは分かりにくいかもしれない。
- `Gaiji` の導出値。`unicode` / `jis_code` は `description` から導かれ、対応表の版に
  依存する。照合時に導出値まで比較するかは決めていない。
- 節の切り分けを消費者に任せていること。どの行が本文でどの行が底本情報かは行の内容から
  導けるが、その規則（3 章）を各実装が持つことになる。行に節の印を付けて運ぶ案もある。

---


## 付録 A. Rust 実装との対応（参考）

規範ではない。

| 本書の型 | Rust |
|---|---|
| 文書全体 | `struct RawDocument { format, version, lines }`（`interchange.rs`。木の器は `RawDoc { lines }`） |
| `RawLine` | `struct RawLine { source, nodes, line_no }` |
| `Node` | `struct Node { kind, span }`（`node/mod.rs`） |
| `Node` の構成子 | `enum NodeKind` |
| `BlockType` / `BlockParams` | `node/block.rs` |
| `RefSpec` | `node/reference.rs` |

- JSON への写像は `serde` の隣接タグ（`#[serde(tag = "kind", content = "value")]`）。
  `Node.kind` は `#[serde(flatten)]` で親に平坦化している。
- 生成は `parser::parse_document_raw(&lines) -> RawDoc`。`RawDocument::from_text` は
  互換フラグ（`Quirks`）を受け取らない——3 章「互換オプションから独立」の実装上の対応で、
  `tests/invariants.rs` の `the_raw_ast_is_independent_of_quirks` が検査する。
  タイル不変条件は同じく `nodes_tile_their_line_without_gaps` が検査する。
- 記法ごとの例が `crates/aozora-core/data/conformance/*.json` にある（入力・RawAST・
  Aozora AST の 3 点組）。
