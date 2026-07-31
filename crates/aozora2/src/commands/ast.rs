//! ast サブコマンド
//!
//! 青空文庫形式のテキストを、交換形式の JSON（docs/spec-rawast-json.md /
//! docs/spec-aozora-ast-json.md）として書き出す。逆向き（JSON → HTML／
//! プレーンテキスト）は `html --from-ast` / `strip --from-ast`。

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use aozora_core::interchange::{AozoraDocument, RawDocument};
use clap::Args as ClapArgs;

/// 出力する木の種類
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Tree {
    /// 記法を解決した正規形（描画・変換向け）
    Aozora,
    /// 記法をそのまま写した構文の木（フォーマッタ・記法リンタ・エディタ支援向け）
    Raw,
}

/// ast サブコマンドの引数
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

    /// 書き出す木の種類
    #[arg(long, value_enum, default_value_t = Tree::Aozora)]
    pub tree: Tree,

    /// 人が読める形に整形する
    #[arg(long)]
    pub pretty: bool,
}

/// ast サブコマンドを実行
pub fn run(args: Args) -> io::Result<()> {
    let bytes = super::read_input(args.input.as_deref(), args.zip)?;
    let input = aozora_core::encoding::decode_to_utf8(&bytes);

    let json = match args.tree {
        Tree::Aozora => to_json(&AozoraDocument::from_text(&input), args.pretty),
        Tree::Raw => to_json(&RawDocument::from_text(&input), args.pretty),
    }?;

    match &args.output {
        Some(path) => fs::write(path, json.as_bytes())?,
        None => io::stdout().write_all(json.as_bytes())?,
    }
    Ok(())
}

fn to_json<T: serde::Serialize>(value: &T, pretty: bool) -> io::Result<String> {
    let mut json = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("直列化できない: {e}")))?;
    json.push('\n');
    Ok(json)
}

/// `--from-ast` で読み込んだ JSON を、描画できる文書に読み戻す。
///
/// `format` の値で 2 つの交換形式を見分ける（RawAST なら畳んでから返す）。
pub fn read_document(path: Option<&std::path::Path>) -> io::Result<AozoraDocument> {
    let mut text = String::new();
    match path {
        Some(p) => text = fs::read_to_string(p)?,
        None => {
            io::stdin().read_to_string(&mut text)?;
        }
    }
    let bad = |e: serde_json::Error| {
        io::Error::new(io::ErrorKind::InvalidData, format!("読み戻せない: {e}"))
    };

    let probe: serde_json::Value = serde_json::from_str(&text).map_err(bad)?;
    match probe.get("format").and_then(|v| v.as_str()) {
        Some(aozora_core::interchange::AOZORA_FORMAT) => serde_json::from_str(&text).map_err(bad),
        Some(aozora_core::interchange::RAWAST_FORMAT) => serde_json::from_str::<RawDocument>(&text)
            .map(|d| d.to_aozora())
            .map_err(bad),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "`format` が `{}` でも `{}` でもない: {}",
                aozora_core::interchange::AOZORA_FORMAT,
                aozora_core::interchange::RAWAST_FORMAT,
                other.unwrap_or("(無し)")
            ),
        )),
    }
}
