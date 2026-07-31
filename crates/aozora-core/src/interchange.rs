//! 文書まるごとの交換形式（`aozora-document`）。
//!
//! [RawAST](crate::parser::RawDoc) と [Aozora AST](crate::AozoraAst) の交換形式
//! （docs/spec-rawast-json.md・docs/spec-aozora-ast-json.md）は、どちらも**本文だけ**を
//! 表す。ヘッダ（題名・著者）も底本情報も含まないので、その 2 つだけでは完全な HTML
//! 文書を組み立て直せない。ここはその不足分を添えて、文書 1 本を往復できるようにする器。
//!
//! 木の外から要るのは 3 つだけである。
//!
//! - **ヘッダ情報**: `<head>` と `<div class="metadata">` の材料。行から抽出済みの
//!   [`HeaderInfo`] を持つ（原文の行は持たない）。
//! - **くの字点**: フッタ「表記について」の項目。参照実装は**生のソース行**を走査して
//!   決める（注記の中に書かれていても拾うため）。Aozora AST は原文を保持しないので、
//!   走査の結果だけを持ち回る。
//! - **末尾が改行か**: 文書末尾の `<br />` の数が変わる（docs/workflow.md 残差一覧）。
//!
//! 木そのものは節ごとに分ける。参照実装が本文と後付け（本文終わり後・底本情報）を
//! 別の規則で出す以上、交換形式でも分けて持つのが素直で、`format` に何の木かを書く。

use crate::ast::Block;
use crate::document::{
    extract_after_text_lines, extract_bibliographical_lines, extract_body_lines,
    extract_header_info, HeaderInfo,
};
use crate::html::{render_document_from_sections, DocumentSections, KunojiUse, RenderOptions};
use crate::lower::lower_to_blocks;
use crate::parser::{parse_document_raw, RawDoc};

/// この器の版。中身の木の版は各交換形式の `version` に従う。
pub const FORMAT: &str = "aozora-document";
/// この器の版。
pub const VERSION: &str = "0.1";

/// 文書 1 本。節ごとの木と、木の外から要る情報を持つ。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Document<T> {
    /// 器の名前（`aozora-document`）
    pub format: String,
    /// 器の版
    pub version: String,
    /// 中身の木の種類（`aozora-ast` / `aozora-rawast`）
    pub tree: String,
    /// ヘッダから抽出した題名・著者など
    pub header: HeaderInfo,
    /// 本文
    pub main_text: T,
    /// 本文終わり後（`［＃本文終わり］` 以降）。無ければ空。
    pub after_text: T,
    /// 底本情報（`底本：` 以降）。無ければ空。
    pub bibliographical: T,
    /// くの字点の使用（フッタ「表記について」用。生のソース行を走査した結果）
    pub kunoji: KunojiUse,
    /// 入力が改行で終わっているか
    pub ends_with_newline: bool,
}

/// テキストから、節ごとに畳んだ Aozora AST の文書を作る。
pub fn aozora_document(input: &str) -> Document<Vec<Block>> {
    build(input, "aozora-ast", |lines| {
        lower_to_blocks(&parse_document_raw(lines))
    })
}

/// テキストから、節ごとの RawAST の文書を作る。
pub fn raw_document(input: &str) -> Document<RawDoc> {
    build(input, "aozora-rawast", parse_document_raw)
}

fn build<T>(input: &str, tree: &str, make: impl Fn(&[&str]) -> T) -> Document<T> {
    let lines = split_lines(input);
    let main_text = extract_body_lines(&lines);
    let after_text = extract_after_text_lines(&lines);
    let bibliographical = extract_bibliographical_lines(&lines);

    // くの字点は**節に属する行だけ**を走査する。どの節にも入らない行（先頭の
    // 注記凡例など）は描画されないので数えない。凡例には記法の説明として
    // `「くの字点」は「／＼」で表しました` と書かれていることがあり、全行を
    // 走査するとそれを拾ってフッタの文面が変わってしまう。
    let mut kunoji = KunojiUse::default();
    for line in main_text.iter().chain(&after_text).chain(&bibliographical) {
        kunoji.scan(line);
    }

    Document {
        format: FORMAT.to_string(),
        version: VERSION.to_string(),
        tree: tree.to_string(),
        header: extract_header_info(&lines),
        main_text: make(&main_text),
        after_text: make(&after_text),
        bibliographical: make(&bibliographical),
        kunoji,
        ends_with_newline: input.ends_with('\n'),
    }
}

/// 参照実装と同じ行分割（CRLF 区切り。末尾の空行は落とす）。
fn split_lines(input: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = input.split("\r\n").collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

impl Document<Vec<Block>> {
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

impl Document<RawDoc> {
    /// 節ごとに畳んで Aozora AST の文書にする。
    pub fn lower(&self) -> Document<Vec<Block>> {
        Document {
            format: self.format.clone(),
            version: self.version.clone(),
            tree: "aozora-ast".to_string(),
            header: self.header.clone(),
            main_text: lower_to_blocks(&self.main_text),
            after_text: lower_to_blocks(&self.after_text),
            bibliographical: lower_to_blocks(&self.bibliographical),
            kunoji: self.kunoji.clone(),
            ends_with_newline: self.ends_with_newline,
        }
    }

    /// 完全な HTML 文書を組み立てる（畳んでから描く）。
    pub fn to_html(&self, options: &RenderOptions) -> String {
        self.lower().to_html(options)
    }
}
