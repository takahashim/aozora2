# 中立AST 木化・BlockManager 撤去 実行計画

策定: 2026-07-25 / 状態: 実行中（Phase A 着手）

## 0. 目的（最上位。ぶれたらここへ戻る）

architecture.md §0 の目標＝**100年後に別の誰かが正しいプロセッサを書き直し、
記法とコーパスを拡張し続けられる基盤**。byte-exact は目的ではなく、互換性の
定義（オラクル）を通じた安全網・検証手段である。参照実装 aozora2html（Ruby）は
将来実行不能になりうる前提で、互換のための逐次挙動は**1箇所に隔離し、参照引退時に
まとめて落とせる**形にする。

## 1. 核心的発見（この計画の分岐理由）

参照 aozora2html は「木」ではなく **`@indent_stack`・`@noprint`・`@terprip`・
`general_output` で1行ずつ状態を持って逐次出力するストリーミング状態機械**。
残る互換バグ（div/br 均衡・burasage 開始行マージ・未閉じ span 等）は、この
ストリーミングモデルの副産物で、しばしば不正・不均衡な HTML を生む。

現状の我々は、その逐次判断を **レンダリング時に BlockManager（188行）と出力HTML
文字列の詮索（classify_output_line）で後追い**しており、状態（terprip/noprint）を
表現できずエッジになる。workflow.md も「参照の TagObject 型モデルを持たない限り
安全に再現できない」と限界を記録している。

**帰結**: これは「木モデルが byte-exact と両立しない」のではない。**逐次判断を
『レンダ時の詮索』から『Lower 時の明示計算』へ移せば、木は互換メタデータとして
その判断を保持できる**。よって architecture.md §4 の目標は**変更不要で、実行が
足りていない**（特に Lowerer）。

## 2. 判断：architecture は変更せず実行する（1点だけ明文化を追加）

- §4 の目標（block⊃line⊃inline の中立AST木 ＋ BlockManager撤去）はそのまま正しい。
- **追加で明文化する1点**：互換のストリーミングモデル（`@indent_stack`/`@terprip`/
  `@noprint`/tail の移植）は **Lowerer が所有**し、Line 単位の互換メタデータ
  （`Line.brk` を拡張）として木に載せる。バックエンドは状態を持たない木歩きにする。
  §4.3 が示唆する `Line.brk` を「Lowerer が per-line 出力モデルを丸ごと移植する」
  まで拡張する。

## 3. 現状（着手前スナップショット）

- byte-exact 17406/17509 = 99.41%（error 56, equivalent 1, different 46）。
- `RawAst(Vec<Node>)` / `Ast(Vec<Node>)` はどちらも平坦 `Vec<Node>`（継ぎ目は
  root 型のみ）。RawAST の中身（前方参照未解決・平坦マーカー）は定義どおりで実体は
  ある。未達は (a) raw専用ノード型の分離、(b) 中立AST の木化（Lowerer は今は
  前方参照解決のみ）、(c) BlockManager 撤去、(d) 位置情報（任意）。
- 関連ファイル: `crates/aozora-core/src/parser/mod.rs`（parse_raw/lower/RawAst/Ast）、
  `crates/aozora-core/src/node/mod.rs`（flat Node）、
  `crates/aozora2/src/html/block_manager.rs`（188行）、
  `crates/aozora2/src/html/renderer.rs`（per-line 描画・classify_output_line）。

## 4. 段階計画（各段 byte-exact 悪化0・snapshot 差分で検証）

### Phase A — 決定と文書化（小・挙動不変）
- architecture.md §4.3/§5 に「Lowerer が互換ストリーミングモデルを所有」を追記。
  §6 段2b/2c の「保留」を「着手（契機＝div均衡/burasage の残差）」へ更新。
- 本計画ファイルを追加。コードは触らない。

### Phase B — 中立AST 木と Lowerer の構築（大・本丸）
既存の平坦 `Vec<Node>`（RawAST）は残したまま**並行して**新パイプラインを作り、
全オラクルで byte 一致したら切替（旧 BlockManager 経路を B4 完了まで残す）。

- **B1**: 中立AST 型を新規定義（RawAST の `Node` と分離＝型の壁）。型トリックなし。
  ```rust
  enum Block { Line { inline: Vec<Inline>, brk: Break }, Nested { kind: BlockKind, children: Vec<Block> } }
  enum Inline { Text, Ruby{..}, Style{..}, Gaiji{..}, Img{..}, Tcy{..}, Note{..}, ... }
  ```
  バックエンドは `Block` 木だけを受け、ソース文字列も RawAST マーカーも見られない。
- **B2**: `lower(): RawDoc → Vec<Block>` を実装。平坦マーカー（`BlockStart`/`BlockEnd`/
  `LineJisage`）を Nested 部分木に畳む。参照の `@indent_stack`/`implicit_close`/
  閉じ照合規則を**一度だけ**移植（今 BlockManager がランタイムでやっている分）。
- **B3**: `Break`（互換メタデータ）を Lower 時に計算。`@terprip`/`@noprint`/tail の
  逐次判断を各 Line に載せる（div均衡・burasage 開始行マージ・bare終了br が表現可能に）。
- **B4**: 新 HTML バックエンドを木の**状態なし歩き**で実装。`BlockManager` と
  `classify_output_line` を削除。
