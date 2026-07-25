//! html サブコマンド
//!
//! 青空文庫形式をHTMLに変換

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use aozora_core::zip::{is_zip_file, read_first_txt_from_zip};
use clap::Args as ClapArgs;
use encoding_rs::SHIFT_JIS;

use aozora2::html::{self, RenderOptions};

/// html サブコマンドの引数
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

    /// ドキュメントのタイトル
    #[arg(long)]
    pub title: Option<String>,

    /// 出力エンコーディング（utf-8 または shift_jis）
    #[arg(long, default_value = "shift_jis")]
    pub encoding: String,
}

/// html サブコマンドを実行
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

    let input = aozora_core::encoding::decode_to_utf8(&bytes);

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
        .with_unicode(args.use_unicode);

    let options = if let Some(title) = &args.title {
        options.with_title(title)
    } else {
        options
    };

    // 変換
    let output_html = html::convert(&input, &options);

    // エンコーディング変換
    let output_bytes = if args.encoding.to_lowercase() == "shift_jis" {
        let (encoded, _, _) = SHIFT_JIS.encode(&output_html);
        encoded.into_owned()
    } else {
        output_html.into_bytes()
    };

    // 出力
    match &args.output {
        Some(path) => {
            fs::write(path, &output_bytes)?;
        }
        None => {
            io::stdout().write_all(&output_bytes)?;
        }
    }

    Ok(())
}
