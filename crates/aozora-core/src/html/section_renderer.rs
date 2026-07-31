//! 文書セクション（本文／本文終わり後／底本情報）の描画。
//!
//! 参照実装は本文を `general_output`、tail（本文終わり後・底本情報）を
//! `tail_output` で出す。どちらも「行を読む→描画する」同じ手順で、違うのは
//! 枠の HTML・自動リンクの有無・外字記号 ※ を出すかだけ。ここはその違いを
//! [`Section`] に持たせ、手順は1つに保つ器。

use crate::document::{
    extract_after_text_lines, extract_bibliographical_lines, extract_body_lines,
};
use crate::lower::lower_to_blocks;
use crate::parser::parse_document_raw;

use super::block_renderer::BlockRenderer;
use super::document_renderer::DocumentRenderer;
use super::notation::NotationState;
use super::options::RenderOptions;
use super::presentation::auto_link;

/// 文書のセクション。どの行を担当し、どう枠を出すかを自分で知っている。
pub enum Section {
    /// 本文（main_text）
    MainText {
        /// このセクションが文書の最後か（後続の tail セクションがどちらも空）。
        ///
        /// 参照は `process` の最後に `tail_output` を無条件に 1 回呼び（空バッファなら
        /// `<br />`）、続く `hyoki` も先頭に `<br />` を出す。つまり**最後に描かれた
        /// セクション**の閉じ `</div>` の前に `<br />` が 2 つ入る。底本行が無い文書では
        /// それが main_text になる。
        is_last: bool,
        /// 入力が改行で終わっているか（末尾 `<br />` の数が変わる）
        input_ends_with_newline: bool,
    },
    /// 本文終わり後（after_text）
    AfterText,
    /// 底本情報（bibliographical_information）
    Bibliographical {
        /// 入力が改行で終わっているか（末尾 `<br />` の数が変わる）
        input_ends_with_newline: bool,
    },
}

impl Section {
    /// 文書全体の行からこのセクションの行を取り出す。
    fn lines<'a>(&self, all_lines: &[&'a str]) -> Vec<&'a str> {
        match self {
            Section::MainText { .. } => extract_body_lines(all_lines),
            Section::AfterText => extract_after_text_lines(all_lines),
            Section::Bibliographical { .. } => extract_bibliographical_lines(all_lines),
        }
    }

    /// tail セクション（参照 tail_output）か。tail は出力へ自動リンクを掛け、
    /// 画像化できない外字の ※ を出さない。
    fn is_tail(&self) -> bool {
        !matches!(self, Section::MainText { .. })
    }

    /// 担当行が無いとき枠ごと出さないか。main_text の枠は空でも必ず出す。
    fn skip_when_empty(&self) -> bool {
        self.is_tail()
    }

    fn render_open(&self, doc: &DocumentRenderer, out: &mut String) {
        match self {
            Section::MainText { .. } => doc.render_main_text_start(out),
            Section::AfterText => doc.render_after_text_header(out),
            Section::Bibliographical { .. } => doc.render_bibliographical_header(out),
        }
    }

    fn render_close(&self, doc: &DocumentRenderer, out: &mut String) {
        match self {
            Section::MainText {
                is_last,
                input_ends_with_newline,
            } => doc.render_main_text_end(out, *is_last, *input_ends_with_newline),
            Section::AfterText => doc.render_after_text_footer(out),
            Section::Bibliographical {
                input_ends_with_newline,
            } => doc.render_bibliographical_footer(out, *input_ends_with_newline),
        }
    }
}

/// セクションを描画する。「表記について」の材料は全セクションを通して
/// 1つの [`BlockRenderer`] に溜まる（参照実装のフラグ・外字一覧と同じ）。
pub struct SectionRenderer<'a> {
    doc: &'a DocumentRenderer<'a>,
    br: BlockRenderer<'a>,
}

impl<'a> SectionRenderer<'a> {
    /// 新しいセクションレンダラを作成
    pub fn new(doc: &'a DocumentRenderer<'a>, options: &'a RenderOptions) -> Self {
        Self {
            doc,
            br: BlockRenderer::new(options),
        }
    }

    /// セクション1つを `out` の末尾に描画する。
    pub fn render(&mut self, out: &mut String, section: Section, all_lines: &[&str]) {
        let lines = section.lines(all_lines);
        if lines.is_empty() && section.skip_when_empty() {
            return;
        }

        self.br.set_tail(section.is_tail());
        section.render_open(self.doc, out);

        // くの字点は生の行から数える（注記内も拾うため）。
        for line in &lines {
            self.br.scan_kunoji(line);
        }
        let raw = parse_document_raw(&lines);
        let blocks = lower_to_blocks(&raw);
        let body = self.br.render_body(&blocks);
        if section.is_tail() {
            out.push_str(&auto_link(&body));
        } else {
            out.push_str(&body);
        }

        section.render_close(self.doc, out);
    }

    /// 全セクションを通して溜まった「表記について」の材料。
    pub fn notation(&self) -> &NotationState {
        self.br.notation()
    }
}

#[cfg(test)]
mod tests {
    use crate::html::{convert, RenderOptions};

    /// くの字点はどのセクションに書かれていても「表記について」に注記が出る
    /// （セクションを足したときに数え忘れると落ちる回帰テスト）。
    #[test]
    fn test_kunoji_is_counted_in_every_section() {
        let note = "<li>「くの字点」は「／＼」で表しました。</li>";

        let in_body = convert(
            "題\r\n\r\n本文でわざ／＼と使う\r\n",
            &RenderOptions::default(),
        );
        assert!(in_body.contains(note), "本文: {in_body}");

        let in_after_text = convert(
            "題\r\n\r\n本文\r\n［＃本文終わり］\r\nあとがきでわざ／＼と使う\r\n",
            &RenderOptions::default(),
        );
        assert!(
            in_after_text.contains(note),
            "本文終わり後: {in_after_text}"
        );

        let in_biblio = convert(
            "題\r\n\r\n本文\r\n\r\n底本：「甲」乙\r\n入力：わざ／＼\r\n",
            &RenderOptions::default(),
        );
        assert!(in_biblio.contains(note), "底本情報: {in_biblio}");
    }

    /// 自動リンクは tail セクションだけに掛かる（参照 tail_output）。
    #[test]
    fn test_auto_link_applies_only_to_tail_sections() {
        let html = convert(
            "題\r\n\r\n本文に info@aozora.gr.jp と書く\r\n\r\n底本：「甲」乙\r\n連絡は info@aozora.gr.jp\r\n",
            &RenderOptions::default(),
        );
        let (main, tail) = html
            .split_once("bibliographical_information")
            .expect("底本情報セクション");
        assert!(
            !main.contains("mailto:"),
            "本文はリンク化しないこと: {main}"
        );
        assert!(tail.contains("mailto:info@aozora.gr.jp"), "実際: {tail}");
    }

    /// 画像化できない外字の ※ は本文だけに付く（参照 tail_output は出さない）。
    #[test]
    fn test_gaiji_mark_only_in_the_main_text_section() {
        let html = convert(
            "題\r\n\r\n刺※［＃「卓＋戈」、U+39B8］\r\n\r\n底本：「甲」乙\r\n刺※［＃「卓＋戈」、U+39B8］\r\n",
            &RenderOptions::default(),
        );
        let (main, tail) = html
            .split_once("bibliographical_information")
            .expect("底本情報セクション");
        assert!(main.contains("刺※<span class=\"notes\">"), "実際: {main}");
        assert!(tail.contains("刺<span class=\"notes\">"), "実際: {tail}");
    }
}
