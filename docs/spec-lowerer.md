# Lowerer 実装ノート（行→ブロック畳み込み）

> **この文書は権威を持たない書き捨ての参考資料である。** 規範は
> [spec-lowerer-constraints.md](spec-lowerer-constraints.md)（制約仕様）で、ここは
> それをコードのどこがどう解いているかの地図である。実装と食い違ったらこの文書が古い。
> 一次資料はコーパスとソースコード（docs/architecture.md §1）。

作成 2026-08-02。2026-08-03 改稿——手続き（巨大な行ループ）の記述だったものを、
制約仕様への対応付けと、制約仕様に書いていない実装の細部だけに縮めた。

RawAST（行ごとの平坦なマーカー列）を Aozora AST（block ⊃ line ⊃ inline の木）へ
畳む。関連: [spec-commands.md](spec-commands.md)（②振り分け）、
[spec-reference-resolver.md](spec-reference-resolver.md)（③。各行の前処理）、
[spec-ast.md](spec-ast.md)（span の意味論・行マージの位置情報）、
[spec-aozora-ast-json.md](spec-aozora-ast-json.md)（`Block`/`Inline` の形）。

---

## 役割

- 行内マーカー（`BlockStart`/`BlockEnd`/`LineJisage`）の行またぎ対応づけを行い、
  入れ子の `Block` 木にする。
- 行末改行（`Break`）・開始/閉じタグの出力形（`OpenKind`/`CloseKind`）という
  互換メタデータをここで確定させ、以降の描画を状態なしの木歩きにする。
- 参照実装 aozora2html の `@indent_stack`（開いているブロック）・
  `close_conflicting_blocks`（暗黙閉じ）・`@terprip`（行末 `<br />` の抑制）・
  `@noprint`（出力を飛ばして次行へ持ち越す）の逐次モデルを、一度の畳み込みで再現する。
- **入力を拒否しない**。閉じ忘れ・過剰な閉じ・種類不一致もすべて何らかの木にする。

入力は `RawDoc`（行の列）、出力はトップレベルの `Vec<Block>` と
`Vec<LowerDiagnostic>`（閉じ忘れ・閉じる相手のない終端・行中のブロック開始。
出力には影響しない）。
各行は畳む前に③を通す（`resolve_references` → `resolve_inline_ruby`。行ごとに独立）。

## モジュールと段階

```text
RawDoc ──③──> 解決済みノード列
           facts::line_facts ──> LineFacts（役割）      制約 1
           solve::solve      ──> LowerPlan              制約 3・4・6・7・8
           plan::materialize ──> Aozora AST
```

| ファイル | 役割 | 制約 |
|---|---|---|
| `lower/facts.rs` | 行の事実。`LineRole` と `line_facts`。行内で閉じる純関数だけ | 1 |
| `lower/inline.rs` | 行内の畳み込み `to_inlines`（同一行範囲の対応） | 2 |
| `lower/solve.rs` | `LowerPlan` を組む解決器。`PlanStack`・`VirtualEnd`・`Joins`・`close_kind` | 3・4・6・7・8 |
| `lower/break_policy.rs` | 行末改行 `content_break` | 7 の `Break` |
| `lower/plan.rs` | `LowerPlan` の型・`materialize`・`check_plan_invariants` | — |
| `lower/mod.rs` | 公開 API と `block_kind_of` | — |

`solve` の中の `PlanStack` は制約 3（対応と包含）を線形時間で解く実装であって仕様では
ない。「最も内側の生存ブロック」がスタックの最上位に当たる。

## 制約と実装の対応

| 制約 | 実装 |
|---|---|
| 1. 役割の割り当て（規則 1〜7） | `facts::role_of`。判定順が規則の順。規則 1＝`BurasageOpen`、2＝`BlockOpen`、3＝`BlockClose`、4＝`Closes`、5＝`BlockOpenWithTail`、6＝`LineWrap`（例外は `BlockOpen`）、7＝`Content` |
| 2. 同一行の範囲 | `inline::to_inlines` と `find_matching_end`（同種で対応） |
| 3. 複数行ブロックの対応と包含 | `PlanStack`（積む・開く・閉じる）、`end_matches`（最内層と書かれた種類の照合）、`apply_closes`／`split_closes`（1 行に複数の終端） |
| 4. 暗黙閉じ | `VirtualEnd::before_opening` の表と `open_block_after_virtual_ends`。通すのは規則 1・2・6 の例外だけ |
| 5. 行スコープ包み | `facts` の規則 6 と、`strip_line_scope_marker` / `take_line_scope_wrap` |
| 6. 行結合 | `Joins`（`defer`／`attach`／`settle`）と `suppressed_lines` |
| 7. `CloseKind` と `Break` | `close_kind`（優先順 1〜5 をそのまま match に写した）と `break_policy::content_break` |
| 8. EOF と診断 | `solve` 末尾の `pop_open` ループ。`Closure::Eof` で閉じ、`LowerDiagnostic` を内側から順に積む。種類は `unclosed-block`／`unmatched-end`（Error）と `midline-block-open`（Warning） |

