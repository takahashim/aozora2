# 青空文庫 AST 仕様（RawAST / Aozora AST）

本書は aozora-core が用いる 2 種類の抽象構文木（AST）を定義する。

- **RawAST（生AST）** … ソースに忠実な中間表現。行単位・平坦なマーカー・前方参照は
  未解決で、**文字（char）単位の位置情報**を持つ。字句解析＋構文解析の忠実な結果。
- **Aozora AST** … 解決・構造化された正規表現。ブロックが入れ子の木になり、前方参照は
  解決済み、記法マーカーは型付きノードに畳まれる。HTML／プレーンテキストのどちらの
  バックエンドからも描画できる（＝backend-neutral。旧称「中立AST」）。

## 命名について

「Aozora AST」は青空文書の**意味構造そのもの**を表す正規モデルの名前とする。「中立
（neutral）」は HTML でもプレーンテキストでも描けるという*性質*であって名前ではないため、
本書では性質は "backend-neutral" と呼び、木の名前を **Aozora AST** に統一する。RawAST は
その手前の**素材**（source-faithful な生表現）という位置づけ。Rust の型名は木の構成要素
として `ast::Block` / `ast::Inline` をそのまま用いる（`ast` モジュール＝Aozora AST）。

## パイプライン上の位置

```
source
  │  tokenize
  ▼
Vec<Token>                     … 字句（各トークン自身が行内 char span を保持）
  │  parse_raw_nodes
  ▼
RawAST : RawDoc { Vec<RawLine{ source, nodes:Vec<Node>, line_no }> }
  │  resolve_references → resolve_inline_ruby（前方参照とルビ親文字の解決。2 パス）
  │  ＋ lower_to_blocks（行→入れ子ブロックへの畳み込み）
  ▼
Aozora AST : Vec<Block>        … 解決済み・入れ子・型付き
  │  BlockRenderer / plain renderer
  ▼
HTML   /   プレーンテキスト
```

- RawAST は「1 行を忠実にパースした平坦ノード列」の集まり。ブロックの開始/終了は
  `BlockStart`/`BlockEnd`/`LineJisage` という**行内マーカー**で、行をまたぐ対応付けは
  まだ無い。`《》` などの前方参照も `UnresolvedReference` のまま。
- Aozora AST は Lowerer（`lower::lower_to_blocks`）がマーカーを入れ子構造に畳み、参照を
  解決した結果。描画は状態を持たない木歩きで行える。

---

## 共通: 位置情報

```rust
/// ソース行内の char 単位の範囲 [start, end)（半開・0 起点）。byte でなく char 数。
struct Span { start: usize, end: usize }
/// Token と Node は種別と行内 char 範囲を自前で持つ。
struct Token { kind: TokenKind, span: Span }
struct Node { kind: NodeKind, span: Span }
```

- **Token** … `tokenize` は `Vec<Token>` を返す。入れ子を含む全トークンが行内絶対spanを持つ。
- **RawAST** … `RawLine.nodes[i]` は `Node`。各生ノード自身が char 精度の位置
  （`.span`）を持つ（旧: `nodes` と並行配列 `spans` の 1:1 対応）。
- **Aozora AST** … 各 `Block` は由来行番号 `line: usize`（本文 0 起点）を持つ。char 精度の
  範囲は持たない（必要なら RawAST 側の `nodes[i].span` を参照する。エディタ支援
  `analysis` は RawAST の span を使う）。

行番号 `line_no` / `line` はいずれも**本文（`extract_body_lines` 後）における 0 起点**。

span は位置メタデータなので、`PartialEq` は `kind` だけを比較する（span は無視）。外付けの
器（`Spanned<T>{node,span}`）ではなく各構文要素の intrinsic なフィールドにしてあるのは、
入れ子（ルビ内容・アクセント内容）まで位置を通すため。入れ子は再トークナイズ時に
base offset を渡すので、子も**行内の絶対 span** を持つ。

### span が「実在」でない箇所（継承・合併）

**Token の span は常に実在する**（全トークンが消費した char に対応する）。一方 **Node と
Inline には実在しない span が混じる**ので、位置として使う側は注意が要る。

- **継承**: 解決器が**合成した**ノードは、元の char 範囲を持たないので**親の span を継承**する。
  - `parse_annotation_text`（`reference_resolver.rs`）… 注記文字列を再 tokenize/再 parse して
    Text/Gaiji を新規生成する。元行の char 位置とは無関係。
  - `node/reference.rs` の `SideNote` / `AnnotationRuby` / `EmbeddedGaiji` … ルビ内容を親文字数
    だけ繰り返し生成する、または注記文字列から生成する。
