# 設計メモ: span を各構文要素の intrinsic フィールドにして Inline まで伝播する

作成 2026-07-26（intrinsic 方針で全面改稿）。目的は「ソース位置 span を、トークンから
Aozora AST の `Inline` まで、**各構文要素が自前で持つ**形で運び、プレビュー↔ソースの
相互対応・位置精度の高い診断を可能にする」ための設計と段階計画。段1・段2を実装済みで、
残る対象はInlineのintrinsic化である。

関連: [spec-ast.md](spec-ast.md)、[plan-lsp.md](plan-lsp.md)、[plan-neutral-ast.md](plan-neutral-ast.md)。

---

## 1. 方針転換の要旨

先行実装では位置を外付けの器 `Spanned<T>{node,span}` で持ち、トークン列と
`RawLine.nodes` に付けた。しかし「Inline まで（入れ子含む）span を通す」
目的には、**位置を各構文要素の intrinsic フィールドにする**方が素直だと判断した。

- **すべてのトークンは入力 char を消費して作られる ⇒ 必ず実在の `[start,end)` を持つ。**
  span を持てないトークンは存在しない（空ルビ `《》`＝テキスト化ですら消費 char に対応）。
- 入れ子（ルビ内容・アクセント内容）も、再トークナイズ時に **base offset** を渡せば
  **絶対 span** を復元できる。「入れ子に位置が無い」のは旧 `tokenize_children` が捨てていた
  実装都合であって本質ではない。
- 位置をフィールドで持つと、splice・合併・分割の多い変換を跨いでも **span は値と一緒に運ばれ**、
  合成/分割の地点だけ計算すればよい。ラッパー方式（各変換で付け直し）より落としにくい。

→ **`Spanned<T>` は本方針では置き換え対象**（直近コミットはブランチ上なので破棄容易）。

## 2. 「実在 span」が厳密に成り立つ境界

| レベル | span の実在性 |
|---|---|
| **Token** | **常に実在**。全トークンが消費 char に対応。入れ子も base offset で絶対化可能。 |
| **Node** | **概ね実在**。多くは token 由来。ただし resolve が**合成するノード**（下記）は単一の元 char 範囲を持たず、**親からの継承 span**を与える。 |
| **Inline** | Node と同様（実在＋一部継承）。 |

**合成ノード（継承 span になる箇所）**:
- `parse_annotation_text`（`reference_resolver.rs`）… 注記文字列を**再 tokenize/再 parse**して
  Text/Gaiji を新規生成。元行 char 位置と無関係。
- `node/reference.rs` の `SideNote`/`AnnotationRuby`/`EmbeddedGaiji` … ルビ内容を親文字数だけ
  **繰り返し生成**、または注記文字列から生成。

**親が子より広がる/ずれる箇所**（合併が要る）:
- ルビ親文字の後方吸収（`resolve_inline_ruby`）… 親文字は 《》 より**前**の本文に在る。
  Ruby ノードの span は `union(親文字spans, 読みspan)` に広げる。子（親文字）は自身の
  （前方の）span を保つ＝「子が親マーカーより前」を正しく表す。

## 3. carrier 設計（intrinsic：`kind` ＋ `span`）

各レベルを「種別 enum＋span」の構造体にする。等価比較は `kind` に委譲して span を無視する。

```rust
// token.rs
pub struct Token { pub kind: TokenKind, pub span: Span }
pub enum TokenKind {
    Text(String),
    Ruby { children: Vec<Token> },              // 子も Token（span 付き）
    PrefixedRuby { base_children: Vec<Token>, ruby_children: Vec<Token> },
    Command { content: String },
    Gaiji { description: String, had_igeta: bool },
    Accent { children: Vec<Token> },
}
impl PartialEq for Token {                        // span を無視（位置はメタデータ）
    fn eq(&self, o: &Self) -> bool { self.kind == o.kind }
}
```

