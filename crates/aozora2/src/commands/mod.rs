//! CLI サブコマンド

pub mod ast;
pub mod html;
pub mod strip;

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use aozora_core::zip::{is_zip_file, read_first_txt_from_zip};

/// 入力を読む（ファイル・標準入力・ZIP）。サブコマンド共通。
///
/// ZIP を `--zip` 無しで渡す取り違えが多いので、中身を見て気付けるようにしている。
pub fn read_input(input: Option<&Path>, zip: bool) -> io::Result<Vec<u8>> {
    if zip {
        let path = input.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "ZIP mode requires an input file",
            )
        })?;
        return read_first_txt_from_zip(path);
    }

    match input {
        Some(path) => {
            let bytes = fs::read(path)?;
            if is_zip_file(&bytes) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "input appears to be a ZIP file; use --zip option",
                ));
            }
            Ok(bytes)
        }
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}
