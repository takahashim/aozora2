//! strip サブコマンド
//!
//! 青空文庫形式をプレーンテキストに変換

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

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

    /// 入力を青空文庫形式のテキストではなく、交換形式の JSON として読む
    /// （`ast` サブコマンドの出力）
    #[arg(long)]
    pub from_ast: bool,
}

/// strip サブコマンドを実行
pub fn run(args: Args) -> io::Result<()> {
    let output = if args.from_ast {
        // 交換形式から読み戻して本文だけを平文にする（ヘッダ・底本は元から対象外）。
        let doc = super::ast::read_document(args.input.as_deref())?;
        strip::convert_blocks(&doc.main_text)
    } else {
        let bytes = super::read_input(args.input.as_deref(), args.zip)?;
        strip::convert(&bytes)
    };

    // 出力
    match &args.output {
        Some(path) => fs::write(path, &output)?,
        None => io::stdout().write_all(output.as_bytes())?,
    }

    Ok(())
}
