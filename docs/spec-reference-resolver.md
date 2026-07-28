# 仕様: 前方参照解決（`resolve_references` / `resolve_inline_ruby`）

> **この文書は権威を持たない書き捨ての参考資料である。** 現状の実装を読み解くための
> 案内であり、規範ではない。実装（コードとテスト）と食い違ったらこの文書が古い。
> 更新の義務は負わない——古くなったら捨て、必要ならコードから書き直す。
> 一次資料はコーパスとソースコード（docs/architecture.md §1）。

作成 2026-07-28。`Vec<Node>`（型付きマーカー列）に対して、ルビ親文字の吸収と
「〇〇」に傍点 のような**前方参照**を解決するアルゴリズム記述。
コード実体は `crates/aozora-core/src/parser/reference_resolver.rs` /
`ruby_parser.rs` / `char_type.rs`。関連: [spec-tokenizer.md](spec-tokenizer.md)、
[spec-ast.md](spec-ast.md)、[design-parser-layers.md](design-parser-layers.md)。

---

## 概要

- **役割**: `parse_raw_nodes` が作った平坦なノード列を受け取り、ルビの親文字を吸収し、
  注記の参照対象を確定して装飾ノードへ畳む。
- **入力を拒否しない**。解決できない参照は捨てずに `Note(raw)`（原文字列）へ落とす。
- **位置単位**: char（Unicode スカラ）。分割は `split_at`、合併は `union`。

### 前方参照の向き

- 注記 `［＃「〇〇」に傍点］` は**自分より前にある本文**を指す（＝前方参照）。
- 探索は**ノード列の末尾から前へ遡る**。参照より後ろのノードは対象にしない。

## 入力と出力

- **入力**: 1 行ぶんの `Vec<Node>`。
- **出力**: 同じ `Vec<Node>` をその場で書き換える（長さは変わりうる）。
- **処理単位**: **行ごとに独立**（行をまたがない）。各関数は引数のノード列だけを見て、
  呼び出しをまたぐ状態を持たない。行をまたぐ構造の確定は後段 `lower_to_blocks` の仕事。
- **不変条件**: 出力に `UnresolvedReference` は残らない（解決されるか `Note` になる）。
  子ノードまで再帰的に成立する。

## 入口（公開 API）

- `resolve_references(&mut Vec<Node>)` … 工程1〜3を順に実行。
- `resolve_inline_ruby(&mut Vec<Node>)` … 工程4。
- `resolve_references_collecting_failures(&mut Vec<Node>) -> Vec<String>` …
  工程1〜3と**ノード変換結果は完全に同一**で、`Note` に落とした参照の `raw` を追加返却する。
  収集対象は**トップレベルの失敗のみ**（子ノード内の失敗は集めない）。

**呼び出し規約**: `resolve_references` の直後に `resolve_inline_ruby` を呼ぶ。

---

## 全体の順序（4 工程）

1. **`resolve_ruby_bases`** … 親文字が空のルビに、直前本文から親文字を吸収。
2. **`resolve_annotation_ranges`** … `［＃注記付き］…［＃「注記」の注記付き終わり］` を 1 ノードへ畳む。
3. **`resolve_style_references`** … `UnresolvedReference` を末尾から遡って解決。最後に子へ再帰。
4. **`resolve_inline_ruby`** … 工程1と同じ処理をもう一度実行する。

---

## 工程1: ルビ親文字の吸収（`resolve_ruby_bases`）

### トリガ

`NodeKind::Ruby { children, ruby, .. }` で **`children` が空・`ruby` が非空・`i > 0`**。

### 手続き

- `nodes[..i]` に `extract_ruby_base_from_nodes` を適用する。
- 返った `(remaining, base)` で `nodes[..i]` を `remaining` に置き換え、`base` をルビの `children` に入れる。
- ルビノードの span に `base` 各ノードの span を `union` する。
- 抽出に失敗したら何もしない（親文字は空のまま）。

---

## 親文字抽出（`extract_ruby_base_from_nodes`）

ノード列の末尾から、ルビ親文字になれる連続を切り出す。工程1・工程4 共通。

### 末尾ノードの種別による分岐（この順に判定）

