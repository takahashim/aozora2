//! 青空文庫形式をプレーンテキストに変換するCLIツール

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

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
    let output = aozora2text::convert(&bytes);

    // 出力
    match &args.output {
        Some(path) => fs::write(path, &output)?,
        None => io::stdout().write_all(output.as_bytes())?,
    }

    Ok(())
}

/// ZIPファイルかどうかをマジックバイトで判定
fn is_zip_file(bytes: &[u8]) -> bool {
    // ZIPマジックバイト: PK\x03\x04 (通常) または PK\x05\x06 (空アーカイブ)
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// ZIPから最初の.txtファイルを読み込む
fn read_first_txt_from_zip(path: &Path) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to read ZIP archive: {}", e),
        )
    })?;

    // .txt ファイルを探す
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let name = entry.name().to_lowercase();
        if name.ends_with(".txt") && !entry.is_dir() {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no .txt file found in ZIP archive",
    ))
}
