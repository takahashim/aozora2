# aozora2text

[![CI](https://github.com/takahashim/aozora2text/actions/workflows/ci.yml/badge.svg)](https://github.com/takahashim/aozora2text/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/aozora2text.svg)](https://crates.io/crates/aozora2text)

青空文庫形式のテキスト本文をプレーンテキストに変換するCLIツールです。

元テキストがShift_JISでもUTF-8でも、UTF-8として出力します。

[In English](./README.en.md)


## 機能

- ルビ（暗黙ルビ） `《》` の除去
- 明示ルビ `｜...《》` の除去
- 注記コマンド `［＃...］` の除去
- 外字 `※［＃...］` のUnicode変換
    - 変換できない文字は`〓`になります
- アクセント記号 `〔...〕` の変換
- 前付け（タイトル・著者、注記の説明）と後付け（底本情報）の除去
- UTF-8 / Shift_JIS 自動判定
- ZIPファイル対応

## インストール

```bash
cargo install aozora2text
```

## 使い方

### コマンドライン

```bash
# ファイルを変換
aozora2text input.txt -o output.txt

# 標準入出力
cat input.txt | aozora2text > output.txt

# ZIPファイル（青空文庫配布形式）
aozora2text --zip wagahaiwa_nekodearu.zip -o output.txt
```

### ライブラリ

```rust
// 高レベルAPI（本文抽出あり）
let input = "タイトル\n著者\n\n吾輩《わがはい》は猫である\n底本：青空文庫";
let plain = aozora2text::convert(input.as_bytes());
assert_eq!(plain, "吾輩は猫である\n");

// 低レベルAPI（1行変換）
let line = "吾輩《わがはい》は猫《ねこ》である";
let plain = aozora2text::convert_line(line);
assert_eq!(plain, "吾輩は猫である");
```

## 変換例

| 入力 | 出力 |
|------|------|
| `漢字《かんじ》` | `漢字` |
| `｜東京《とうきょう》` | `東京` |
| `猫である［＃「である」に傍点］` | `猫である` |
| `※［＃「丸印」、U+25CB］` | `○` |

## ライセンス

MIT