1. **`Note(_)`** → **その 1 ノードだけ**を親文字にする。
2. **`Style` / `Tcy` / `FontSize` / `Keigakomi` / `Yokogumi` / `Caption`** →
   **そのタグ 1 ノードだけ**を親文字にする（`Midashi` は含まない）。
3. **`last_char_type()` が `None` を返す種別** → **即座に失敗**（`None`）。
   値を返すのは `Text` / `Gaiji`（=Kanji） / `Accent`（=Hankaku） / `DakutenKatakana`（=Katakana）
   のみで、`UnresolvedReference`・`Img`・`BlockStart`/`BlockEnd`・`Ruby` などはすべて `None`。
4. それ以外 → 下の走査へ。

### 走査

- 末尾ノードの最後の文字の文字種を**基準種別**とする。
- 末尾から前へノードを見て、基準種と同種である限り親文字に取り込む。異種に当たったら停止。
  - `Text` … `extract_ruby_base` で分割し、前半を remaining・後半を base とする。
    同種なら base を取り込み、remaining が残ればそこで停止（span も分割）。
    異種なら丸ごと remaining に回して停止。
  - `Gaiji` … 基準種が `Kanji` なら取り込む。それ以外は停止。
  - `Accent` … 基準種が `Hankaku` なら取り込む。それ以外は停止。
  - `DakutenKatakana` … 基準種が `Katakana` なら取り込む。それ以外は停止。
  - その他 … 停止。
- 取り込んだ base が空なら失敗（`None`）。

---

## テキストの文字種分割（`extract_ruby_base`）

1 つの文字列を、末尾の同種連続 = base と、その前 = remaining に分ける。

- 空文字列なら `None`。
- 末尾 1 文字の文字種を `last_char_type` とする。
- `last_char_type` が **`HankakuTerminate`**（`. ; " ? ! )`）… 直前が `Hankaku` なら
  **直前の Hankaku 連も巻き込む**。
  - `本文Fig.` → base=`Fig.` / remaining=`本文`
  - `あ.` → base=`.` / remaining=`あ`
  - `Fig..` → base=`.`（終端記号が 2 つでも最後の 1 つだけ）
- `last_char_type` が **`Else`** … **末尾 1 文字だけ**を base にする。
  - `テスト。` → base=`。` / remaining=`テスト`
- それ以外 … 末尾から同種が続く限り base に含める。
  - `私の東京` → base=`東京` / remaining=`私の`

### 文字種（`CharType`）

| 種別 | 範囲 |
|---|---|
| `Hiragana` | ぁ-ん（U+3041-3093）, ゝ, ゞ |
| `Katakana` | ァ-ン（U+30A1-30F3）, ー, ヽ, ヾ, ヴ |
| `Zenkaku` | ０-９ Ａ-Ｚ ａ-ｚ, ギリシャ Α-Ω/α-ω, キリル А-Я/а-я, `− ＆ '(U+2019) ， ．` |
| `Hankaku` | A-Z a-z 0-9, `# - & ' ,` |
| `Kanji` | **SJIS が 0x889F-0xEAA4（亜-熙）** に入る CJK, および `々 ※ 仝 〆 〇 ヶ` |
| `HankakuTerminate` | `. ; " ? ! )` |
| `Else` | 上記以外（句読点・カギカッコ・全角括弧など）。親文字になれない |

- U+4E00-9FFF でも **SJIS が 亜-熙 の範囲外**（NEC/IBM 拡張漢字・エンコード不能）は `Else` になり、
  親文字の連なりがそこで切れる。例: `厓`（U+5393, SJIS 0xFA8D）。

---

## 工程2: 注記付き範囲の畳み込み（`resolve_annotation_ranges`）

### トリガ

`BlockStart { block_type }` が `AnnotationRange` または `LeftAnnotationRange`。

### 手続き

- `i+1..` を前から走査し、**同種の `BlockEnd` を最初に見つかったもの**とする。`params.annotation` を取る。
- **畳まない条件**（いずれも何もせず次のノードへ）:
  - 対応する `BlockEnd` が無い。
  - `BlockEnd` はあるが `params.annotation` が `None`。
- `nodes[i+1..end_idx]` を `children` とする。
- 注記文字列を `parse_annotation_text` で再トークナイズする。**`Text` と `Gaiji` のみ**ノード化し、
  他のトークンは捨てる。span は終了マーカーの span を継承。
