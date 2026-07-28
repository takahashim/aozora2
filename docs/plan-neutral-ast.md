# Aozora AST 木化・BlockManager 撤去 実行計画

策定: 2026-07-25 / 状態: **Phase B4/C/D 完了・旧スタック撤去済み・HTML/プレーンテキスト共にAozora AST-only**。オラクル 17361（外字注記バグ修正で -2）。

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

- §4 の目標（block⊃line⊃inline のAozora AST木 ＋ BlockManager撤去）はそのまま正しい。
- **追加で明文化する1点**：互換のストリーミングモデル（`@indent_stack`/`@terprip`/
  `@noprint`/tail の移植）は **Lowerer が所有**し、Line 単位の互換メタデータ
  （`Line.brk` を拡張）として木に載せる。バックエンドは状態を持たない木歩きにする。
  §4.3 が示唆する `Line.brk` を「Lowerer が per-line 出力モデルを丸ごと移植する」
  まで拡張する。

## 3. 現状（着手前スナップショット）

- byte-exact 17406/17509 = 99.41%（error 56, equivalent 1, different 46）。
- `RawAst(Vec<Node>)` / `Ast(Vec<Node>)` はどちらも平坦 `Vec<Node>`（継ぎ目は
  root 型のみ）。RawAST の中身（前方参照未解決・平坦マーカー）は定義どおりで実体は
  ある。未達は (a) raw専用ノード型の分離、(b) Aozora AST の木化（Lowerer は今は
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

### Phase B — Aozora AST 木と Lowerer の構築（大・本丸）
既存の平坦 `Vec<Node>`（RawAST）は残したまま**並行して**新パイプラインを作り、
全オラクルで byte 一致したら切替（旧 BlockManager 経路を B4 完了まで残す）。

- **B1**: Aozora AST 型を新規定義（RawAST の `Node` と分離＝型の壁）。型トリックなし。
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
- 第2バックエンド（plain text か EPUB）をAozora AST から実装＝HTML 非依存の実証。
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
  Aozora AST型（Block/BlockKind/Break/Inline）を新規定義。RawAST の Node と別型
  （型の壁）。pub 未使用型なのでオラクル不変・全テスト通過。
- 2026-07-25 **Phase B2 着手時のメモ（次セッションの起点）**: Lowerer の第一歩
  として `to_inlines`（解決済み Node → Inline）から作る想定だったが、正確な移植に
  下記の分岐判断が要ると判明。B2 はこれらを1つずつ実測（参照 Ruby 最小入力）で
  確定しながら進める。急がず、各判断をテストに固定する。
  - **is_block 分岐**: 罫囲み/横組み/キャプションは `ここから…`（ブロック形＝
    BlockKind/Nested）と `［＃罫囲み］…終わり`（インライン形＝Inline）がある。
    現行は `BlockParams.is_block` で区別。Aozora ASTではインライン形を Inline、
    ブロック形を Nested に振り分ける。
  - **割り注（warichu）**: 現行は BlockStart{Warichu}/BlockEnd{Warichu} マーカー
    だが apply_warichu は状態を持たないインライン出力（開き `（`／閉じ `）`）。
    Aozora ASTでは Inline::Warichu{open, suppress_paren} マーカーにする（ast.rs 定義済み）。
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
  block_renderer に足す＝旧 node_renderer のインライン描画のAozora AST版）、(c) chitsuki/
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
    **状態付き逐次ロジックを丸ごとAozora AST版に再実装**する必要がある（Gaiji の
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

- 2026-07-25 **(A) 採択: 空幅CSS quirk 回収＋残差は文書化受容。** BlockKind::Jisage を
  width:Option、Burasage を wrap_width/width:Option にし、empty_indent_css quirk で
  参照の不正CSS（`jisage_`/`margin-left: em`）を compat 時のみ再現。切替阻害 52→49
  （回帰0・exact body 99.72%）。残 49（exact の 0.28%）は真のストリーミング由来 quirk
  （burasage/jisage 閉じ位置ズレ・mid-line で複数行にまたがるブロック）＝文書化受容。
