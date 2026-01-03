//! strip サブコマンド
//!
//! 青空文庫形式をプレーンテキストに変換

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use aozora_core::zip::{is_zip_file, read_first_txt_from_zip};
use clap::Args as ClapArgs;

use aozora2::strip;

/// strip サブコマンドの引数
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// 入力ファイル（省略時は標準入力）
    pub input: Option<PathBuf>,

    /// 出力ファイル（省略時は標準出力）
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// 入力をZIPファイルとして扱う
    #[arg(short, long)]
    pub zip: bool,
}

/// strip サブコマンドを実行
pub fn run(args: Args) -> io::Result<()> {
    // 入力読み込み
    let bytes = if args.zip {
        // ZIPモード
        let path = args.input.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "ZIP mode requires an input file",
            )
        })?;
        read_first_txt_from_zip(path)?
    } else {
        // 通常モード
        match &args.input {
            Some(path) => {
                let bytes = fs::read(path)?;
                // ZIPファイルの誤用を検出
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
