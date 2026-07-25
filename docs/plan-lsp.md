# 青空文庫エディタの LSP 的機能 設計

Aozora AST に付与した**位置情報**（`token::Span` の char オフセット＋行番号）を土台に、
aozora_farm のエディタ（CodeMirror 6）へ LSP 的な支援機能を導入するための設計。

## 1. 方針とアーキテクチャ

**単一の真実の源は `aozora-core`（Rust）**。解析ロジックはフロントに複製せず、パーサ
本体（`parse_document_raw` の位置情報付き生ノード）から派生させる。これにより本文
レンダリング（`convert`）と解析（`analysis`）が同じパーサ結果を共有し、乖離しない。

```
                     ┌─────────────────────────── aozora-core (Rust) ───────────────────────────┐
  source (buffer) ─► │ tokenize_spanned → parse_raw_nodes_spanned → RawDoc{RawLine{nodes,spans}} │
                     │        │                                              │                    │
                     │        ├─ lower_to_blocks → html/strip (convert)      │  ← 既存             │
                     │        └─ analysis::analyze ──────────────────────────┘  ← 本設計          │
                     └──────────────────────────────────┬───────────────────────────────────────┘
                                                         │ Analysis { tokens, symbols, diagnostics }
                    ┌───────────────────── 2通りの消費経路 ─────────────────────┐
                    │ (A) GUI: Tauri invoke("analyze")  … 本設計の主経路（低遅延・同一プロセス）
                    │ (B) 汎用: aozora-lsp バイナリ（tower-lsp）… VSCode 等から stdio LSP（将来）
                    └───────────────────────────────────────────────────────────┘
```

- **(A) GUI 内 invoke** … aozora_farm では Tauri コマンド `analyze(input) -> Analysis` を
  デバウンス付きで呼ぶ。IPC はローカルなので往復は速く、200ms 程度のデバウンスで十分。
- **(B) 標準 LSP サーバ** … 同じ `analysis` モジュールを `aozora-lsp` バイナリで包めば、
  VSCode/Neovim 等からも使える。GUI と共通ロジックなので追加コストは薄い（将来）。

位置は **LSP と同じ 0 起点**（行・char とも、`end` は含まない半開区間）で統一。
CodeMirror は行 1 起点なので、フロントで `line+1` して変換する。

## 2. すでに実装済み（本コミット）

| 層 | 実体 | 内容 |
|----|------|------|
| Rust | `aozora-core::analysis` | `analyze(&str) -> Analysis`。純粋関数・`convert` 非依存・オラクル無影響 |
| Rust | `Analysis` | `tokens: Vec<SemToken>` / `symbols: Vec<Symbol>` / `diagnostics: Vec<Diagnostic>` |
| Rust | `serde` フィーチャ | 結果型を JSON 直列化可能に（既定ビルドは serde 無し） |
| Tauri | `analyze` コマンド | `src-tauri` が `aozora-core/serde` を有効化して `Analysis` を返す |
| TS | `commands/tauri.ts` | `analyze()` ブリッジ＋型（`SemToken`/`OutlineSymbol`/`AozoraDiagnostic`） |

現状の中身:
- **セマンティックトークン**: 各生ノード（Ruby/Midashi/Style/Gaiji/Accent/Img/ブロック
  見出し/その他注記）を種別＋char 範囲で列挙。正規表現ハイライトより正確で、ホバーの土台。
- **アウトライン**: 見出し（インライン `Node::Midashi` とブロック `BlockStart..BlockEnd`
  の両形式）を level（1/2/3）＋テキスト＋位置で列挙。
- **診断**: `Node::UnresolvedReference`（＝パーサが対象を前方に見つけられなかった注記）を
  warning 化。「解決できなかった」事実が根拠なので**誤検知しない**安全な第一診断。

## 3. 機能 → データ 対応と実現度

| LSP 機能 | 土台データ | 実現度 | フェーズ |
|----------|-----------|--------|----------|
| セマンティックハイライト | `analysis.tokens`（span 付き） | **実装・CM接続済**（`editor/lsp.ts`） | 1 |
| アウトライン／シンボル | `analysis.symbols` | 実装済（データ供給・パネル未実装） | 1 |
| 診断: 未解決注記 | `Node::UnresolvedReference` | **実装・CM接続済**（linter＋lintGutter） | 1 |
| 診断: 未閉じブロック | `lower` 末尾 `stack.pop()` の残り（`open_line`） | 設計済・未実装 | 2 |
| 診断: 未知コマンド | `CommandKind::Unknown` を span 付きで | 設計済・未実装 | 2 |
| 診断: 未解決外字 | `Node::Gaiji` の JIS/U+ が表引きで解決不能 | 設計済・未実装 | 2 |
| ホバー | 位置 → token → 説明（注記の意味／外字の実文字・画像） | 設計済・未実装 | 2 |
| 補完 | `［＃` 直後に注記コマンド候補（`palette.ts` の台帳を流用） | 設計済・未実装 | 3 |
| 折りたたみ | ブロック `BlockStart`/`BlockEnd`（or Block 木）の行範囲 | 設計済・未実装 | 3 |
| go-to / peek | 対象参照（注・図版）※青空記法では用途限定 | 要検討 | 後 |