- 2026-07-25 **render() 切替の 悪化0 分岐点（要ユーザ承認）**: 新経路は本文を exact 作品
  17357/17406 で byte 再現。残 49 作品は現在 oracle exact（旧＝oracle）なので、render()
  を新経路へ丸ごと切替えると **その 49 作品が oracle 悪化**する（新≠旧＝≠oracle）。
  よって「文書化受容」は実質「49 作品を quirk 由来の既知悪化として 悪化0 の例外に
  する」合意を意味する。tail セクション統合（after_text/bibliographical/notation_notes/
  card＋footer 状態）は additive で安全に進められるが、**最終フリップには 49 悪化の
  明示承認が要る**。承認まではフリップせず additive（新経路を並行構築・旧経路 live 維持）。

- 2026-07-25 **★ render() 切替 完了・オラクル検証済み（Phase B4 到達）。** convert() を
  `render_via_blocks`（Aozora AST新経路）へ切替。head/metadata/tail 枠は DocumentRenderer
  を再利用、本文＋tail は lower_to_blocks→BlockRenderer（BlockManager 非依存）。footer
  状態（has_notes/gaiji_images/accent/jisx0213/kunoji/dakuten_kunoji/unconverted_gaiji）は
  BlockRenderer が描画副作用で蓄積、scan_kunoji/enter_tail も移植。BlockCloseWithTail
  （先頭 BlockEnd＋後続本文）も実装。
  - **全文書比較（body-diff --full）**: 新経路は exact 作品 17357/17406 を byte 再現、
    tail/footer/head 由来の新規差分ゼロ（差分は本文の既知49と完全一致）。
  - **オラクル実測**: `--oracle-dir oracle` で 17406→17361。**想定外回帰0**（回帰48件は
    全て事前合意済みのストリーミング由来 div 収支 quirk）、うち1件は BlockCloseWithTail
    で解消、さらに非exact→exact 改善3件。旧経路は convert_legacy として差分オラクルに
    保持（compare_body/compare_full が使用）。
  - 残り（撤去 Phase）: convert_line/render_line の新経路化 → BlockManager・renderer(旧
    render)・node_renderer・tag_generator・classify_output_line の撤去。convert_legacy を
    差分オラクルとして当面残すか撤去するかは要判断。48件の quirk 再現は別途 quirk 化 or
    受容。

- 2026-07-25 **撤去 Phase（convert_legacy 保持方針）: 本番経路を全て新経路へ寄せ、旧
  BlockManager スタックを差分オラクル専用に隔離。** convert_line を
  `block_renderer::render_line_inline`（Aozora AST・インライン列）へ移行。これで本番HTML
  （convert / convert_line / html サブコマンド）は完全に新経路。旧
  renderer/node_renderer/block_manager/tag_generator は **convert_legacy（差分オラクル
  compare_body/compare_full）の唯一の入口からのみ到達**（mod.rs に2経路と legacy
  マーカーを明記）。物理削除は convert_legacy を残す間は保留（参照実装引退時／全 quirk
  を新経路へ移行後にまとめて撤去可能）。全テスト通過・オラクル 17361 維持。
  → **Phase B4 実質完了**（BlockManager は本番から排除、木の状態なし歩きが本番経路）。

- 2026-07-25 **Phase C（保留互換バグの回収）実測評価**: 木モデル（Lowerer が逐次判断を
  所有）により保留項目の大半が解けた。
  - **div/br 均衡**: Phase B の implicit_close 完全化で大半を回収（burasage 186件等）。
  - **bare 終了 br（4850/43866）**: CloseKind 導入で解決。旧の文字列判定は176件悪化した
    が、新経路はノードレベル（BlockEnd.explicit_close）判定で**回帰0**。オラクル
    17361→17363（different→exact ×2）。＝木モデル移行の恩恵の実証。
  - **burasage 開始行マージ／改行天付き（58012）**: 1作品・改行天付き＋折り返しの深い
    ストリーミング挙動（body 差分44）。1作品・高リスクのため**保留継続**（memory の
    当初判断どおり割に合わない）。
  → Phase C は tractable な項目を回収し実質完了。残 58012 は文書化済みの 1-doc edge。

