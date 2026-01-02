//! 青空文庫形式をプレーンテキストに変換するCLIツール

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::Parser;

use aozora2text::document::extract_body_lines;
use aozora2text::encoding::decode_to_utf8;
use aozora2text::extractor::PlainTextExtractor;
use aozora2text::tokenizer::Tokenizer;

#[derive(Parser)]
#[command(name = "aozora2text")]
#[command(version)]
#[command(about = "青空文庫形式をプレーンテキストに変換")]
struct Args {
    /// 入力ファイル（省略時は標準入力）
    input: Option<PathBuf>,

    /// 出力ファイル（省略時は標準出力）
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // 入力読み込み
    let bytes = match &args.input {
        Some(path) => fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };

    // エンコーディング判定・変換（UTF-8 or Shift_JIS）
    let text = decode_to_utf8(&bytes);

    // 本文抽出
    let lines: Vec<&str> = text.lines().collect();
    let body_lines = extract_body_lines(&lines);

    // 各行をトークナイズ→プレーンテキスト化
    let mut output = String::new();
    for line in body_lines {
        let mut tokenizer = Tokenizer::new(line);
        let tokens = tokenizer.tokenize();
        let plain = PlainTextExtractor::extract(&tokens);
        output.push_str(&plain);
        output.push('\n');
    }

    // 出力
    match &args.output {
        Some(path) => fs::write(path, &output)?,
        None => io::stdout().write_all(output.as_bytes())?,
    }

    Ok(())
}
