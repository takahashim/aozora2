# 実装計画: 制約解消型 Lowerer への移行

> **この文書は作業計画であり、権威を持たない。** 設計は
> [spec-lowerer-constraints.md](spec-lowerer-constraints.md)、現行挙動は
> [spec-lowerer.md](spec-lowerer.md) と実装が正。移行が完了したらこの文書は
> 役目を終える（削除してよい）。

作成 2026-08-03。[spec-lowerer-constraints.md](spec-lowerer-constraints.md) の
「段階的移行」を実施可能な作業単位に落としたもの。**実装はオラクル
（`../aozorabunko` コーパスと `../aozora-htmlcheck`）のあるマシンで進める**。
この計画を書いたマシンにはどちらも無く、全書庫検証が回らないため。

受け入れ基準は設計文書の**現行再現プロファイル**: 移行の全段階（C0〜C7）で
AST・診断・HTML を 1 バイトも変えない。既知バグ修正（PR-B）だけが移行完了後の
独立変更。

---

## 方式: 凍結コピー＋その場変形

「別実装を並走させて最後に差し替える」のではなく、**現行ループの逐語コピーを
`lower/legacy.rs` に凍結して検証基準とし、既定経路をその場で段階変形する**。

- 並走検証の基準は変形対象とコードを共有してはならない（共有すると両側が同時に
  変わったとき並走が盲目になる）。凍結コピーがこれを構造的に保証する。
- legacy と共有してよいのは、移行中に触らない凍結境界の葉だけ:
  `inline::to_inlines`・`break_policy::content_break`・`block_kind_of`。
- 移行中に葉を触る必要が出たら、**先に legacy へ該当関数をコピーしてから**触る。

### 凍結境界（変えてはいけない。呼び出し元は調査済み）

- `lower_to_blocks` / `lower_to_blocks_with_diagnostics` のシグネチャと出力値。
- `LowerDiagnostic` の形と診断列の順序（`analysis.rs` の `analyze` が消費）。
- `ast.rs` の全型・serde 属性・手書き `PartialEq`。
- `lower::inline::to_inlines`。`html/block_renderer.rs` の `render_line_inline`
  （`html::convert_line` の実体）と `strip.rs` の `convert_line` が**行ループを
  通らずに直接使う**。行ループを差し替えてもこの 2 経路は同じ関数を共有し続ける。
- pub モジュール `lower::break_policy` / `lower::inline`（crates.io 公開済み。
  削除は semver-major）。
- `data/conformance/*.json` のスナップショット。**移行中は UPDATE_CONFORMANCE を
  叩かない**（旧実装の出力の凍結として使う）。

自由に再構成できるのは `lower/mod.rs` の private 一式（行ループ・`classify_line`・
`BlockStack`・`ImplicitClose`・`apply_closes`・`carry`/`swallowed`・
`find_unmatched_block_ends`・`burasage_open`・`take_line_scope_wrap` 等）。

### 旧経路の公開方法

```rust
#[doc(hidden)]
pub fn lower_to_blocks_legacy(raw: &RawDoc) -> (AozoraAst, Vec<LowerDiagnostic>)
```

を 1 本だけ生やす。pub なので dead_code 警告が出ず、統合テストとクレート跨ぎの
example から呼べる。`#[doc(hidden)]` は semver 慣習上保証外なので追加・削除とも
minor で通る。`#[cfg(test)]` の pub(crate) はクレート跨ぎの example から見えず、
`#[path]` include はテストクレート側で `crate::ast` が解決できないので、いずれも不可。

## モジュール構成

```
crates/aozora-core/src/lower/
  mod.rs        公開 API＋薄いドライバ（③適用 → facts → solve → materialize）
  facts.rs      新規・private。制約 1（役割の割り当て）。LineRole / LineFacts /
                line_facts()。行内で閉じる純関数のみ
  plan.rs       新規・private。LowerPlan / PlanBlock / PlanLine / Closure と
                materialize()（Plan → AozoraAst の純写像）
  solve.rs      新規・private。facts 列 → LowerPlan。PlanStack・仮想終端（暗黙閉じ）・
                joins（行結合）・CloseKind/Break 導出・EOF 診断（制約 3,4,6,7,8）
  legacy.rs     移行中のみ。現行 mod.rs の行ループ一式（36〜800 行相当）の逐語コピー。
                ヘッダに「並走検証の基準。編集禁止（削除のみ）」と明記
  inline.rs     既存のまま（制約 2 の実体。動かさない）
  break_policy.rs 既存のまま
```

設計上の要点:

- `LineRole` の優先順は制約 1 の規則 1〜7 と**同型**にする。規則 1（ぶら下げ開始行）が
  最上位で、旧 `burasage_open` のループ先頭割り込みを吸収する。規則 5 の
  `head_is_text` は互換バグとして**温存**し、コメントで PR-B を参照。