`Node`/`Inline` も同型（`Node{kind:NodeKind,span}` / `Inline{kind:InlineKind,span}`）。

### なぜ `kind` 委譲の `PartialEq` か
トークンテストはspanを除いた構造期待値をヘルパーで比較する。span を
フィールドに入れると素の derive では span 違いで壊れる。`PartialEq` を `kind` 委譲にすれば、
`TokenKind` の derive 比較が子 `Vec<Token>` を（span 無視の）`Token::eq` で再帰比較するので、
**span を無視した構造比較が全階層で成り立つ**。テストの構造比較ではspanを見ない。`Hash` は使っていない（`Hash` を
足す場合は `kind` のみでハッシュして整合を取る）。

代替として「enum を保ったまま各 variant に `span` を足す」案は、構築・match の全面改修に加え
span を無視する等価が書けず（variant ごとに手書き）、**非推奨**。

## 4. 入れ子の絶対 span（base offset の伝播）

`tokenize_children` は内容部分文字列を新しい `Tokenizer` で再トークナイズしている。ここに
**元行内での開始 char オフセット**を渡し、子トークンの span を絶対位置にする。

```rust
// 概念。new_top_level / new に base offset を持たせ、span を out.push 時に +base する。
fn tokenize_children(input: &str, base: usize) -> Vec<Token> { ... }
// 呼び出し側は content の開始 char 位置を base に渡す（read_ruby なら 《 の次の位置、
// read_accent なら content_start、read_prefixed_ruby なら base_start / 《 の次）。
```

これで**入れ子まで含めた全トークンが行内絶対 span を持つ**（フロンティア B がトークナイザ
段で解決）。ルビ親文字の**後方吸収**分だけは resolve 段で本文 Node から取り込むため、その
span は §5 の合併で計算する（トークナイザだけでは決まらない）。

## 5. 中間変換での span 演算（token→node→inline）

`resolve_*` と `to_inlines` は結合（N:1）・分割（部分文字列切り出し）・合成（再パース）を行う。
intrinsic なら「値に付いた span を運びつつ、下記の地点だけ計算」すればよい。（file:line は
調査時点。）

| 変換 | 場所 | span 演算 |
|---|---|---|
| 前方参照の対象範囲確定 | `reference_resolver.rs` `search_front_reference` (303-354) | Text の `strip_suffix` で**途中分割**(316)。切った char 数で prefix=`[start, start+len)` / target=`[start+len, end)` に**分割**。要: 切り出し関数に char オフセットを返させる |
| 前方参照の適用 | 同 `apply_front_reference` (358-378) | 対象 Node 群を装飾1個へ **合併**（`[min start, max end)`）。prefix Text は分割 span |
| 参照解決失敗 | 同 (230) | 1:1 `UnresolvedReference`→`Note`。**span 保存** |
| 注記付き範囲 | 同 `resolve_annotation_ranges` (121-190) | マーカー対の範囲を **合併**（Ruby）／Note+内容+AnnotationEnd（新規は継承） |
| ルビ親文字吸収 | 同 `resolve_inline_ruby` (28-77) / `ruby_parser.rs` `extract_ruby_base` (35-77) | 親文字は文字種境界で Text を**分割**。Ruby は `union(親文字, 読み)` に**合併** |
| 注記文字列の再パース | `parse_annotation_text` (403) ほか | 元 span 無し。**継承**（親マーカー/参照の span） |
| 行→Block | `lower.rs` `classify_line`/畳み込み | Block は `line` 番号を保持。char span は Inline 側。先頭マーカー削除(119,209)は span に影響させない |
| Node→Inline | `ast.rs` `to_inlines` (164)/`inline_from_node` (33) | 大半 1:1（**span 引き継ぎ**）。範囲コマンドは `BlockStart…BlockEnd` を **合併**(172,188,203) |
| くの字点 | `html/mod.rs` `scan_kunoji` | 生行を走査しフラグを立てるのみ。文字不変＝**span ずれ要因でない** |