- 2026-07-25 **Phase D 着手（新バックエンド不要の範囲）。**
  - **D-B（純度の総仕上げ）**: 行末 br の要否を描画時 `is_block_only_line`（HTML詮索）
    から Lower 時計算へ移設（`ast::line_is_block_only`：末尾が Normal 見出し/
    ChitsukiInline/div系 BlockInline なら抑制）。バックエンドは brk を見るだけになり
    **「状態なし・HTML詮索ゼロの木歩き」テーゼが完成**。オラクル 17363・回帰0・byte 不変。
  - **D-A（第2バックエンド実証）**: 既存 `strip`（プレーンテキスト）をAozora AST経由で
    再実装（`strip::convert_via_ast`、サブコマンド `strip --via-ast`）。HTML と同じ
    tokenize→parse→lower を共有し、終端の木歩きだけ差し替え＝**Aozora ASTはバックエンド
    非依存**と実証。プレーンテキスト walk は CloseKind/Break/div/br を一切見ない。
    コーパス比較で差分の大半（413/415）はブロックコマンド行由来の余計な空行が消える
    改善、1は accent 合成改善、1は外字 geta 軽微差。本文内容は保持。既定 strip は
    無変更（トークン経路 convert は保持）。→ Phase D の実証目的を達成。

- 2026-07-25 **★ 旧 BlockManager ストリーミングスタック撤去完了（プラン §0 の到達点）。**
  convert_legacy（差分オラクル）を削除し、それが唯一保持していた旧4ファイル
  （renderer/node_renderer/block_manager/tag_generator）と body-diff サブコマンド、
  presentation の旧専用ヘルパ（is_block_only_line/classify_output_line/classify_line/
  LineType/LineInfo/is_midashi_line）を一括削除（**計 2,384 行削除**）。UnconvertedGaiji は
  presentation へ移設。**HTML 本番経路は render_via_blocks（Aozora AST）のみ**。回帰検出は
  本物のオラクル（--oracle-dir oracle）が担う。オラクル 17363・回帰0・全テスト通過。
  - 残: プレーンテキストの既定は依然トークン経路（strip::convert）。AST版（convert_via_ast）
    は検証済みだが既定切替は出力変化（余計な空行除去＝改善＋accent 合成改善＋外字 geta
    軽微差）の受容判断が要る。切替えれば HTML・プレーンテキスト双方がAozora AST-only になる。

- 2026-07-25 **外字注記の参照バグ修正（compat→correctness）＋プレーンテキストもAozora AST-only化。**
  - **外字注記バグ**: `「対象」に「※［＃句点コード］…」の注記` を参照実装は基底を落とし
    外字画像だけ出す。これを正しく base=対象・ruby=（外字＋後続）の Ruby に修正
    （RefSpec::EmbeddedGaiji に annotation_ruby を追加。置換形 `「5」はローマ数字…` は
    従来どおり裸の外字）。**新ノード型は導入せず既存 Ruby+Gaiji のみ**。ユーザ判断で
    参照バグは再現せず正しい実装のみ（quirk なし）。影響は注記形2作品（58400/58401）、
    **オラクル 17363→17361（意図的な bug 修正の乖離・新ベースライン）**。
  - **文字コード**: jis2ucs.json（11,233エントリ・第1〜4水準）は既存。plain 描画が
    解決済み Gaiji の空 description を使い geta 化していたのを jis_code 優先に修正
    （2-94-2 → 鳲 等）。
  - **strip をAozora AST-only化**: strip::convert/convert_line の既定をAozora AST経由に統一し、
    旧トークン経路（extract/Tokenizer）と --via-ast を撤去。**HTML・プレーンテキスト双方が
    本番でAozora ASTのみから生成される**（第2バックエンドが実証から本番へ）。

