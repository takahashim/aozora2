//! body-diff サブコマンド（中立AST移行の一致率計測用）
//!
//! 旧経路（BlockManager）と新経路（中立AST）の本文（main_text 内側）を比較し、
//! 一致なら `MATCH`、不一致なら `DIFF\t<最初の相違の前後>` を1行で出力する。
//! docs/plan-neutral-ast.md の各段でコーパス全件の一致率を測るのに使う。

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use aozora_core::zip::{is_zip_file, read_first_txt_from_zip};
use clap::Args as ClapArgs;

use aozora2::html::{compare_body, RenderOptions};

/// body-diff サブコマンドの引数
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// 入力ファイル（省略時は標準入力）
    pub input: Option<PathBuf>,
    /// 入力をZIPファイルとして扱う
    #[arg(short, long)]
    pub zip: bool,
    /// 本文だけでなく全文書（head/tail/footer 含む）を比較する
    #[arg(long)]
    pub full: bool,
}

pub fn run(args: Args) -> io::Result<()> {
    let bytes = if args.zip {
        let path = args
            .input
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ZIP mode requires input"))?;
        read_first_txt_from_zip(path)?
    } else {
        match &args.input {
            Some(path) => {
                let b = fs::read(path)?;
                if is_zip_file(&b) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "input is a ZIP; use --zip",
                    ));
                }
                b
            }
            None => {
                let mut buf = Vec::new();
                io::stdin().read_to_end(&mut buf)?;
                buf
            }
        }
    };

    let input = aozora_core::encoding::decode_to_utf8(&bytes);
    let (old_body, new_body) = if args.full {
        aozora2::html::compare_full(&input, &RenderOptions::default())
    } else {
        compare_body(&input, &RenderOptions::default())
    };

    if old_body == new_body {
        println!("MATCH");
    } else {
        // 最初の相違位置の前後を出す（デバッグ用）。
        let ob = old_body.as_bytes();
        let nb = new_body.as_bytes();
        let mut i = 0;
        while i < ob.len().min(nb.len()) && ob[i] == nb[i] {
            i += 1;
        }
        let ctx = |s: &str| {
            // マルチバイト境界に落ちないよう前後を char 境界まで広げる。
            let mut start = i.saturating_sub(40);
            while start > 0 && !s.is_char_boundary(start) {
                start -= 1;
            }
            let mut end = (i + 40).min(s.len());
            while end < s.len() && !s.is_char_boundary(end) {
                end += 1;
            }
            s.get(start..end)
                .unwrap_or("")
                .replace('\r', "")
                .replace('\n', "⏎")
        };
        println!(
            "DIFF@{i} (old {} new {})\told:{}\tnew:{}",
            old_body.len(),
            new_body.len(),
            ctx(&old_body),
            ctx(&new_body)
        );
    }
    Ok(())
}
