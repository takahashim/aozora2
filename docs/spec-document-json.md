# 文書交換形式（JSON）

改訂: 2026-07 v0.1

この文書はたたき台である。

[RawAST 交換形式](spec-rawast-json.md)と [Aozora AST 交換形式](spec-aozora-ast-json.md)は、
どちらも**本文だけ**を表す。ヘッダ（題名・著者）も底本情報も含まないので、その 2 つだけでは
完全な HTML 文書を組み立て直せない。本書はその不足分を添えて、**文書 1 本を往復できる**
ようにする器を定める。

---

## 1. 何を足すのか

木の外から要るのは 3 つだけである。

| 足すもの | なぜ要るか |
|---|---|
| ヘッダ情報 | `<head>` と `<div class="metadata">` の材料。題名・著者などは本文の木に入らない |
| くの字点の使用 | フッタ「表記について」の項目。**生のソース行**を走査して決まるので、原文を保持しない Aozora AST からは復元できない |
| 末尾が改行か | 文書末尾の `<br />` の数が変わる |

節（本文・本文終わり後・底本情報）は分けて持つ。元の変換系が本文と後付けを別の規則で
出す以上、器の側でも分けるのが素直である。

## 2. 文書全体

```json
{
  "format": "aozora-document",
  "version": "0.1",
  "tree": "aozora-ast",
  "header": HeaderInfo,
  "main_text": Tree,
  "after_text": Tree,
  "bibliographical": Tree,
  "kunoji": { "plain": Bool, "dakuten": Bool },
  "ends_with_newline": Bool
}
```

- `tree` は中身の木の種類。`"aozora-ast"` なら各節は [Aozora AST](spec-aozora-ast-json.md) の
  `blocks`（`Block` の列）、`"aozora-rawast"` なら [RawAST](spec-rawast-json.md) の
  `{ "lines": [...] }` になる。読む側はこの値だけで見分けられる。
- `after_text` と `bibliographical` は無ければ空（`[]` または `{"lines": []}`）。
- 節の切れ目は元のテキストの記法で決まる。`［＃本文終わり］` があれば**以降はすべて**
  `after_text` に入り（底本行も含む）、`bibliographical` は空になる。`［＃本文終わり］` が
  無く `底本：` があれば、そこから `bibliographical` に入る。
- 共通の規約（構成子の表し方・位置・文字列・行）は 2 つの交換形式と同一。

### HeaderInfo

```
HeaderInfo = {
  title: Text?, author: Text?, subtitle: Text?,
  original_title: Text?, original_subtitle: Text?,
  translator: Text?, editor: Text?, henyaku: Text?
}
```

- ヘッダの**行**ではなく、行から抽出した結果を持つ。行数によって解釈が変わる規則
  （2〜6 行だけを解釈し、1 行と 7 行以上は題名だけにする）は抽出の側にあり、この器は
  結果だけを運ぶ。

### kunoji

```
kunoji = { plain: Bool, dakuten: Bool }
```

- `plain` は `／＼`、`dakuten` は `／″＼` を使ったか。フッタ「表記について」に出す項目を
  決めるためだけに使う。
- 注記の中に書かれていても拾う必要があるため、**パース後の木ではなく生の行**を走査した
  結果である。木からは復元できないのでここに持つ。

## 3. 何が復元できて、何ができないか

この器から**完全な HTML 文書**が組み立て直せる。実装では全コーパス（17509 作品）で
「テキスト → JSON → HTML」が「テキスト → HTML」とバイト一致することを確かめている。

復元**できない**もの:

- **原文そのもの**。`tree` が `aozora-rawast` なら各行の `source` から本文は戻せるが、
  `aozora-ast` は原文を保持しない。
- **ヘッダの原文の行**。`HeaderInfo` は抽出済みの値なので、元の行の並びには戻らない。

変換オプション（外字を画像にするか実体参照にするか等）は木に入らない。木は入力だけで
決まり、オプションは描画の段で与える（[Aozora AST 交換形式](spec-aozora-ast-json.md)の
不変条件 4）。同じ JSON から異なるオプションで描き分けられる。

## 4. 使い方

```
# テキスト → JSON
aozora2 ast 入力.txt --tree aozora > doc.json
aozora2 ast --zip 作品.zip --tree raw --pretty > doc.json

# JSON → HTML／プレーンテキスト
aozora2 html --from-ast doc.json
aozora2 strip --from-ast doc.json
```

`html --from-ast` は `--use-jisx0213` などの変換オプションを通常どおり受け取る。
`strip --from-ast` は本文（`main_text`）だけを平文にする（後付けは元から対象外）。

## 5. 未決（議論が必要な点）

- **節の持ち方**。`after_text` と `bibliographical` を別のキーにしたが、節を配列にして
  種類を各要素に持たせる案もある。節が増えたときはそちらが素直になる。
- **`tree` の位置**。中身の木の種類を器の側に持たせているが、各節の中に持たせる案もある
  （節ごとに違う木を入れる用途は今のところ無い）。
- **ヘッダの原文**。完全な可逆性が要るなら、抽出結果ではなくヘッダ行そのものを持つ形に
  なる。今は「HTML を組み立て直せること」までを目標にしている。