`LowerPlan` の不変条件（包含・結合の非巡回・診断の順・吸収した行）は
`plan::check_plan_invariants` が見る。`solve` の末尾から `cfg!(debug_assertions)` の
ときだけ呼ぶので、デバッグビルドで走る検証はすべてこの検査を通る。

## 制約仕様に書いていない実装の細部

### `block_kind_of`: ブロックになれる種類

`BlockType` ＋パラメータ → `BlockKind` の写像。写せる種類だけが複数行ブロックになる。

| 写せる（ブロックになる） | 写せない（インライン専用） |
|---|---|
| 字下げ・地付き・字詰め・罫囲み・横組み・キャプション・大/小文字・太字・斜体・ぶら下げ・見出し | 縦中横・割り注・割書・装飾（Style）・注記付き範囲（左も） |

写せない種類の開始・終了が行に残った場合はブロックを開閉せず、
インライン畳み込みが処理するか、黙って落ちる。

### 行末改行の決定（`content_break` → `Break`）

行のインライン列（結合したあとの全体）から決める。参照実装 `@terprip` の再現。

`Break::None`（`<br />` を出さない）になる条件（いずれか）:

1. 行に explicit な閉じ（`ここで…終わり`）が含まれる。
2. 行末のインラインが「閉じタグで終わる」種類:
   通常見出し（同行・窓は除く）／行末まで包む地付き（`ChitsukiInline`）／
   div を出す種類の `BlockInline`（字下げ・地付き・字詰め・罫囲み・横組み・
   キャプション・文字サイズ・太字・斜体。見出しは Normal のみ）。
3. 通常見出しが行の**どこかに**現れる（入れ子の中・ルビ親文字の中も見る。
   参照は見出しコマンドに出会った時点で旗を立てるため位置を問わない。実測）。

それ以外は `Break::Br`（行末に `<br />`）。`Break::NoNewline` は行途中で
ブロックが閉じるときの前半本文専用（改行は閉じタグ側が出す）。

### 行内のインライン畳み込み（`to_inlines`）

ノード列の断片をインライン列へ写す。先頭から走査し:

1. `BlockStart` から始まる範囲は、次の順で 1 つのインラインに畳めるか試す:
   1. **同一行のインライン範囲**（is_block=false）: `find_matching_end` で**同種**の
      `BlockEnd` を深さを数えて探し、見つかれば範囲を畳む。畳める種類は
      見出し・装飾・大/小文字・横組み・縦中横・罫囲み・キャプション・割書
      （この一覧は `wrap_inline_range` の網羅マッチだけが持つ。catch-all を
      置かない——variant 追加時に黙って注記化させないため）。
   2. **同一行のブロック形**（is_block=true）: 同種の終わりが同じ行にあれば
      `BlockInline`（開始タグを行内に埋め込む形）に畳む。
   3. **行途中の地付き**（is_block=false の Chitsuki）: 同種の終わりまで、
      無ければ**行末まで**を `ChitsukiInline` に包む。
2. 畳めなければ 1 ノードずつ写す。割り注の `BlockStart`/`BlockEnd` だけは対応づけ
   なしの開閉マーカー（`InlineKind::Warichu {open}`）としてそのまま写す。
   それ以外のブロック構造マーカーは、ここまでで消費されなかったものは**落ちる**。
3. 注記（`Note`）・送り仮名の中身は、生の注記文字列を本文と同じ
   tokenize→parse→ルビ解決で再パースして持つ（前方参照は走らせない）。
   注記が注記を含む再帰は深さ 4（`MAX_NOTE_DEPTH`）で打ち切り、素のテキストにする。
4. `UnresolvedReference` はここへ来ない（③が解決するか `Note` にしている。
   来たら呼び出し側が③を飛ばしたバグ）。

`to_inlines` は行ループを通らない経路（`html::convert_line`＝`block_renderer` の
`render_line_inline`、`strip::convert_line`）からも直に使われる。

### 行マージが位置情報に与える影響

出力を飛ばした行の断片は次の行の `Block::Line` に入るので、span がその行より前を
指すことがある。[spec-ast.md](spec-ast.md)「span が『実在』でない箇所」を見よ。

## 性質まとめ

- **1 パスで解き、判断はすべて `LowerPlan` に集まる**。`materialize` は状態も分岐も
  持たない写像である。
- **対応づけは最内層照合の LIFO**（終端は最内層の種類が書かれた種類と一致するときだけ
  閉じ、合わなければ何も閉じずに診断へ出す）＋**開くときの暗黙閉じ**。
  同一行で揃う対はインライン畳み込みが先に消費する（こちらも**同種**で対応づける）。
- **互換メタデータ（`Break`/`OpenKind`/`CloseKind`）はここで確定**し、
  バックエンドは状態を持たない。
- **入力を拒否しない**。過剰な終端は無視、閉じ忘れは EOF で閉じ、診断で可視化する。