- **`AnnotationRange`**: `Ruby { children, ruby: 注記, direction: Right, keep_gaiji_notes_in_base: true }`
  1 ノードに置換。span は 開始 ∪ 終了。
- **`LeftAnnotationRange`**: `Note("左に注記付き")` + children +
  `AnnotationEnd { prefix: "左に「", content: 注記, suffix: "」の注記付き終わり" }` に展開。

---

## 工程3: 前方参照の解決（`resolve_style_references`）

### トリガ

`UnresolvedReference { target, spec, raw }`。

### 手続き

- `i > 0` なら `search_front_reference(&nodes[..i], i-1, target)` で照合する。
  - 成功 → `apply_front_reference` で畳み、参照ノードを除去。`i` は畳んだ直後へ進めて再検査。
  - 失敗 → `Note(raw)` に置き換える。
- 全ノードを処理後、各コンテナの子ノード列へ**再帰**する（`resolve_style_references_in_children`）。
  - 対象: `Ruby`（children と ruby の両方）, `Style`, `FontSize`, `Tcy`, `Keigakomi`,
    `Yokogumi`, `Caption`, `Midashi`, `Warichu`（upper と lower）。
  - 再帰は多段（子の子まで）。

---

## 前方照合（`search_front_reference`）

`nodes[..end_idx]` の末尾から連続要素を消費して、`target` を**接尾辞**として照合する。
再帰で 1 つ前へ遡る。

- `target` が空なら `None`。
- **`Text(s)`**:
  1. `s` が空 → 捨てて 1 つ前へ再帰（`target` はそのまま）。
  2. `s` が `target` で終わる（`s.strip_suffix(target)`）→ `s` を分割し、前半 prefix をバッファに残し、
     `target` 相当を子にする（span は後半）。ここで確定。
  3. `target` が `s` で終わる（`target.strip_suffix(s)`）→ 残り `remaining` で 1 つ前へ再帰し、
     戻りの `children` 末尾に `s` を足す。
  4. いずれでもなければ `None`。
- **`Ruby` / `Style` / `FontSize` / `Tcy` / `Keigakomi` / `Yokogumi` / `Caption` / `Midashi`**
  （＝スパンの一要素になれるノード）:
  - `extract_plain_text` で内部テキスト `inner` を取る。空なら `None`。
  - `inner == target` → そのノード丸ごとを子に取り込んで確定。
  - `target` が `inner` で終わる → 残りで 1 つ前へ再帰し、戻りにこのノードを足す。
  - それ以外は `None`。
- **上記以外**（画像・外字・アクセント・訓点など）→ `None`（照合打ち切り）。

### `extract_plain_text`

- `Text` → そのまま。
- `Ruby` → **親文字（children）のみ**（ルビ文字は無視）。
- `Style` / `FontSize` / `Tcy` / `Keigakomi` / `Yokogumi` / `Caption` / `Midashi` → 子を連結。
- それ以外 → 空文字列。

---

## 照合結果の適用（`apply_front_reference`）

- 参照ノードの span に、対象子ノード群の span を `union` して `combined_span` とする。
- `spec.resolve(children, combined_span)` で装飾ノードを生成する。
- `prefix` が残るなら、分割前半を `Text` として先頭に足す（span は `split_at(prefix長).0`）。
- `start_idx..=(参照の直前)` を `[prefix?][装飾ノード]` に置換し、参照ノード自身を除去する。
- `i` を装飾ノードの次に進めて続きを再検査する。

---

## 工程4: ルビ親文字の再解決（`resolve_inline_ruby`）

- 中身は工程1（`resolve_ruby_bases`）と**同一**。呼ぶ位置だけが違う。
- 工程3で `UnresolvedReference` が装飾タグや `Note` に変わった結果、
  「タグ 1 ノードが親文字」の分岐に該当するようになったルビがここで解決される。
  - 例: `公事根源［＃「公事根源」は斜体］《くじこんげん》` の親文字は工程4で `Style` になる。
- 工程1も省略できない。工程3の `search_front_reference` は `extract_plain_text(Ruby)`＝親文字を
  見るため、親文字が空のままでは照合が失敗する。
