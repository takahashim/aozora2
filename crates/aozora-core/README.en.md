# aozora-core

Core library for processing Aozora Bunko format text.

[日本語](./README.md)

## Features

- Tokenizer (lexical analysis)
- Parser (syntax analysis)
- Gaiji (JIS external characters) conversion
- Accent notation conversion
- Encoding detection and conversion (UTF-8 / Shift_JIS)
- ZIP file processing

## Usage

```rust
use aozora_core::tokenizer::tokenize;
use aozora_core::parser::parse;

// Tokenize
let tokens = tokenize("漢字《かんじ》");
assert_eq!(tokens.len(), 2);

// Parse
let nodes = parse(&tokens);
```

## License

MIT
