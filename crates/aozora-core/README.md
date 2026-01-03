# aozora-core

青空文庫形式のテキストを処理するコアライブラリです。

[In English](./README.en.md)

## 機能

- トークナイザ（字句解析）
- パーサー（構文解析）
- 外字（JIS外文字）変換
- アクセント記号変換
- エンコーディング検出・変換（UTF-8 / Shift_JIS）
- ZIPファイル処理

## 使用例

```rust
use aozora_core::tokenizer::tokenize;
use aozora_core::parser::parse;

// トークナイズ
let tokens = tokenize("漢字《かんじ》");
assert_eq!(tokens.len(), 2);

// パース
let nodes = parse(&tokens);
```

## ライセンス

MIT
