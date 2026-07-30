//! html サブコマンド
//!
//! 青空文庫形式をHTMLに変換

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use aozora_core::encoding::{chars_not_allowed, CharsetPolicy};
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

    /// Shift_JIS 出力時に許す文字の範囲（`--encoding shift_jis` のときだけ効く）
    #[arg(long, value_enum, default_value_t = Charset::Lenient)]
    pub charset: Charset,
}

/// `--charset` の値。既定は通す（従来どおり）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Charset {
    /// **既定**。符号化できない文字も止めず、WHATWG 仕様どおり数値文字参照
    /// （`&#128512;` 等）に置き換えて出力する。置き換えが起きたときは標準エラーに
    /// 知らせるので、出力そのものは従来と変わらない。
    Lenient,
    /// Shift_JIS（CP932）で符号化できない文字があればエラーにする。
    Cp932,
    /// 青空文庫形式の入力規則どおり、ASCII と JIS X 0208 以外があればエラーにする
    /// （半角カナや ① 﨑 のような CP932 拡張も拒否する）。
    X0208,
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
        encode_output(&input, &output_html, args.charset)?
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

/// Shift_JIS で書き出すバイト列を作る。`charset` の指定に応じて事前に検査する。
///
/// 検査は **HTML ではなく入力の青空文庫テキスト**に対して行う。HTML に現れる
/// 符号化できない文字はすべて入力由来（テンプレート側の文字はすべて X 0208）なので、
/// 入力の行・桁で報せた方が直す場所が分かる。
fn encode_output(input: &str, html: &str, charset: Charset) -> io::Result<Vec<u8>> {
    let policy = match charset {
        Charset::Lenient => None,
        Charset::Cp932 => Some(CharsetPolicy::Cp932),
        Charset::X0208 => Some(CharsetPolicy::X0208),
    };

    if let Some(policy) = policy {
        let bad = chars_not_allowed(input, policy);
        if !bad.is_empty() {
            let shown: Vec<String> = bad
                .iter()
                .take(5)
                .map(|c| format!("{}行{}桁の {:?}", c.line + 1, c.column + 1, c.ch))
                .collect();
            let rest = bad.len().saturating_sub(shown.len());
            let tail = if rest > 0 {
                format!("（ほか {rest} 件）")
            } else {
                String::new()
            };
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Shift_JIS で書き出せない文字があります: {}{tail}",
                    shown.join("、")
                ),
            ));
        }
    }

    let (encoded, _, had_errors) = SHIFT_JIS.encode(html);
    if had_errors {
        // 既定（Lenient）は従来どおり出力を止めない。ただし黙って数値文字参照に
        // 化けるのは気付けないので、標準エラーにだけ知らせる（標準出力は不変）。
        eprintln!(
            "警告: Shift_JIS にできない文字を数値文字参照に置き換えました。\
             --charset cp932 を付けるとエラーにできます。"
        );
    }
    Ok(encoded.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str =
        "作品名\r\n著者\r\n\r\n本文に😀と、ⅹ と、普通の字。\r\n\r\n底本：「テスト」\r\n";

    /// 既定は従来どおり通す。符号化できない文字は数値文字参照になり、
    /// 出力も終了コードも変わらない（気付けるよう標準エラーにだけ知らせる）。
    #[test]
    fn lenient_passes_through() {
        let html = "本文に😀と、ⅹ と。";
        let bytes = encode_output(SRC, html, Charset::Lenient).expect("止まらない");
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        assert!(text.contains("&#128512;"), "{text}");
        assert!(text.contains('ⅹ'), "CP932 拡張はそのまま出る: {text}");
    }

    /// cp932 は符号化できない文字だけを拒む。CP932 拡張（ⅹ）は通す。
    #[test]
    fn cp932_rejects_only_unencodable() {
        let err = encode_output(SRC, "dummy", Charset::Cp932).expect_err("拒否される");
        let msg = err.to_string();
        assert!(msg.contains('😀'), "{msg}");
        assert!(!msg.contains('ⅹ'), "CP932 拡張は通す: {msg}");
        // 位置は入力（青空文庫テキスト）の行・桁で報せる。
        assert!(msg.contains("4行4桁"), "{msg}");
    }

    /// x0208 は入力規則どおり、CP932 拡張も拒む。
    #[test]
    fn x0208_rejects_cp932_extensions_too() {
        let err = encode_output(SRC, "dummy", Charset::X0208).expect_err("拒否される");
        let msg = err.to_string();
        assert!(msg.contains('😀') && msg.contains('ⅹ'), "{msg}");
    }

    /// 問題のない文書はどの指定でも同じバイト列になる。
    #[test]
    fn clean_input_is_identical_under_every_charset() {
        let src = "作品名\r\n著者\r\n\r\n吾輩は猫である。\r\n";
        let html = "吾輩は猫である。";
        let lenient = encode_output(src, html, Charset::Lenient).unwrap();
        for charset in [Charset::Cp932, Charset::X0208] {
            assert_eq!(encode_output(src, html, charset).unwrap(), lenient);
        }
    }
}
