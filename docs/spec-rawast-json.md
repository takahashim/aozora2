# RawAST 交換形式（JSON）

改訂: 2026-07 v0.1

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

### RawAST の不変条件

1. ソース忠実
  - 1 ソース行 = 1 `RawLine`
  - `source` を保持し、各ノードは char 単位の `span` を持つ。
2. 平坦
  - 入れ子は `BlockStart` / `BlockEnd` / `LineJisage` のマーカーで表し、木にしない。
3. 未解決
  - 前方参照（`［＃「対象」に傍点］` など）は `UnresolvedReference` のまま。
  - 編集者注 `［＃…］` の中身も文字列のまま。
4. 可逆
  - 位置と原文を残す
  - 行を連結すれば元のテキストに戻せる。

## 2. 共通の規約

Aozora AST 交換形式と同一（[spec-aozora-ast-json.md](spec-aozora-ast-json.md) の同名の節と同じ内容）。

### 構成子の表し方

- 構成子（直和の枝）は `{"kind": "名前", "value": ...}`
  - `value` は構成子の内容。
- 内容を持たない構成子は `value` を省く。
- `Node` は、この 2 つのキーを親のオブジェクトに平坦化して持つ。
  - `{"kind": "Text", "value": "…", "span": {…}}` の形になる。

### フィールドと値

- フィールド名は snake_case。
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

