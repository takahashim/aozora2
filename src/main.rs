//! 青空文庫形式をプレーンテキストに変換するCLIツール

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use flate2::read::DeflateDecoder;
use zip::CompressionMethod;

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
            format!("failed to read ZIP archive: {} ({})", e, path.display()),
        )
    })?;

    // .txt ファイルを探す
    for i in 0..archive.len() {
        let mut entry = archive.by_index_raw(i).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to read ZIP entry: {} ({})", e, path.display()),
            )
        })?;

        let entry_name = entry.name().to_string();
        let name = entry_name.to_lowercase();
        if name.ends_with(".txt") && !entry.is_dir() {
            return read_zip_entry_bytes(&mut entry, path, &entry_name);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no .txt file found in ZIP archive: {}", path.display()),
    ))
}

fn read_zip_entry_bytes(
    entry: &mut zip::read::ZipFile<'_>,
    path: &Path,
    entry_name: &str,
) -> io::Result<Vec<u8>> {
    if entry.encrypted() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "encrypted ZIP entry is not supported: {} ({})",
                entry_name,
                path.display()
            ),
        ));
    }

    let mut compressed = Vec::new();
    entry.read_to_end(&mut compressed).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "failed to read ZIP entry: {} ({} in {})",
                e,
                entry_name,
                path.display()
            ),
        )
    })?;

    match entry.compression() {
        CompressionMethod::Stored => Ok(compressed),
        CompressionMethod::Deflated => {
            let mut decoder = DeflateDecoder::new(&compressed[..]);
            let mut out = Vec::new();
            if entry.size() <= usize::MAX as u64 {
                out.reserve(entry.size() as usize);
            }
            decoder.read_to_end(&mut out).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "failed to decompress ZIP entry: {} ({} in {})",
                        e,
                        entry_name,
                        path.display()
                    ),
                )
            })?;
            Ok(out)
        }
        method => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported ZIP compression method {:?}: {} ({})",
                method,
                entry_name,
                path.display()
            ),
        )),
    }
}
