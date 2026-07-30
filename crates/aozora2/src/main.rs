//! aozora2 CLI
//!
//! 青空文庫形式の変換ツール

use clap::{Parser, Subcommand};
use std::io;

mod commands;

#[derive(Parser)]
#[command(name = "aozora2")]
#[command(version)]
#[command(about = "青空文庫形式の変換ツール")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// プレーンテキストに変換（注記・ルビを除去）
    Strip(commands::strip::Args),
    /// HTMLに変換
    Html(commands::html::Args),
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result: io::Result<()> = match cli.command {
        Commands::Strip(args) => commands::strip::run(args),
        Commands::Html(args) => commands::html::run(args),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        // `io::Result` を main から返すと Debug 表記（`Custom { kind: …, error: "…" }`）で
        // 出てしまうので、人が読む文面だけを標準エラーに出す。
        Err(e) => {
            eprintln!("エラー: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
