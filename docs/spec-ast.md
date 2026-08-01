# 青空文庫 AST（RawAST / Aozora AST）

aozora-core が用いる 2 種類の抽象構文木について、**木の形そのものではなく、実装から
しか見えない事柄**を書く。

- **RawAST（生AST）** … ソースに忠実な中間表現。行単位・平坦なマーカー・前方参照は
  未解決で、**文字（char）単位の位置情報**を持つ。字句解析＋構文解析の忠実な結果。
- **Aozora AST** … 解決・構造化された正規表現。ブロックが入れ子の木になり、前方参照は
  解決済み、記法マーカーは型付きノードに畳まれる。HTML／プレーンテキストのどちらの
  バックエンドからも描画できる（＝backend-neutral。旧称「中立AST」）。

## 詳細の在り処

構成子の一覧・フィールド・不変条件は交換形式の仕様が持つ。**そちらは
`tools/verify_ast_spec.rb` が実装と突き合わせて検査する**ので、二重に書くと本書側だけが
黙って古くなる。

| 知りたいこと | 見る文書 |
|---|---|
| RawAST の器・`Node` の全構成子・マーカー・`BlockParams`・`RefSpec`・不変条件 | [spec-rawast-json.md](spec-rawast-json.md) |
| Aozora AST の `Block` / `BlockKind` / `Inline` / 互換メタデータ・不変条件 | [spec-aozora-ast-json.md](spec-aozora-ast-json.md) |
| 字句トークン（`TokenKind`）と走査の手続き | [spec-tokenizer.md](spec-tokenizer.md) |
| 前方参照の解決順序と規則 | [spec-reference-resolver.md](spec-reference-resolver.md) |
| Rust の型との対応 | 上記 2 つの交換形式仕様の付録 A |

本書に残すのは、パイプライン上の位置づけ、**span の意味論**（実在しない span がどこに
混じるか）、Lowerer の挙動、2 つの木の使い分け。いずれも JSON には現れないか、形を
見ただけでは分からない事柄である。

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
- 図は**節 1 つ**の流れ。文書 1 本は節（本文・本文終わり後・底本情報）に分かれ、
  RawAST はファイルの全行を 1 つの列で持ち、Aozora AST は節ごとに木を持つ。
  その器と JSON 交換形式は `interchange.rs`（docs/spec-rawast-json.md・
  docs/spec-aozora-ast-json.md）。`html::convert` もこの器を通る。

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
- **RawAST** … `RawLine.nodes[i]` は `Node`。各生ノード自身が char 精度の位置（`.span`）を
  持ち、行のノード列はその行を隙間なく覆う（不変条件は spec-rawast-json.md「位置」）。
- **Aozora AST** … 各 `Block` は由来行番号 `line: usize`（本文 0 起点）を持つ。`Inline` も
  span を持つが、下記のとおり実在しないものが混じる。エディタ支援 `analysis` は RawAST
  の span を使う。

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
- **別原点**: 注記の中身（`Note { content }` / `Okurigana { content }`）は、Lowerer が
  注記文字列を再 tokenize/再 parse して作る。この `content` の span は**注記文字列内の
  相対位置**であって行内の絶対位置ではない（前方参照の解決に失敗した注記など、`raw` が
  ソース行の部分文字列とは限らないため写せない）。継承・合併と違い**行内の値として
  読めてしまう**ので、位置として使う側は注記の内側に降りたかを意識すること。
- **合併**: ルビ親文字の後方吸収では、親文字が `《》` より**前**の本文に在る。Ruby ノードの
  span は `union(親文字spans, 読みspan)` に広げる。子（親文字）は自身の前方の span を保つので、
  「子が親マーカーより前にある」ことが正しく表れる。
- **区切りの引き受け**: アクセント `〔…〕` はブロックとして木に残らず中身だけが並ぶので、
  素朴に実装すると区切りの 2 文字がどのノードにも属さない。`parser::widen_to_delimiters`
  が先頭ノードの始点と末尾ノードの終点をブロックの端まで広げて引き受けている
  （行を隙間なく覆う不変条件のため）。
