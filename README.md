# aozora-rs

[![CI](https://github.com/takahashim/aozora-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/takahashim/aozora-rs/actions/workflows/ci.yml)

青空文庫形式のテキストを処理するRustツール群です。

[In English](./README.en.md)

## パッケージ

| パッケージ | crates.io | 説明 |
|-----------|-----------|------|
| [aozora-core](./crates/aozora-core/) | [![crates.io](https://img.shields.io/crates/v/aozora-core.svg)](https://crates.io/crates/aozora-core) | 共通ライブラリ（トークナイザ、パーサー、外字変換等） |
| [aozora2](./crates/aozora2/) | [![crates.io](https://img.shields.io/crates/v/aozora2.svg)](https://crates.io/crates/aozora2) | HTML変換等の統合CLI |
| [aozora2text](./crates/aozora2text/) | [![crates.io](https://img.shields.io/crates/v/aozora2text.svg)](https://crates.io/crates/aozora2text) | プレーンテキスト変換CLI(aozora2の薄いラッパー) |

## ライセンス

MIT
