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

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Strip(args) => commands::strip::run(args),
        Commands::Html(args) => commands::html::run(args),
    }
}
