# 中立AST 木化・BlockManager 撤去 実行計画

策定: 2026-07-25 / 状態: 実行中（Phase B4・新経路 body 被覆率 96.6%・未接続）

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
- 2026-07-25 **設計上の学び（B2 畳み込み以降の進め方）**: `to_inlines` は純粋な
  局所写像なので単体テストで正しさを確定できた。しかし**ブロック畳み込み
  （`lower_to_blocks`）は正しさが文書全体の逐次モデル（indent_stack/@terprip/
  @noprint/burasage per-line）に依存し、単体テストだけでは確定できない**。畳み込み
  だけ先に投機実装すると、新バックエンド（B4）でオラクル byte 検証して初めて誤りが
  発覚し手戻りになる。したがって **B2（畳み込み）＋B3（Break）＋B4（新バックエンド）は
  密結合の垂直スライスとして、オラクルで byte 検証しながら一体で作る**。旧
  BlockManager 経路を残したまま、新経路の出力を旧経路と全件 diff して一致させてから
  切替える（並行実装＝§5 安全策）。この一体スライスは腰を据えて取り組む単位なので、
  着手時は「まず jisage だけの最小文書で新経路→HTML→旧経路と byte 一致」を作り、
  記法を1種類ずつ増やして全件一致まで広げる（各段オラクル悪化0）。
- 2026-07-25 **垂直スライス最小核 稼働（jisage）**。`lower.rs`（`lower_to_blocks`）と
  `block_renderer.rs`（`render_body_blocks`）を新設し、jisage の Nested・内容行・
  Inline::Text を実装。旧経路（convert の main_text 内側）と新経路が byte 一致する
  比較テスト3本（単純 jisage・兄弟 jisage＝implicit_close・順次 jisage）。確定した
  互換モデル: (1) **implicit_close** ＝新 jisage を開くとき最上位が jisage/burasage
  なら閉じてから開く（兄弟）、(2) **閉じタグ直後の改行**は explicit_close（`ここで…
  終わり`＝`</div>\r\n`）か暗黙閉じ（次の開きと同じ行＝`</div>`）かで変わる →
  `Block::Nested.explicit_close` を互換メタデータに追加。新経路は未接続でテストのみ
  使用（オラクル 17406 不変）。
  次の一歩: 記法を1種類ずつ拡張。優先: (a) 内容行の Break（terprip：見出し・
  `ここで…終わり` 行の `<br/>` 抑制）、(b) インライン（Ruby/Gaiji/Style… を
  block_renderer に足す＝旧 node_renderer のインライン描画の中立AST版）、(c) chitsuki/
  jizume/font/burasage の Nested。各段で比較テストを足し byte 一致を保つ。最終的に
  render() 全体を新経路へ切替え（全オラクル byte 一致確認後）BlockManager を撤去。
- 2026-07-25 **本文一致率の計測ハーネス＋現状測定（重要）**。`aozora2 body-diff`
  サブコマンドと `html::compare_body`（旧 convert の main_text 内側 vs 新
  lower_to_blocks→render_body_blocks）を追加。コーパス3000件サンプルで測定:
  - Jisage＋基本インライン（Text/Style/Tcy/罫囲み/横組み/キャプション/割書/Ruby）
    のみ: **MATCH 34.2%**。
  - chitsuki/jizume/keigakomi/yokogumi/caption ブロックを追加: **34.3%（+0.1%＝+1件）**。
  → **機械的ブロックは被覆率をほぼ動かさない。ボトルネックは Gaiji（ほぼ全作品が
    使う）と intricate な逐次挙動（burasage per-line 包み・Break/terprip・
    Note 再帰・Img/Accent の register_alt_gaiji 副作用・フッタ表記について蓄積）。**
- 2026-07-25 **重大な問題（測定に基づく結論）: render() 全体の切替は本セッションでは
  安全に完了できない。** 理由:
  - 新経路が旧経路と byte 一致するのは現状 34%。残り 66% を一致させるには、旧
    node_renderer（~700行）+ renderer（~450行）+ tag_generator + block_manager の
    **状態付き逐次ロジックを丸ごと中立AST版に再実装**する必要がある（Gaiji の
    has_gaiji_images/has_jisx0213/unconverted_gaiji/gaiji_alt(quirks)/in_ruby_base、
    Accent の has_accent＋名前quirk、Img の register_alt_gaiji 副作用、Note の再帰
    描画、burasage の per-line 包み＋blank_type、Break の classify_output_line、
    Midashi の id カウンタ、フッタ表記について、tail の after_text/bibliographical、
    同行本文つきブロック開始、LineJisage 等）。
  - これは複数セッション規模。**100% 一致に達する前に render() を切替えると 66% の
    作品が悪化し、プロジェクトの神聖な不変条件「悪化0」を破る。** よって部分切替は
    不可、全再実装が前提。
  - 本セッションで確立したもの（安全に継続可能な土台）: (1) 新パイプライン
    lower_to_blocks/render_body_blocks が参照の逐次挙動（implicit_close・
    explicit_close の改行）を**クリーンな木＋メタデータで byte 再現できることを実証**、
    (2) `body-diff` 計測ハーネスで被覆率を定量化・ギャップを特定できる、
    (3) 「機能→block_renderer に厳密移植→比較テストで byte 一致」の反復パターン。
  - 継続手順（次セッション以降）: `body-diff` で被覆率を見ながら、最大レバーの
    **Gaiji** から着手（block_renderer を状態構造体化し node_renderer の gaiji 系
    ヘルパを移植）。次に Note/Img/Accent、burasage per-line、Break、フッタ、tail。
    被覆率が全件一致に達したら render() を新経路へ切替え、BlockManager と
    出力HTML詮索を撤去。各段オラクル悪化0。