- 各 B ステップ: 旧経路と全件出力 diff、byte 一致を確認してから前進（`snapshot.py`）。

### Phase C — 保留していた互換バグの回収（中）
Lowerer に逐次モデルが載るので memory 記録の保留項目が表現可能になり解ける:
- div/br 均衡（2025 等）、burasage 入れ子/開始行マージ（58012 等）、bare終了br
  （4850/43866）。Phase B の副産物として byte-exact が伸びる見込み。

### Phase D — 100年目標の本命（実証）
- 第2バックエンド（plain text か EPUB）を中立AST から実装＝HTML 非依存の実証。
- 残：RawAST 専用ノード型の分離・位置情報（低優先・任意）。

## 5. リスクと安全策

- 最大リスク：Lowerer 書き直しが 99.41% を割ること。→ **並行実装＋全件 byte 一致で
  切替**。close 系は一度に触らない（段2c burasage 移植で 228/176 件悪化した教訓）。
- 中止条件：Phase B で旧経路との byte 一致に到達できないエッジは、そのエッジだけ
  Quirk 隔離して先へ（100%一致は目標でない）。
- 検証手順（不変条件、workflow.md §1）：`cd aozora2 && cargo test`、
  `cd aozora-htmlcheck && cargo build --release && ./target/release/aozora-htmlcheck
  --oracle-dir oracle --baseline baseline-oracle.jsonl --out results-oracle.jsonl`。
  真の互換指標は byte-exact 件数（results の status=="exact"）。
  大規模リファクタは `snapshot.py` で前後の全件出力 byte 一致を確認。

## 6. 進捗ログ（追記のみ）

- 2026-07-25 計画策定。Phase A 着手。
- 2026-07-25 **Phase A 完了**（挙動不変）。architecture.md §4.3 に「互換ストリー
  ミングモデルは Lowerer が所有」、§5 に決定記録（2026-07-25）、§6 段2b/2c を
  「着手・実行中」へ更新。本計画ファイル追加。
- 2026-07-25 **Phase B1 完了**（挙動不変）。`crates/aozora-core/src/ast.rs` に
  中立AST型（Block/BlockKind/Break/Inline）を新規定義。RawAST の Node と別型
  （型の壁）。pub 未使用型なのでオラクル不変・全テスト通過。
- 2026-07-25 **Phase B2 着手時のメモ（次セッションの起点）**: Lowerer の第一歩
  として `to_inlines`（解決済み Node → Inline）から作る想定だったが、正確な移植に
  下記の分岐判断が要ると判明。B2 はこれらを1つずつ実測（参照 Ruby 最小入力）で
  確定しながら進める。急がず、各判断をテストに固定する。
  - **is_block 分岐**: 罫囲み/横組み/キャプションは `ここから…`（ブロック形＝
    BlockKind/Nested）と `［＃罫囲み］…終わり`（インライン形＝Inline）がある。
    現行は `BlockParams.is_block` で区別。中立ASTではインライン形を Inline、
    ブロック形を Nested に振り分ける。
  - **割り注（warichu）**: 現行は BlockStart{Warichu}/BlockEnd{Warichu} マーカー
    だが apply_warichu は状態を持たないインライン出力（開き `（`／閉じ `）`）。
    中立ASTでは Inline::Warichu{open, suppress_paren} マーカーにする（ast.rs 定義済み）。
    Node::Warichu{upper,lower}（二部構成）は別物。構築箇所の有無を要確認。
  - **Midashi/AnnotationEnd**: Node::Midashi（同行/窓見出し＝インライン見出し）と
    Node::AnnotationEnd（左注記範囲終了）を Inline に写す変種が未定義。B2 で
    Inline に追加する（B1 の型は暫定・B2 で精緻化する前提）。
  - **ブロックマーカー**: BlockStart/BlockEnd（is_block=true）・LineJisage・
    UnresolvedReference は to_inlines には現れない（ブロック畳み込みが消費）。
  次の一歩: `to_inlines` を上記分岐込みで実装＋テスト（挙動不変・未接続）。その後
  ブロック畳み込み（indent_stack モデル移植）→ Break 計算 → 新バックエンド。
- 2026-07-25 **Phase B2 途中: インライン変換完了**（挙動不変・未接続）。
  `ast.rs` に `inline_from_node`/`to_inlines`（解決済み Node → Inline）を実装。
  確定した分岐: 割り注は BlockStart/BlockEnd{Warichu} を `Inline::Warichu{open,
  suppress_paren}` マーカーに写す。`Node::Warichu{upper,lower}` は**構築箇所ゼロの
  デッドコード**と確認（無視）。Midashi/AnnotationEnd を `Inline` に追加。ブロック
  構造マーカー（is_block=true の BlockStart/BlockEnd・LineJisage・
  UnresolvedReference）は None（畳み込みが消費）。テスト3本（純インライン・割り注
  マーカー・ブロックマーカー除外）。**残: 罫囲み等インライン形ブロック（is_block=false
  の開閉対）の入れ子化はブロック畳み込み側で対にする。**
  次の一歩: ブロック畳み込み `lower_to_blocks(RawDoc)→Vec<Block>`（indent_stack /
  implicit_close の移植、インライン形ブロックの対化、行と Nested の構築）。
