//! aozora-html-converter CLI
//!
//! 青空文庫形式のテキストをHTMLに変換するコマンドラインツール

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::Parser;

use aozora_html_converter::{convert, RenderOptions};

/// 青空文庫形式をHTMLに変換
#[derive(Parser, Debug)]
#[command(name = "aozora-html-converter")]
#[command(about = "Convert Aozora Bunko format to HTML")]
#[command(version)]
struct Cli {
    /// 入力ファイル（省略時は標準入力）
    input: Option<PathBuf>,

    /// 出力ファイル（省略時は標準出力）
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 外字画像ディレクトリ
    #[arg(long, default_value = "../../../gaiji/")]
    gaiji_dir: String,

    /// CSSファイル（カンマ区切りで複数指定可）
    #[arg(long, default_value = "../../aozora.css")]
    css_files: String,

    /// JIS X 0213外字を数値実体参照で表示
    #[arg(long)]
    use_jisx0213: bool,

    /// Unicode外字を数値実体参照で表示
    #[arg(long)]
    use_unicode: bool,

    /// 完全なHTMLドキュメントを生成
    #[arg(long)]
    full_document: bool,

    /// ドキュメントのタイトル
    #[arg(long)]
    title: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    // 入力読み込み
    let input = match &cli.input {
        Some(path) => {
            let bytes = fs::read(path)?;
            aozora_core::encoding::decode_to_utf8(&bytes)
        }
        None => {
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer)?;
            aozora_core::encoding::decode_to_utf8(&buffer)
        }
    };

    // オプション設定
    let css_files: Vec<String> = cli
        .css_files
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let options = RenderOptions::new()
        .with_gaiji_dir(&cli.gaiji_dir)
        .with_css_files(css_files)
        .with_jisx0213(cli.use_jisx0213)
        .with_unicode(cli.use_unicode)
        .with_full_document(cli.full_document);

    let options = if let Some(title) = &cli.title {
        options.with_title(title)
    } else {
        options
    };

    // 変換
    let html = convert(&input, &options);

    // 出力
    match &cli.output {
        Some(path) => {
            fs::write(path, &html)?;
        }
        None => {
            io::stdout().write_all(html.as_bytes())?;
        }
    }

    Ok(())
}