- 2026-07-25 **本文被覆率を 34% → 96.6% へ（同一セッション大幅前進・未接続）。** 旧経路
  無変更・oracle 17406 維持・各段でユニットテスト全通過。`body-diff`（3582サンプル・
  byte 一致率）を各段で測りながら最大レバー順に block_renderer / lower / to_inlines /
  ast を厳密移植した。主なマイルストーン（コミット単位）:
  - インライン移植（Note/FontSize/Midashi/Warichu/Okurigana/AnnotationEnd）＋
    LineJisage（同行字下げ）: 41.7% → 57.0%。
  - 行スコープ包み LineWrap で字下げ＋地付き line-form を統一: 57.0% → 74.8%（大レバー）。
  - `to_inlines` を範囲畳み込み対応にし同行コマンド範囲を畳む: 見出し 74.8→75.7、
    装飾/大小文字 75.7→77.7。
  - ブロック形の大小文字・太字・斜体（Nested）: 77.7 → 78.6。
  - ルビ親文字の外字注記を rb 外へ振り分け（render_ruby_base）: 78.6 → 81.4。
  - ぶら下げ per-line モデル（外側 div なし・空行は素 br）: 81.4 → 85.3。
  - 見出し・ブロックのみ行の行末 br 抑制（is_block_only_line を描画時に判定）: 85.3 → 86.0。
  - ブロック形見出し（ここから中見出し等、h4/a、id 共通カウンタ）: 86.0 → 86.3。
  - **行の途中で開く地付き（mid-line chitsuki）を Inline::ChitsukiInline で行末まで包む**:
    86.3 → 95.5（trailing attribution `本文。［＃地付き］（大正…）` が非常に多く最大レバー）。
  - 計測修正（compare_body が after_text/notation_notes を終端に含めず過剰取得していた）:
    実測 95.5 → 96.6。
- 2026-07-25 **残差（約3.4%）の内訳と方針**: 最大クラスタは **ぶら下げの div 収支 quirk**
  （burasage 内の見出し・入れ子ブロックが参照実装で余分な `</div>` を出すストリーミング
  由来。新経路は均衡した綺麗な HTML を出すため byte が食い違う）。ユーザ方針「余分な
  閉じタグは全部 quirks 扱い」＋ memory `div-balance-quirk-category` に該当。残りは
  mid-line jisage・特定文脈の欧文引用符など少数の一点物。
- 2026-07-25 **render() 切替に向けた次の分岐点**: 悪化0 を守るには、切替時に *現在 oracle を
  通っている作品すべて* で新 body が旧 body と byte 一致する必要がある。96.6% では不足。
  残る burasage div 収支 quirk を「(a) quirk フラグ付きで参照の余分 `</div>` を再現する」か
  「(b) 当該作品が元々 non-exact であることを確認して除外扱いにする」かを決める必要がある。
  次段: tail セクション（after_text/bibliographical/notation_notes/card）と footer 状態
  蓄積を新経路へ接続 → 全文書 byte 一致を確認 → render() 切替 → BlockManager/renderer/
  node_renderer/tag_generator と出力HTML詮索を撤去。

- 2026-07-25 **(b) 診断＋① 実装＋implicit_close 完全化で切替阻害を 280→52 に（exact 作品
  body 一致 99.70%・overall 99.53%・旧経路無変更・oracle 17406 維持・回帰0）。**
  ユーザ選択（方針1: burasage quirk を局所再現）に沿って精査した結果、**「burasage の
  余分 </div>」の大半は quirk ではなく未実装の implicit_close** だった（重要な発見）:
  - 診断: 現在 exact な 17406 作品で新経路 body-diff を全数実行 → 280 作品が不一致
    （＝切替時に悪化する集合）。仕分けは burasage 222 / mid-line yokogumi 35 / 他。
  - ① 実装: 同行開閉のブロック形（`TEXT［＃ここから横組み］…終わり］`）を
    `Inline::BlockInline` に畳み、`［＃ここで…終わり］`（explicit_close=true）行の
    行末 br 抑制（@terprip）を lower で決定。同行 yokogumi/caption/shatai/tcy/keigakomi/
    warigaki も範囲畳み込み。→ 280→238。
  - **implicit_close 完全化**: 参照 close_conflicting_blocks に合わせ、Burasage 開始で
    Jisage/Burasage を、Chitsuki 開始で Chitsuki/Burasage を最上位から続く限り閉じる
    （従来 Jisage 開始・top1つのみ）。Burasage 開始行は可視タグ無し→暗黙閉じ </div> に
    行末 \r\n（explicit_close=true）。→ 238→52（回帰0）。
  - 残 52（exact 作品の 0.30%）の内訳: 空幅 CSS quirk（`margin-left: em`、empty_indent_css）
    数件・mid-line で開き複数行にまたがるブロック（斜体等）数件・複雑な連続ネストでの
    burasage/jisage 閉じ位置ズレ（真の div 収支 quirk）多数。後2者はストリーミング状態
    由来で、tree では局所再現が難しい残差。