- **合併**: ルビ親文字の後方吸収では、親文字が `《》` より**前**の本文に在る。Ruby ノードの
  span は `union(親文字spans, 読みspan)` に広げる。子（親文字）は自身の前方の span を保つので、
  「子が親マーカーより前にある」ことが正しく表れる。

---

## RawAST 仕様

### 器

```rust
struct RawDoc  { lines: Vec<RawLine> }
struct RawLine {
    source: String,             // もとのソース行（くの字点走査などで参照）
    nodes:  Vec<Node>,          // 生ノード列（前方参照は未解決）。各Nodeが char 位置範囲を持つ
    line_no: usize,             // 本文 0 起点の行番号
}
```

### 字句トークン（`Token`）

パーサ入力。RawAST の素材で、AST ではないが位置情報の起点なので併記する。

| Token | 意味 |
|-------|------|
| `Text(String)` | 通常テキスト |
| `Ruby { children }` | 暗黙ルビ `《…》`（親文字は直前 Text） |
| `PrefixedRuby { base_children, ruby_children }` | 明示ルビ `｜親《ルビ》` |
| `Command { content }` | 注記 `［＃…］` |
| `Gaiji { description, had_igeta }` | 外字 `※［＃…］`（`＃` は任意） |
| `Accent { children }` | アクセント分解 `〔…〕` |
| `RubyPrefix` | `｜` の一時マーカー。畳み込みパスで `PrefixedRuby` になるか `Text("｜")` に戻るので、**`tokenize()` の出力には現れない**（[spec-tokenizer.md](spec-tokenizer.md)） |

### 生ノード（`Node`）

RawLine を構成する平坦ノード。**入れ子はマーカーで表し**、前方参照は未解決で残す。

| Node | 種別 | 意味・備考 |
|------|------|------------|
| `Text(String)` | 葉 | プレーンテキスト |
| `Ruby { children, ruby, direction, keep_gaiji_notes_in_base }` | 葉 | ルビ（親文字＋ルビ＋方向） |
| `Style { children, style_type }` | 葉 | 傍点・傍線・太字など（`StyleType`） |
| `Midashi { children, level, style }` | 葉 | インライン見出し（同行・窓） |
| `Gaiji { description, unicode, jis_code, had_igeta }` | 葉 | 外字 |
| `Accent { code, name, unicode }` | 葉 | アクセント分解文字 |
| `Img { filename, alt, is_photo, width, height }` | 葉 | 画像（挿絵/写真） |
| `Tcy { children }` | 葉 | 縦中横 |
| `Keigakomi / Yokogumi / Caption { children }` | 葉 | 罫囲み/横組み/キャプション（インライン） |
| `Warichu { upper, lower }` | 葉 | 割書き（上段・下段） |
| `FontSize { children, size_type, level }` | 葉 | 大きな/小さな文字 |
| `Kaeriten(String)` / `Okurigana(String)` | 葉 | 返り点／訓点送り仮名 |
| `Note(String)` | 葉 | 編集者注（Aozora AST では中身を解決して `Note { content, raw }` にする） |
| `DakutenKatakana { num }` | 葉 | 濁点片仮名参照 |
| `LineJisage { width }` | マーカー | 行単位字下げ `［＃N字下げ］` |
| `BlockStart { block_type, params }` | マーカー | ブロック開始（`ここから…`／範囲開始） |
| `BlockEnd { block_type, params, explicit_close }` | マーカー | ブロック終了（`explicit_close`＝`ここで…終わり`か） |
| `AnnotationEnd { prefix, content, suffix }` | マーカー | 左注記範囲の終了（外字を含みうる） |
| `UnresolvedReference { target, spec, raw }` | 未解決 | 前方参照。解決器が `spec` を対象に適用 |

補助:

- `RubyDirection` = `Right` | `Left`
- `BlockType`（マーカーの種別）= `Jisage, Chitsuki, Jizume, Keigakomi, Midashi, Yokogumi,
  Futoji, Shatai, FontDai, FontSho, Tcy, Caption, Warigaki, Warichu, Burasage, Style,
  AnnotationRange, LeftAnnotationRange`（全 18 種）
- `BlockParams { width, wrap_width, level, midashi_style, font_size, style_type, is_block,
  has_open_paren, has_close_paren, annotation }`（開始/終了タグ生成に必要な素材。全 10 項目）

### 前方参照の指定（`RefSpec`）

`UnresolvedReference.spec`。対象テキストにどう作用するかを表す。

