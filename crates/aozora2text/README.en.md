# aozora2text

[![CI](https://github.com/takahashim/aozora2text/actions/workflows/ci.yml/badge.svg)](https://github.com/takahashim/aozora2text/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/aozora2text.svg)](https://crates.io/crates/aozora2text)

A Rust tool to convert [Aozora Bunko](https://www.aozora.gr.jp/) format text to plain text.

[In Japanese](./README.md)

## Features

- Remove (implicit) ruby annotations `《》`
- Remove explicit ruby annotations `｜...《》`
- Remove annotation commands `［＃...］`
- Convert gaiji (external characters) `※［＃...］` to Unicode
- Convert accent notation `〔...〕` to accented characters
- Remove header (title/author) and footer (source info)
- Auto-detect UTF-8 / Shift_JIS encoding
- Support ZIP files (Aozora Bunko distribution format)

## Installation

```bash
cargo install aozora2text
```

## Usage

### Command Line

```bash
# Convert a file
aozora2text input.txt -o output.txt

# Use stdin/stdout
cat input.txt | aozora2text > output.txt

# ZIP file (Aozora Bunko download format)
aozora2text --zip wagahaiwa_nekodearu.zip -o output.txt
```

### Library

```rust
// High-level API (with body extraction)
let input = "Title\nAuthor\n\n吾輩《わがはい》は猫である\n底本：青空文庫";
let plain = aozora2text::convert(input.as_bytes());
assert_eq!(plain, "吾輩は猫である\n");

// Low-level API (single line)
let line = "吾輩《わがはい》は猫《ねこ》である";
let plain = aozora2text::convert_line(line);
assert_eq!(plain, "吾輩は猫である");
```

## Conversion Examples

| Input | Output |
|-------|--------|
| `漢字《かんじ》` | `漢字` |
| `｜東京《とうきょう》` | `東京` |
| `猫である［＃「である」に傍点］` | `猫である` |
| `※［＃「丸印」、U+25CB］` | `○` |
| `〔cafe'〕` | `café` |

## License

MIT