- 行番号は materialize で再計算せず **solve が焼き込む**。carry 併合行＝結合先の
  行番号、非内容行でのフラッシュ＝フラッシュを起こした行の番号、Nested＝開いた行、
  EOF 閉じ＝開いた行。
- `to_inlines` を呼ぶ**スライス境界を旧実装から変えない**（ぶら下げ開始行は
  `nodes[..marker]` と `nodes[marker+1..]` を別々に呼ぶ。連結してから 1 回で呼ぶと
  同一行範囲の対応づけが変わりうる）。
- 診断は EOF 到達時に PlanStack を内側から畳んだ順（現行と同順）。
- `block_close_kind` は `outer_after_pop: Option<&BlockKind>` を取る形に一般化して
  `BlockStack` 依存を切る。
- 設計文書の `Position { line, node, offset }` はフル実装しない。現行再現に必要なのは
  line と行内 node 添字だけ（offset span は説明用にしか使わない）。

## コミット単位（各段階で全テスト green・出力不変）

前提コミット（docs）は済んでいる想定（spec-lowerer-constraints.md・spec-lowerer.md・
本書）。作業ブランチは `spec-commands` の続きでも新ブランチでもよい。

- **C0** `test(core): 旧 Lowerer の凍結コピーと並走検証を追加した`
  legacy.rs＋`lower_to_blocks_legacy`＋`tests/lower_parity.rs`＋
  `crates/aozora2/examples/lower_parity.rs`。この時点では既定経路とコピーの比較
  なので自明に一致（配線チェック）。以降の全変形を最初から監視下に置く。
- **C1** `refactor(core): 行の役割判定を lower/facts.rs へ切り出した`
  `classify_line`/`burasage_open`/`find_unmatched_block_ends`/`ends_with_hard_break`
  を facts.rs の `LineRole` へ再編。旧ループは `match facts.role` に書き換え。
- **C2** `refactor(core): 行ループが LowerPlan を組んで materialize する形にした`
  plan.rs 導入。`BlockStack` → `PlanStack`。ループ末尾で materialize。
  **→ ここで全書庫並走の 1 回目を回す**（基盤 2 コミットを早期に固める）。
- **C3** `refactor(core): 暗黙閉じを仮想終端の制約として解くようにした`
  `ImplicitClose::apply` を「開始の直前に CloseKind 確定済みの仮想終端を挿入し、
  通常の閉じ機構で処理」へ。表（字下げ→1個NoBreak／地付き→連続NoBreak／
  ぶら下げ→連続Newline）はデータとして solve.rs へ。適用条件は規則 1・2・6例外のみ。
- **C4** `refactor(core): 行結合を carry の可変状態から joins 関係へ移した`
  `carry`/`swallowed` → 断片チェーンと `suppressed_lines`。累積規則（ぶら下げ
  開始行・HardBreak 行が連続しても確定しない）と行番号 quirk を厳守。
- **C5** `refactor(core): 閉じ対応・CloseKind・Break の導出を解決器へ集めた`
  `apply_closes` を solve.rs へ。CloseKind を制約 7 の優先順の後段導出に一本化。
  IgnoredEnd の実測 2 例をテストに固定。`check_plan_invariants`（区間が入れ子か
  互いに素・joins 非巡回・診断が内側順）を `#[cfg(test)]` で並走入力全件に適用。
- **ゲート G**: 全書庫並走 一致 100%＋aozora-htmlcheck 悪化 0。
  **確認が取れるまで C6 を積まない**。不一致が出たら examples の出力で C1〜C5 を bisect。
- **C6** `refactor(core): 凍結していた旧行ループと並走検証を削除した`
  legacy.rs・legacy 関数・example を削除。`tests/lower_parity.rs` は Plan 不変条件＋
  固定入力の検査（`lower_plan_invariants.rs`）へ縮小改名。
- **C7** `docs: spec-lowerer.md を制約仕様への対応付けへ縮めた`

### 後続 PR（本移行と切り離す）

- **PR-A（設計文書の段階 5）** `refactor(core): コマンド振り分けを候補と優先度表へ移した`
  `parse_command` の 24 分岐を
  `static DISPATCH: &[(&str, fn(&str) -> Option<CommandResult>)]` へ。落とし穴 2 つを
  テストに固定: (i) `ここから…`/`ここで…` は Note を返しても**選択済み**（後続へ
  落とさない）(ii) 割り注は `without_nested_commands` 後の部分一致。
  検証に `ruby tools/verify_commands.rb`。lower とは依存が無いので並行可。