| RefSpec | 意味 |
|---------|------|
| `Style(StyleType)` | 対象に傍点・傍線などを付す |
| `Midashi { level, style }` | 対象を見出しにする |
| `FontSize { size_type, level }` | 対象を大/小文字にする |
| `Inline(InlineKind)` | 縦中横・罫囲み・横組み・キャプション・返り点・送り仮名など |
| `AnnotationRuby { … }` | 注記をルビとして表示 |
| `SideNote { annotation }` | 傍記（各文字の脇に注記） |
| `EmbeddedGaiji { jis_code, annotation_ruby }` | 句点コード外字。置換形（`annotation_ruby:None`）／注記形（`Some`） |

`RefSpec::resolve(&self, children: Vec<Node>, span: Span)` が対象の子ノードに指定を適用し
最終ノードを生む（`span` は消費した範囲を覆う）。解決器が
対象を前方に見つけられなければ `raw`（元の注記文字列）をそのまま `Note` にする（エディタ
解析 `analysis` はこの未解決を warning 診断にする）。

### RawAST の特徴（不変条件）

1. **ソース忠実**：1 ソース行 = 1 `RawLine`。`source` を保持し、各生ノードは char 単位 span（`nodes[i].span`）を持つ。
2. **平坦**：入れ子は `BlockStart`/`BlockEnd`/`LineJisage` マーカーで表し、木化しない。
3. **未解決**：`《》` 等の前方参照は `UnresolvedReference` のまま。
4. **可逆志向**：位置と原文を残すので、エディタ支援（ハイライト・診断・アウトライン）の基盤。

---

## Aozora AST 仕様

### ブロック（`Block`）

文書は `Vec<Block>`。3 形態。

```rust
enum Block {
    /// 内容の 1 行（インライン列＋行末改行制御）
    Line     { inline: Vec<Inline>, brk: Break, line: usize },
    /// 入れ子ブロック（複数行を包む。字下げ・見出し・罫囲み等）
    Nested   { kind: BlockKind, children: Vec<Block>, close: CloseKind, line: usize },
    /// 行単位のブロック包み（同じ行に本文がある字下げ／地付き）。開き直後の改行も
    /// 内側 <br /> も出ない 1 行 div。
    LineWrap { kind: BlockKind, inline: Vec<Inline>, line: usize },
}
```

### ブロック種別（`BlockKind`）

```rust
enum BlockKind {
    Jisage   { width: Option<u32> },                 // N 字下げ（空幅は None＝Quirk）
    Chitsuki { width: u32 },                          // 地付き／字上げ（右寄せ）
    Jizume   { width: u32 },                          // 字詰め
    Burasage(BurasageGeometry),                       // ぶら下げ（折り返し字下げ）
    Midashi  { level: MidashiLevel, style: MidashiStyle },    // 見出し（後続行を包む）
    Keigakomi,                     // 罫囲み（ブロック形）
    Yokogumi,                      // 横組み（ブロック形）
    Caption,                       // キャプション（ブロック形）
    FontSize { size_type: FontSizeType, level: u32 }, // 大きな/小さな文字（ブロック形）
    Futoji,                                           // 太字（ブロック形）
    Shatai,                                           // 斜体（ブロック形）
}
```

### インライン（`Inline`）

```rust
struct Inline { kind: InlineKind, span: Span }

enum InlineKind {
    Text(String),
    Ruby { base: Vec<Inline>, ruby: Vec<Inline>, direction, keep_gaiji_notes_in_base },
    Style { children, style_type },
    Midashi { children, level, style },               // 同行・窓見出し
    AnnotationEnd { prefix, content, suffix },         // 左注記範囲終了
    Gaiji { description, unicode, jis_code, had_igeta },
    Accent { code, name, unicode },
    Img { filename, alt, is_photo, width, height },
    Tcy { children }, Keigakomi { children },
    Yokogumi { children }, Caption { children },
    Warichu { open: bool, suppress_paren: bool },      // 割り注（開閉マーカー）
    Warigaki { children },                             // 割書
    FontSize { children, size_type, level },
    Kaeriten(String),                                  // 返り点（中身は素の文字）
    Okurigana { content: Vec<Inline>, raw: String },    // 訓点送り仮名（中身は解決済み）
    Note { content: Vec<Inline>, raw: String },         // 編集者注（中身は解決済み）
    DakutenKatakana { num },
    ChitsukiInline { width, children },                // 行途中で開く地付き（行末で閉じる）
    BlockInline { kind: BlockKind, children },          // 同一行で開閉するブロック形コマンド
}
```

全Inlineは自前の行内char spanを持つ。Nodeから直接写るInlineはNodeのspanを引き継ぎ、
開始・終了コマンドを畳む範囲Inlineは消費したマーカーと内容を覆うunion spanを持つ。
spanは位置メタデータであり、構造比較・HTML／プレーンテキスト描画には影響しない。