- 2026-07-25 **仕様整理: レガシー撤去＋位置情報付与（出力 byte 不変・オラクル 17361）。**
  - **RawAST の正規化**: 旧 `RawAst(Vec<Node>)`/`Ast(Vec<Node>)`/`parse_raw`/`lower`（薄い
    未使用ラッパ）を撤去し、`parse_raw_nodes`＋`parse` に一本化。**RawAST の器は
    RawDoc/RawLine に確定**。neutral AST の器は `ast::Block`/`Inline`。この2層で
    「RawAST（マーカー含む Vec<Node>）／Aozora AST（マーカー無し・型安全な木）」が明確に。
  - **位置情報**: `RawLine.line_no`（本文0起点）を追加し、`lower_to_blocks` が各 Block
    （Line/Nested/LineWrap）へ由来行 `line` を伝播（Nested は開いた行）。バックエンドは
    無視するので出力不変。将来の char 単位 span はこの上に載せられる。
  - **残る任意項目**: `Node` enum は依然 raw マーカー（BlockStart/BlockEnd/LineJisage/
    UnresolvedReference）と解決済みノードを兼ねる中間表現。マーカーは lower で消費され
    Aozora AST に漏れない（型で保証）ため、Node の完全分割は低優先・任意。`CloseKind`/
    `Break` を中立コアに置くかは設計論点として保留（実用上は機能）。
  → **RawAST・Aozora AST の仕様は実質固まった**（2層・型の壁・位置情報あり・レガシー無し）。

- 2026-07-25 **位置情報を char 単位 span まで拡張（出力 byte 不変）。**
  `token::Span`（行内 char オフセット [start,end)）を追加。tokenizer を next_token 抽出＋
  span 付き `tokenize`、parser に `parse_raw_nodes_spanned` を追加し、生ノードに由来
  トークン span を格納。**位置情報は源泉忠実な RawAST に置く**設計（char span＝RawAST 側／
  Aozora AST は派生の行番号＝Block.line）。`line.chars()[span.start..span.end]` で原文断片を
  取得可。Aozora AST Inline への per-inline span は変換パーサ（後方参照・ルビ抽出・範囲畳み
  込み）を跨ぐ Node への span 付与が要るため大規模——現状は RawAST span＋中立 line 番号で
  source-map 可能。
  - 2026-07-26 追記: `tokenize_spanned` と `RawLine` の並行配列 `nodes`/`spans` を廃止し、
    `Spanned<T>{node,span}` に統一（`tokenize -> Vec<Spanned<Token>>`／
    `RawLine.nodes: Vec<Spanned<Node>>`）。値と位置がずれない形に。出力 byte 不変・オラクル 17361。

- 2026-07-25 **main 整合＋aozora_farm（Tauri GUI）取り込み（オラクル 17361・回帰0）。**
  main は html/strip を aozora-core へ移す再構成＋parser 分割（＝機能追加なしの整理）を
  していた一方、私の branch は同 base からAozora AST化した機能的スーパーセット。aozora_farm
  が `aozora_core::html::convert` を使うため、**私の html/strip を aozora-core へ移設**
  （main の構造に整合）し、aozora2 は後方互換で再エクスポート。`crates/aozora_farm` を
  取り込み workspace に登録、GUI は GTK 依存のため default-members から除外。API 一致
  （convert/RenderOptions）を確認、コード移動のみで出力不変。diverged parser の生マージ
  （両者が大改稿し Frankenstein 化）は避け、私のコードを main レイアウトへ載せる形で end-state
  を達成。main 固有の compatibility テスト/parser 分割は未取り込み（必要なら選択的に可能）。

- 2026-07-28 **span を intrinsic フィールドへ（`Spanned<T>` は撤去）。** 上の 2026-07-26 追記で
  導入した外付けの器 `Spanned<T>{node,span}` は、入れ子（ルビ内容・アクセント内容）まで位置を
  通す目的には迂遠だったため、`Token`/`Node`/`Inline` をそれぞれ `kind`＋`span` の構造体にする
  形へ置き換えた（`PartialEq` は `kind` のみ比較）。入れ子は再トークナイズ時に base offset を
  渡すので子も行内絶対 span を持つ。これにより、上の 2026-07-25 の記録で「大規模」として
  見送っていた **Aozora AST `Inline` への per-inline span が実現**している。出力 byte 不変・
  オラクル 17361。span が「実在」でない箇所（合成ノードの継承・ルビ親文字吸収の合併）は
  spec-ast.md「共通: 位置情報」に記載。