- **PR-B（設計文書の段階 6）** `fix(core): 行途中オープンの head_is_text ガードを外した`
  AST が変わる唯一の変更。実測済み 5 形（前がテキスト/ルビ/外字/傍点/行内地付き）を
  参照実測値の unit テストに固定。影響する conformance フィクスチャがあれば
  **ここで初めて** `UPDATE_CONFORMANCE=1` を叩き差分を目視コミット。オラクルで
  悪化 0（改善は baseline 更新を同コミットに含める）。spec-lowerer.md /
  spec-lowerer-constraints.md の該当節を更新。

## 並走テストの設計

### `crates/aozora-core/tests/lower_parity.rs`（feature ゲートなし）

feature ゲートを付けない（CI の test ジョブは素の `cargo test` しか回さないため。
serde の下に置くと CI で走らない）。

入力 4 系統:

1. `data/conformance/*.json` 35 件の `source`（dev-dep の serde_json::Value で読む。
   serde feature 不要。`\n`→`\r\n` 変換は conformance.rs と同じ）。
2. `../aozora2/tests/fixtures/{chukiichiran_zenrei,chukiichiran_kinyurei,junshi}.txt`
   （CARGO_MANIFEST_DIR 相対。**存在しなければ警告して skip**——crates.io に単体
   パッケージされた状態の `cargo test` を壊さない）。`chukiichiran_zenrei.txt` は
   注記一覧の全例 1068 行で、単一入力としては最も分岐カバレッジが高い。
3. `tests/invariants.rs` の SOURCE 相当の複合文字列。
4. 手書きエッジ表: `lower/mod.rs` の position_tests の全入力＋実測済みの edge
   （`Ａ［＃字下げ終わり］Ｂ［＃ここで字下げ終わり］Ｃ`、開き無しの明示終端、
   ぶら下げ開始行 3 連続、`［＃割り注終わり］` 単独行、字下げ中の `［＃４字下げ］`
   単独行、`［＃２字下げ］あ［＃５字下げ］い`、行スコープ包み＋未閉じ〔＋空行吸収、
   EOF 多重未閉じ、carry フラッシュ形＝ぶら下げ開始行の直後にブロック開始行）。

比較:

- 診断は `assert_eq!(Vec<LowerDiagnostic>)`（derive PartialEq で完全比較できる）。
- AST は **`format!("{:#?}")` の文字列比較**。`Block`/`Inline` の手書き `PartialEq` は
  `line`/`span` を比較しないので、`==` では行番号 quirk の退行を検出できない。
  **PartialEq 比較に退化させないこと。** 失敗時は最初の相違行±3 行を表示
  （依存追加なしで書ける）。

### `crates/aozora2/examples/lower_parity.rs`（コーパス規模）

`roundtrip_text.rs` の形式を踏襲。stdin/引数に ZIP パス列 →
`read_first_txt_from_zip` → `decode_to_utf8` → 既定経路 vs `lower_to_blocks_legacy`
を `{:#?}` と診断で比較 → `一致 N / 不一致 M / 読めず E（計 T）` を stdout、
不一致は先頭 5 件のパス＋最初の相違行を stderr、不一致 > 0 で exit 1。

全書庫実行:

```
cargo build --release --example lower_parity -p aozora2
find ../aozorabunko/cards -name '*.zip' | ./target/release/examples/lower_parity
```

## 検証マトリクス

| 検証 | 実行時期 |
|---|---|
| `cargo test`（並走テスト含む） | 全コミット |
| `cargo test -p aozora-core --features serde`（conformance は素の test では素通り） | 全コミット |
| `cargo fmt --all --check` / `cargo clippy --workspace --exclude aozora_farm --all-targets` | 全コミット |
| `ruby tools/verify_ast_spec.rb` | C2・C6・PR-B（安価なので毎回でも可） |
| `ruby tools/verify_commands.rb`（要 release build） | C5・C6・PR-A・PR-B |
| 全書庫並走（examples/lower_parity） | C2 後に 1 回目、ゲート G で最終 |
| aozora-htmlcheck オラクル悪化 0 | ゲート G、PR-B |

## リスクと規律

- **葉の凍結規律**: 移行中に inline.rs / break_policy.rs / `block_kind_of` を触らない。
  必要が出たら先に legacy へ該当関数をコピーしてから。
- リポジトリ内入力は小規模で、C1〜C5 の差は**コーパスでしか出ない種類がありうる**。
  C2 後の早期全書庫実行で挟み撃ちにする。
- 依存追加なし（proptest 等は入れない。dev-dep は serde_json のみという既存文化に合わせる）。
- 設計文書の不変条件 4（走査法によらず同じ LowerPlan）は第二の解決器では検証しない。
  「solve が facts のみを入力とする単一決定的パス」であることを構造で保証し、
  `check_plan_invariants` で区間・結合・診断順の性質を検査する。
