//! 2 つの交換形式の**文書としての器**（docs/spec-rawast-json.md・
//! docs/spec-aozora-ast-json.md の「文書全体」）。
//!
//! どちらの形式も文書 1 本をそのまま表す。第 3 の形式は作らない。
//!
//! - [`RawDocument`] は**ファイルの全行**を持つ。`source` が原文そのものなので、
//!   連結すれば元のテキストに戻る（RawAST の不変条件「可逆」）。ヘッダも底本も
//!   行として入っているので、節への切り分けはここから導ける。
//! - [`AozoraDocument`] は節ごとに畳んだ木を持つ。原文を保持しない形式なので、
//!   木から導けない文書レベルの情報（ヘッダ・くの字点・末尾の改行）だけを併せ持つ。

use crate::ast::Block;
use crate::document::{
    after_text_range, bibliographical_range, body_line_indices, extract_header_info, HeaderInfo,
};
use crate::html::{render_document_from_sections, DocumentSections, KunojiUse, RenderOptions};
use crate::lower::lower_to_blocks;
use crate::parser::{parse_document_raw, RawDoc, RawLine};

/// RawAST 交換形式の名前
pub const RAWAST_FORMAT: &str = "aozora-rawast";
/// Aozora AST 交換形式の名前
pub const AOZORA_FORMAT: &str = "aozora-ast";
/// 両形式の版
pub const VERSION: &str = "0.1";

/// RawAST の文書（docs/spec-rawast-json.md「文書全体」）。
///
/// `lines` は**ファイルの全行**。ヘッダも注記凡例も底本情報も含む。末尾が改行なら
/// 最後に空行が 1 つ入るので、そこも含めて原文に戻る。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawDocument {
    /// 形式の名前（`aozora-rawast`）
    pub format: String,
    /// 形式の版
    pub version: String,
    /// ファイルの全行
    pub lines: Vec<RawLine>,
}

/// Aozora AST の文書（docs/spec-aozora-ast-json.md「文書全体」）。
///
/// 節ごとに畳んだ木と、木から導けない文書レベルの情報を持つ。後者は
/// [`KunojiUse`] と `ends_with_newline` の 2 つで、どちらも出力の形を決める
/// 互換メタデータである（`Break` や `CloseKind` と同じ性格のもの）。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AozoraDocument {
    /// 形式の名前（`aozora-ast`）
    pub format: String,
    /// 形式の版
    pub version: String,
    /// ヘッダから抽出した題名・著者など
    pub header: HeaderInfo,
    /// 本文
    pub main_text: Vec<Block>,
    /// 本文終わり後（`［＃本文終わり］` 以降）。無ければ空。
    pub after_text: Vec<Block>,
    /// 底本情報（`底本：` 以降）。無ければ空。
    pub bibliographical: Vec<Block>,
    /// くの字点の使用（フッタ「表記について」用）
    pub kunoji: KunojiUse,
    /// 入力が改行で終わっているか（文書末尾の `<br />` の数が変わる）
    pub ends_with_newline: bool,
}

/// 参照実装と同じ行分割（CRLF 区切り）。末尾の空行も残す（原文に戻すため）。
fn split_lines(input: &str) -> Vec<&str> {
    input.split("\r\n").collect()
}

/// 描画・節分けに使う行（末尾の空行を落とした形）。参照実装の行の数え方に合わせる。
fn content_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut lines = lines.to_vec();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

impl RawDocument {
    /// テキストから作る。
    pub fn from_text(input: &str) -> Self {
        Self {
            format: RAWAST_FORMAT.to_string(),
            version: VERSION.to_string(),
            lines: parse_document_raw(&split_lines(input)).lines,
        }
    }

    /// 各行の原文を連結して元のテキストに戻す（不変条件「可逆」）。
    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.source.as_str())
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    /// 節ごとに畳んで Aozora AST の文書にする。
    pub fn to_aozora(&self) -> AozoraDocument {
        let sources: Vec<&str> = self.lines.iter().map(|l| l.source.as_str()).collect();
        let lines = content_lines(&sources);

        // 参照実装が補う空行（原文に対応が無い行）は、空の RawLine を作って埋める。
        let empty = |line_no: usize| RawLine {
            source: String::new(),
            nodes: Vec::new(),
            line_no,
            unclosed_accents: Vec::new(),
            unclosed_accent_to_eol: false,
        };
        let main_text: Vec<RawLine> = body_line_indices(&lines)
            .into_iter()
            .enumerate()
            .map(|(n, i)| i.map_or_else(|| empty(n), |i| self.lines[i].clone()))
            .collect();
        let slice = |range: Option<std::ops::Range<usize>>| -> Vec<RawLine> {
            range.map_or_else(Vec::new, |r| self.lines[r].to_vec())
        };
        let after_text = slice(after_text_range(&lines));
        let bibliographical = slice(bibliographical_range(&lines));

        // くの字点は**節に属する行だけ**を数える。どの節にも入らない行（先頭の
        // 注記凡例など）は描画されないので数えない。凡例には記法の説明として
        // `「くの字点」は「／＼」で表しました` と書かれていることがある。
        let mut kunoji = KunojiUse::default();
        for line in main_text.iter().chain(&after_text).chain(&bibliographical) {
            kunoji.scan(&line.source);
        }

        AozoraDocument {
            format: AOZORA_FORMAT.to_string(),
            version: VERSION.to_string(),
            header: extract_header_info(&lines),
            main_text: lower_to_blocks(&RawDoc { lines: main_text }),
            after_text: lower_to_blocks(&RawDoc { lines: after_text }),
            bibliographical: lower_to_blocks(&RawDoc {
                lines: bibliographical,
            }),
            kunoji,
            ends_with_newline: sources.last() == Some(&""),
        }
    }

    /// 完全な HTML 文書を組み立てる（畳んでから描く）。
    pub fn to_html(&self, options: &RenderOptions) -> String {
        self.to_aozora().to_html(options)
    }
}

impl AozoraDocument {
    /// テキストから作る。
    pub fn from_text(input: &str) -> Self {
        RawDocument::from_text(input).to_aozora()
    }

    /// 完全な HTML 文書を組み立てる。
    pub fn to_html(&self, options: &RenderOptions) -> String {
        render_document_from_sections(
            &self.header,
            DocumentSections {
                main_text: &self.main_text,
                after_text: &self.after_text,
                bibliographical: &self.bibliographical,
            },
            &self.kunoji,
            self.ends_with_newline,
            options,
        )
    }
}