## 4. CodeMirror 6 への接続設計

解析は**バッファ変更をデバウンス**（~200ms）して `analyze()` を 1 回呼び、結果を
`StateField` に格納。各機能はそのフィールドを購読する。

```ts
// エディタ拡張（スケッチ）
const analysisField = StateField.define<Analysis | null>({ /* update: setAnalysis effect */ })

function scheduleAnalyze(view: EditorView) {           // docChanged で呼ぶ・200ms デバウンス
  const text = view.state.doc.toString()
  analyze(text).then(a => view.dispatch({ effects: setAnalysis.of(a) }))
}

// 位置変換ヘルパ: 0 起点(line,char) → CM の絶対 pos
const toPos = (doc: Text, line: number, ch: number) => doc.line(line + 1).from + ch
```

- **診断（linter）**: `@codemirror/lint` の `linter()` を `analysisField` から供給。
  `diagnostics.map(d => ({ from: toPos(...d.range.start), to: toPos(...d.range.end),
  severity: d.severity, message: d.message }))`。※`@codemirror/lint` を依存追加。
- **ハイライト（decorations）**: `tokens` を `Decoration.mark({class: 'aoz-'+kind})` にして
  `ViewPlugin` で描画。現行 `aozora-lang.ts` の StreamLanguage は段階的に置換（まず併用可）。
- **ホバー**: `@codemirror/view` の `hoverTooltip`。位置を含む token を引き、種別に応じた
  説明を出す（外字は実文字＋画像、注記は意味）。token だけで足りない情報は追加の
  軽量コマンド（例 `describe_gaiji(jis)`）で補う。
- **アウトライン**: `symbols` をサイドパネルにツリー描画。クリックで `toPos` へスクロール。

必要な npm 追加: `@codemirror/lint`（linter/hover 用）。他は既存 `@codemirror/*` で足りる。

## 5. フェーズ計画

- **フェーズ1（土台＋CM接続・完了）**: `analysis` モジュール＋Tauri コマンド＋TS ブリッジ。
  CM 側 `editor/lsp.ts` で **診断（linter＋lintGutter）とセマンティックハイライト（Decoration.mark）**
  を接続済み（`@codemirror/lint` 依存追加・解析は 200ms デバウンス・位置は `toPos` で 0起点→CM 変換）。
  残: **ホバー**（token に説明が要る→フェーズ2）・**アウトラインパネル**（`getOutline` でデータ供給済み、UI 未実装）。
- **フェーズ2（診断の拡充）**:
  - **未閉じブロック**: `lower` に `lower_to_blocks_with_diagnostics(&RawDoc) -> (Vec<Block>,
    Vec<Diagnostic>)` を追加。既存 `lower_to_blocks` はこれを呼んで診断を捨てる薄い包みにし、
    **Block 出力は 1 バイトも変えない**（オラクル不変を必ず確認）。末尾 `stack.pop()` に残る
    ブロックの `open_line` を warning にする。
  - **未知コマンド／未解決外字**: `tokenize_spanned` の `Token::Command`/`Token::Gaiji` を span
    付きで分類（`parse_command` の `Unknown`、外字は JIS/U+ の表引き失敗）。誤検知を避けるため
    「対象指定つき注記（〜に傍点 等）」は除外する規則を明記。
  - **ホバー**: token 種別ごとの説明文＋外字補助コマンド。
- **フェーズ3（編集支援）**: `［＃` 補完（`palette.ts` 台帳流用）、ブロック折りたたみ、
  StreamLanguage を decorations へ完全移行。
- **将来**: `aozora-lsp` バイナリ（tower-lsp）で同ロジックを標準 LSP として外部エディタへ。

## 6. 設計上の決定・注意

- **オラクル不変**: `analysis` は `convert` と独立の追加レイヤ。フェーズ2 で `lower` を触る際も
  「診断を**追加返却**するだけで Block 出力は不変」を厳守し、都度オラクル 17361 を確認する
  （`div-balance-quirk` の高リスク方針に沿い、挙動は変えない）。
- **位置基準**: 解析結果は 0 起点で統一（LSP 準拠）。CM 変換はフロントの `toPos` に集約。
- **性能**: 全バッファ再解析をデバウンス。長文でも 1 パス O(nodes) で軽い。将来必要なら
  行単位の差分解析に拡張可能（`RawLine` は行独立なので差分化しやすい）。
- **未解決注記診断の意味**: 参照実装は未解決注記をそのまま本文に出す（エラーにしない）ので
  severity は warning。エディタ支援としての「気づき」であって変換を止めるものではない。
