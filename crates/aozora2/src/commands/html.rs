//! html サブコマンド
//!
//! 青空文庫形式をHTMLに変換

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use clap::Args as ClapArgs;

use aozora2::html::{self, RenderOptions};

/// html サブコマンドの引数
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// 入力ファイル（省略時は標準入力）
    pub input: Option<PathBuf>,

    /// 出力ファイル（省略時は標準出力）
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// 外字画像ディレクトリ
    #[arg(long, default_value = "../../../gaiji/")]
    pub gaiji_dir: String,

    /// CSSファイル（カンマ区切りで複数指定可）
    #[arg(long, default_value = "../../aozora.css")]
    pub css_files: String,

    /// JIS X 0213外字を数値実体参照で表示
    #[arg(long)]
    pub use_jisx0213: bool,

    /// Unicode外字を数値実体参照で表示
    #[arg(long)]
    pub use_unicode: bool,

    /// 完全なHTMLドキュメントを生成
    #[arg(long)]
    pub full_document: bool,

    /// ドキュメントのタイトル
    #[arg(long)]
    pub title: Option<String>,
}

/// html サブコマンドを実行
pub fn run(args: Args) -> io::Result<()> {
    // 入力読み込み
    let input = match &args.input {
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
    let css_files: Vec<String> = args
        .css_files
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let options = RenderOptions::new()
        .with_gaiji_dir(&args.gaiji_dir)
        .with_css_files(css_files)
        .with_jisx0213(args.use_jisx0213)
        .with_unicode(args.use_unicode)
        .with_full_document(args.full_document);

    let options = if let Some(title) = &args.title {
        options.with_title(title)
    } else {
        options
    };

    // 変換
    let output_html = html::convert(&input, &options);

    // 出力
    match &args.output {
        Some(path) => {
            fs::write(path, &output_html)?;
        }
        None => {
            io::stdout().write_all(output_html.as_bytes())?;
        }
    }

    Ok(())
}