- **別の行が混じる（行マージ）**: 参照実装には出力を飛ばしてバッファを次の行へ持ち越す
  経路が 2 つあり（未閉じ `〔` の行と、ぶら下げを開く行。後述「行マージ」）、Lowerer は
  それを写して**2 つのソース行を 1 つの `Block::Line` に畳む**。このとき前半のインラインの
  span は**前の行**の位置、`Block.line` は**後の行**の番号になる。`Block.line` を原点にして
  span を引くと誤った行に着地するので、行マージした行では char 位置を使わないこと
  （`UnclosedAccentBreak` がその境目に入っているので、含む行はマージ済みと分かる）。

---

## RawAST → Aozora AST 変換（Lowerer）

`lower::lower_to_blocks(&RawDoc) -> Vec<Block>` が担う。参照実装の `@indent_stack` /
`implicit_close` / `@terprip` の逐次モデルを畳み込みで再現する。

- 各行の先頭マーカー（`BlockStart`/`LineJisage`）で入れ子ブロックを開き、`BlockEnd` や
  競合ブロックの出現で閉じる（参照実装 aozora2html の `close_conflicting_blocks` 相当）。
- 同じ行に本文があるブロックは `LineWrap`、複数行を包むものは `Nested`。
- 閉じ切られなかったブロックは文末で閉じる（`CloseKind::Newline`）。
- 行末 `<br />`（`Break`）と閉じの出力形（`CloseKind`）を**この時点で確定**し、以降の描画は
  状態を持たない。これらの互換メタデータの意味は spec-aozora-ast-json.md「互換メタデータ」。
- `UnresolvedReference` は解決器が対象を前方に探して最終ノードにする（見つからなければ
  `Note`）。解決は **`resolve_references`（親文字→注記範囲→前方参照）→ `resolve_inline_ruby`
  （ルビ親文字の 2 パス目）の順に 2 回**走らせる必要がある。前方参照の照合はルビの親文字を
  見るので親文字解決が先に要り、逆に親文字が装飾タグになる場合は前方参照の解決が先に要る、
  という相互依存のため。詳細は [spec-reference-resolver.md](spec-reference-resolver.md)。

### 行マージ（参照のバッファ持ち越し）

参照実装には**出力を飛ばしてバッファを次の行へ持ち越す**経路が 2 つある。どちらも
1 ソース行が単独では出力にならず、次の行と 1 つの出力単位にまとまる。

- `apply_burasage` は先頭で `@noprint = true` を**無条件に**立て、`general_output` は
  `@noprint` のときバッファを流さずに return する。折り返し開始行は出力されず、開始タグの
  **前後**にあった本文はどちらも次の行と同じ per-line ぶら下げ div に入る。
- `AccentParser#general_output` は対応する `〕` が無いまま行末に達すると、文字列
  `"<br />\r\n"` を積んで戻る（改行は AccentParser が食べている）。参照が**旗ではなく
  内容**として持つのに合わせ、こちらも行の末尾に `UnclosedAccentBreak` を置く
  （`TokenKind`/`NodeKind`/`InlineKind` に同名の変種があり、素通しで写る）。
  Lowerer はその末尾ノードを見て次の行とマージする。

Lowerer は行ループに持ち越し（`carry`）を持ち、これらの行の内容を次の行の先頭へ繰り越す。
持ち越しを繋げるのは内容行だけで、他の行種に当たったら持ち越しを独立した行として出す。

この畳み込みだけが「1 ソース行 = 1 `Block::Line`」を崩す。位置情報への影響は上述の
「span が『実在』でない箇所」を参照。

### 変換に影響しない診断

同一行で閉じられなかったアクセントの位置は、木ではなく別に返す
（`parse_document_raw_with_diagnostics` → `Vec<ParseDiagnostic>`）。変換出力に影響しない
検証用の副産物を木に混ぜないための分離で、Lowerer の `LowerDiagnostic` と同じ考え方。

## 2 つの AST の使い分け

| 用途 | 使う AST | 理由 |
|------|----------|------|
| HTML / プレーンテキスト描画 | **Aozora AST** | 解決済み・入れ子・状態レスに描ける |
| エディタ支援（ハイライト・診断・アウトライン, `analysis`） | **RawAST** | 各生ノードに char 単位 span（`nodes[i].span`）と原文を持つ |
| 位置 → 意味の対応（将来の LSP） | 両方 | RawAST の span で位置特定、Aozora AST で構造理解 |

関連ドキュメント: [`plan-lsp.md`](plan-lsp.md)（`analysis` レイヤと LSP 設計）。