補助 enum:

- `MidashiLevel` = `O`(大) | `Naka`(中) | `Ko`(小)
- `MidashiStyle` = `Normal` | `Dogyo`(同行) | `Mado`(窓)
- `FontSizeType` = `Dai`(大) | `Sho`(小)
- `RubyDirection` = `Right` | `Left`
- `StyleType` … 傍点・傍線（実線/二重/破線/波/鎖）・太字・斜体・上付き/下付き 等の全 32 種

### 互換メタデータ: `Break` と `CloseKind`

参照実装 aozora2html は「1 ソース行につき 1 つの `\r\n`」を出し、行末 `<br />` の有無を
状態（`@terprip`）で決める。Aozora AST はこの**改行の出方だけ**をメタデータで保持し、
描画器を状態レスにする。

```rust
enum Break     { Br, None }                    // 行末に <br /> を出すか（Line.brk）
enum CloseKind { NoBreak, Newline, BareBreak } // 入れ子ブロック閉じの出力形（Nested.close）
```

| CloseKind | 出力 | 契機 |
|-----------|------|------|
| `NoBreak`   | `</div>` | 兄弟字下げ・行スコープ地付きの implicit_close、先頭終了＋後続本文 |
| `Newline`   | `</div>\r\n` | `ここで…終わり`（explicit）・ぶら下げ開始の暗黙閉じ・EOF 閉じ |
| `BareBreak` | `</div><br />\r\n` | bare `…終わり`（`ここで` 無し）で複数行ブロックを閉じた行 |

これらは HTML 互換のための情報。プレーンテキスト描画は無視してよい（backend-neutral）。

### Aozora AST の特徴（不変条件）

1. **解決済み**：前方参照は解決され、`UnresolvedReference` は残らない。
2. **入れ子**：ブロックは `Nested`/`LineWrap` の木。開始/終了マーカーは消える。
3. **型付き・マーカーレス**：記法は `BlockKind`/`Inline` の型で表され、生の `［＃…］`
   文字列は残らない。編集者注の中身も Lowerer が一度だけ解決して `Note { content }`
   に入れる（`raw` は診断・エディタ支援用に併置するが、描画には使わない）。
   バックエンドはトークナイザ・パーサに依存しない。
4. **backend-neutral**：HTML・プレーンテキストどちらも同じ木から状態レスに描画できる。
5. **行番号を保持**：各ブロックは由来行 `line` を持つ（char 精度は RawAST 側 `nodes[i].span`）。

---

## RawAST → Aozora AST 変換（Lowerer）

`lower::lower_to_blocks(&RawDoc) -> Vec<Block>` が担う。参照実装の `@indent_stack` /
`implicit_close` / `@terprip` の逐次モデルを畳み込みで再現する。

- 各行の先頭マーカー（`BlockStart`/`LineJisage`）で入れ子ブロックを開き、`BlockEnd` や
  競合ブロックの出現で閉じる（参照実装 aozora2html の `close_conflicting_blocks` 相当）。
- 同じ行に本文があるブロックは `LineWrap`、複数行を包むものは `Nested`。
- 閉じ切られなかったブロックは文末で閉じる（`CloseKind::Newline`）。
- 行末 `<br />`（`Break`）と閉じの出力形（`CloseKind`）を**この時点で確定**し、以降の描画は
  状態を持たない。
- `UnresolvedReference` は解決器が対象を前方に探して最終ノードにする（見つからなければ
  `Note`）。解決は **`resolve_references`（親文字→注記範囲→前方参照）→ `resolve_inline_ruby`
  （ルビ親文字の 2 パス目）の順に 2 回**走らせる必要がある。前方参照の照合はルビの親文字を
  見るので親文字解決が先に要り、逆に親文字が装飾タグになる場合は前方参照の解決が先に要る、
  という相互依存のため。詳細は [spec-reference-resolver.md](spec-reference-resolver.md)。

## 2 つの AST の使い分け

| 用途 | 使う AST | 理由 |
|------|----------|------|
| HTML / プレーンテキスト描画 | **Aozora AST** | 解決済み・入れ子・状態レスに描ける |
| エディタ支援（ハイライト・診断・アウトライン, `analysis`） | **RawAST** | 各生ノードに char 単位 span（`nodes[i].span`）と原文を持つ |
| 位置 → 意味の対応（将来の LSP） | 両方 | RawAST の span で位置特定、Aozora AST で構造理解 |

関連ドキュメント: [`plan-neutral-ast.md`](plan-neutral-ast.md)（移行計画）、
[`plan-lsp.md`](plan-lsp.md)（`analysis` レイヤと LSP 設計）。
