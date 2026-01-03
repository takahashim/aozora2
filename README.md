# aozora2

[![CI](https://github.com/takahashim/aozora2/actions/workflows/ci.yml/badge.svg)](https://github.com/takahashim/aozora2/actions/workflows/ci.yml)

青空文庫形式のテキストを変換するRustツールです。

[In English](./README.en.md)

## インストール

```bash
cargo install aozora2
```

## 使い方

### プレーンテキストに変換 (strip)

ルビ・注記を除去してプレーンテキストに変換します。

```bash
aozora2 strip input.txt -o output.txt
aozora2 strip --zip archive.zip -o output.txt
cat input.txt | aozora2 strip > output.txt
```

### HTMLに変換 (html)

青空文庫形式をHTMLに変換します。

```bash
aozora2 html input.txt -o output.html
aozora2 html input.txt --full-document --title "タイトル" -o output.html
```

オプション:
- `--full-document` - 完全なHTMLドキュメントを生成
- `--title <TITLE>` - ドキュメントのタイトル
- `--gaiji-dir <DIR>` - 外字画像ディレクトリ
- `--css-files <FILES>` - CSSファイル（カンマ区切り）

## パッケージ

| パッケージ | crates.io | 説明 |
|-----------|-----------|------|
| [aozora2](./crates/aozora2/) | [![crates.io](https://img.shields.io/crates/v/aozora2.svg)](https://crates.io/crates/aozora2) | メインCLI（strip, html サブコマンド） |
| [aozora-core](./crates/aozora-core/) | [![crates.io](https://img.shields.io/crates/v/aozora-core.svg)](https://crates.io/crates/aozora-core) | コアライブラリ（トークナイザ、パーサー、外字変換等） |
| [aozora2text](./crates/aozora2text/) | [![crates.io](https://img.shields.io/crates/v/aozora2text.svg)](https://crates.io/crates/aozora2text) | 後方互換CLI（`aozora2 strip` のラッパー） |

## ライセンス

MIT