**span 演算は3種だけ**: 合併（`[min,max)`）・分割（char 長で割る）・継承（親の span）。

## 6. バイト一致への影響

**span は出力に載せない限りオラクルに影響しない。** 変換の *値*（`kind`）を変えず span という
メタデータを運ぶだけなので、17361 exact は保たれる。リスクは「出力変化」ではなく
「**span 演算のバグ**（行外・逆転・欠落）」。各フェーズで以下の不変条件テストとオラクルを確認:
- 全 span が `end <= line.chars().len()`（行内）かつ `start <= end`。
- 子の span は親の span に**包含**される（吸収で親を広げた後も成立）。
- 同階層の兄弟は原文順に概ね単調（重なり最小）。

## 7. 段階計画（各段末でオラクル 17361 とテスト緑を確認）

- **段0（本メモ）**: 方針合意。`Spanned<T>` は intrinsic に置換する前提を確定。
- **段1: `Token{kind,span}` へ intrinsic 化（完了）**
  - `Token`→`TokenKind`＋`span`、`PartialEq` を kind 委譲。`Spanned<Token>` を廃止し
    `tokenize -> Vec<Token>`（各要素が span を持つ）に。
  - `tokenize_children` に base offset を導入 → **入れ子含む全トークンが絶対 span**。
  - テストはspanを除いた構造期待値で比較し、span自体は専用テストで検証。
  - 成果: トークン層の位置情報が完成（フロンティア B の土台）。
- **段2: `Node{kind,span}` へ intrinsic 化（完了）**
  - `parse_raw_nodes` で token span を Node に引き継ぐ。`RawLine` は `nodes: Vec<Node>`
    （`Spanned<Node>` を廃止。span は Node 内）。
  - `resolve_*` を §5 の合併/分割/継承で span 保持対応に（本工事の中心）。
  - `analysis` は `node.span` を直接使う（現行の `Spanned` 経由から移行）。
- **段3: `Inline{kind,span}` へ intrinsic 化**
  - `to_inlines`/`inline_from_node` で node span を Inline に引き継ぐ（範囲畳み込みは合併）。
  - **描画器は `inline.kind` を見る**ため出力不変（span はメタデータ）。
  - `Block::Line{inline: Vec<Inline>}` はそのまま（Inline が span を内包）。
- **（任意）段4: source-map 出力**
  - GUI プレビュー用に HTML へ `data-*` で span を載せ、クリック→エディタ該当箇所へ。
    ここで初めて出力が変わるので editor 専用出力とし、オラクル系統とは分離。

## 8. 規模とリスク（正直な見積り）

- **段1（Token）**: 中規模。`TokenKind` 化で tokenizer/parser の match・構築が対象。テスト書換多め。
- **段2（Node）**: **最大工事**。`Node` は parser/resolver/lower/ast/html renderer 全域で使われ、
  `NodeKind` 化＝match・構築が数百箇所。resolve の span 演算実装もここ。**最もリスクが高い段**。
- **段3（Inline）**: 中規模。`InlineKind` 化で renderer の match が対象（`.kind` 参照へ）。出力不変。

段2 が重いので、**段ごとに独立でオラクル緑を保ちながら**進め、各段を別コミットにする。
途中で「Token だけ intrinsic・Node/Inline は Spanned 折衷」に切り替える判断も段1完了後に可能。

## 9. 未決事項（実装前に確認）

1. carrier は `kind`＋`span` 構造体＋`PartialEq` kind 委譲でよいか（等価は span 無視）。
2. `Node`→`NodeKind` の大工事（段2）を許容するか。段1（Token）だけ先に入れて評価してから
   段2 の可否を決める進め方でよいか。
3. `Hash` 依存は無い想定だが、もしあれば kind のみでハッシュする方針でよいか。
4. 段ごとに別コミット・オラクル確認、で進めてよいか。