- `StyleType`（装飾の種類）は次のいずれか。

  ```
  傍点系            SesameDot WhiteSesameDot BlackCircle WhiteCircle BlackTriangle
                    WhiteTriangle Bullseye Fisheye Saltire
  傍点系（左・上）  上の 9 つに After を付けた形（SesameDotAfter など）
  傍線系            UnderlineSolid UnderlineDouble UnderlineDotted UnderlineDashed
                    UnderlineWave
  傍線系（左・上）  OverlineSolid OverlineDouble OverlineDotted OverlineDashed OverlineWave
  その他            Bold Italic Subscript Superscript
  ```

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
- `source` に改行文字は含めない。

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
  line_no: Nat,            // 0 起点の行番号
  source: Text,            // その行の原文（記法を含む。改行は含まない）
  nodes: Node*,            // 解析結果
  unclosed_accents: Span*, // 閉じられていないアクセント記法の位置（診断用）
  unclosed_accent_to_eol: Bool  // 閉じられていないアクセントが行末まで達したか
}
```

- **対象はファイルの全行**である。題名・著者などのヘッダ、注記凡例、底本情報も
  行として入る。入力が改行で終わっていれば最後に空行が 1 つ入る。
- したがって `source` を CRLF で連結すれば**元のテキストがそのまま戻る**
  （1 章の不変条件「可逆」）。ヘッダ・底本・末尾の改行の有無も、この行の列から導ける。
- どの行がどの節（本文・本文終わり後・底本情報）かは、行の内容で決まる。`底本：` で
  始まる行から底本情報、`［＃本文終わり］` の次の行から本文終わり後になる。
  ヘッダは先頭から最初の空行まで、注記凡例は罫線で囲まれた範囲。
- `source` を持つのは、位置情報の基準になるのと、診断で原文を示すため。
- `unclosed_accent_to_eol` は互換メタデータである。元の変換系はアクセント記法の
  途中で行末に達すると、その行を出力せず内容を次の行へ持ち越して 1 つの出力単位に
  する（行末には素の改行が入る）。行の切れ目が入力と出力で 1 対 1 でなくなる唯一の
  ケースで、木の形からは読み取れないのでここに持つ。`unclosed_accents` が診断のための
  副産物なのに対し、こちらは出力の形を決める。

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
| `Kaeriten` | `Text` | `［＃「レ」は返り点］` |
| `DakutenKatakana` | `{ num: Text }` | 濁点付き片仮名（面区点 1-7-82〜85） |
| `AnnotationEnd` | `{ prefix: Text, content: Node*, suffix: Text }` | 注記付き範囲の終了 |

- `Gaiji` の `description` は `※［＃…］` の中身（先頭の `＃` を除く）。`unicode` と
  `jis_code` はそれを解析した導出値で、権威は `description` にある。導出には
  JIS X 0213 の面区点 → Unicode 対応表が要り、表の版によって結果が変わりうる。
- `had_igeta` は記法に `＃` があったかどうか。`※［…］`（`＃` 無し）も外字として扱うが、
  外字一覧に載せる名前が空になるなど後段の扱いが変わるため区別する。
- `Ruby` の `children` が空なのは、親文字がまだ確定していない段階を表す（`｜` の無いルビは
  直前のテキストから親文字を切り出す処理が要る）。
- **前方参照を解決すると現れる構成子**: `Midashi` `Tcy` `Keigakomi` `Yokogumi` `Caption`
  `FontSize` `DakutenKatakana` `AnnotationEnd` は `UnresolvedReference` を解決した結果と
  して生まれる。RawAST は前方参照を未解決のまま持つ形式（1 章の不変条件）なので、
  **生成した直後の RawAST には現れない**。解決を RawAST の上で行う消費者が作りうる
  ものとして表に挙げてある。

### 4.2 マーカー

Aozora AST には残らない。ブロックの畳み込みと前方参照の解決が消費する。

| 構成子 | 内容 | 意味 |
|---|---|---|
| `BlockStart` | `{ block_type: BlockType, params: BlockParams }` | ブロックの開始（`ここから…`／範囲開始） |
| `BlockEnd` | `{ block_type: BlockType, params: BlockParams, explicit_close: Bool }` | ブロックの終了 |
| `LineJisage` | `{ width: Nat? }` | 行単位の字下げ `［＃N 字下げ］` |
| `UnresolvedReference` | `{ raw: Text, target: Text, spec: RefSpec }` | 前方参照（未解決） |

- `block_type` は `{"kind": "名前"}` の形。名前は次のいずれか。

  ```
  Jisage | Chitsuki | Jizume | Burasage | Midashi | Keigakomi | Yokogumi | Caption
  Futoji | Shatai | FontDai | FontSho | Tcy | Warigaki | Warichu | Style
  AnnotationRange | LeftAnnotationRange
  ```

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
  | Style          StyleType
  | Midashi        { level: MidashiLevel, style: MidashiStyle }
  | FontSize       { size_type: FontSizeType, level: Nat }
  | Inline         "Tcy" | "Keigakomi" | "Yokogumi" | "Caption" | "Kaeriten" | "Okurigana"
  | AnnotationRuby { annotation: Text }
  | SideNote       { annotation: Text }
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

## 5. 例

`本文［＃「本文」に傍点］` の 1 行。

```json
{
  "format": "aozora-rawast",
  "version": "0.1",
  "lines": [
    {
      "line_no": 0,
      "source": "本文［＃「本文」に傍点］",
      "nodes": [
        { "kind": "Text", "value": "本文", "span": { "start": 0, "end": 2 } },
        {
          "kind": "UnresolvedReference",
          "value": {
            "raw": "「本文」に傍点",
            "target": "本文",
            "spec": { "kind": "Style", "value": "SesameDot" }
          },
          "span": { "start": 2, "end": 12 }
        }
      ],
      "unclosed_accents": []
    }
  ]
}
```

解決すると `Text` と `UnresolvedReference` が 1 つの `Style` に畳まれる
（[Aozora AST 交換形式](spec-aozora-ast-json.md)の例が同じ入力の変換後）。

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
- `unclosed_accents`。診断のための情報が構文木に同居している。別に分ける案。
- `source` の重複。各行が原文を持つので、文書全体としては入力の写しを 2 度持つことになる。
- `Note` の中身。RawAST では文字列、Aozora AST では解決済み。2 つの形式で同じ名前の
  構成子が違う中身を持つのは分かりにくいかもしれない。
- `Gaiji` の導出値。`unicode` / `jis_code` は `description` から導かれ、対応表の版に
  依存する。照合時に導出値まで比較するかは決めていない。
- 節の切り分けを消費者に任せていること。どの行が本文でどの行が底本情報かは行の内容から
  導けるが、その規則（`底本：` で始まる行、`［＃本文終わり］`、罫線で囲まれた凡例）を
  各実装が持つことになる。行に節の印を付けて運ぶ案もある。

---


## 付録 A. Rust 実装との対応（参考）

規範ではない。

| 本書の型 | Rust |
|---|---|
| 文書全体 | `struct RawDocument { format, version, lines }`（`interchange.rs`。木の器は `RawDoc { lines }`） |
| `RawLine` | `struct RawLine { source, nodes, line_no, unclosed_accents, unclosed_accent_to_eol }` |
| `Node` | `struct Node { kind, span }`（`node/mod.rs`） |
| `Node` の構成子 | `enum NodeKind` |
| `BlockType` / `BlockParams` | `node/block.rs` |
| `RefSpec` | `node/reference.rs` |

- JSON への写像は `serde` の隣接タグ（`#[serde(tag = "kind", content = "value")]`）。
  `Node.kind` は `#[serde(flatten)]` で親に平坦化している。
- 生成は `parser::parse_document_raw(&lines) -> RawDoc`。
- 記法ごとの例が `crates/aozora-core/data/conformance/*.json` にある（入力・RawAST・
  Aozora AST の 3 点組）。
