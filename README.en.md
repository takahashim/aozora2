# aozora2

[![CI](https://github.com/takahashim/aozora2/actions/workflows/ci.yml/badge.svg)](https://github.com/takahashim/aozora2/actions/workflows/ci.yml)

A Rust tool for converting Aozora Bunko format text.

[日本語](./README.md)

## Installation

```bash
cargo install aozora2
```

## Usage

### Convert to Plain Text (strip)

Removes ruby annotations and notes, converting to plain text.

```bash
aozora2 strip input.txt -o output.txt
aozora2 strip --zip archive.zip -o output.txt
cat input.txt | aozora2 strip > output.txt
```

### Convert to HTML (html)

Converts Aozora Bunko format to HTML.

```bash
aozora2 html input.txt -o output.html
aozora2 html input.txt --full-document --title "Title" -o output.html
```

Options:
- `--full-document` - Generate a complete HTML document
- `--title <TITLE>` - Document title
- `--gaiji-dir <DIR>` - Gaiji (external character) image directory
- `--css-files <FILES>` - CSS files (comma-separated)

## Packages

| Package | crates.io | Description |
|---------|-----------|-------------|
| [aozora2](./crates/aozora2/) | [![crates.io](https://img.shields.io/crates/v/aozora2.svg)](https://crates.io/crates/aozora2) | Main CLI (strip, html subcommands) |
| [aozora-core](./crates/aozora-core/) | [![crates.io](https://img.shields.io/crates/v/aozora-core.svg)](https://crates.io/crates/aozora-core) | Core library (tokenizer, parser, gaiji conversion, etc.) |
| [aozora2text](./crates/aozora2text/) | [![crates.io](https://img.shields.io/crates/v/aozora2text.svg)](https://crates.io/crates/aozora2text) | Backward-compatible CLI (wrapper for `aozora2 strip`) |

## License

MIT
