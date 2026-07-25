//! aozora2text - 青空文庫形式をプレーンテキストに変換
//!
//! このコマンドは `aozora2 strip` の薄いラッパーです。
//! 新規ユーザーは `aozora2` コマンドの使用を推奨します。

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use aozora2::aozora_core::zip::{is_zip_file, read_first_txt_from_zip};
use aozora2::strip;
use clap::Parser;

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

    /// 入力をZIPファイルとして扱う
    #[arg(short, long)]
    zip: bool,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // 入力読み込み
    let bytes = if args.zip {
        let path = args.input.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "ZIP mode requires an input file",
            )
        })?;
        read_first_txt_from_zip(path)?
    } else {
        match &args.input {
            Some(path) => {
                let bytes = fs::read(path)?;
                if is_zip_file(&bytes) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "input appears to be a ZIP file; use --zip option",
                    ));
                }
                bytes
            }
            None => {
                let mut buf = Vec::new();
                io::stdin().read_to_end(&mut buf)?;
                buf
            }
        }
    };

    // 変換
    let output = strip::convert(&bytes);

    // 出力
    match &args.output {
        Some(path) => fs::write(path, &output)?,
        None => io::stdout().write_all(output.as_bytes())?,
    }

    Ok(())
}
