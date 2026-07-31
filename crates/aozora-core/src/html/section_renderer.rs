//! 文書セクション（本文／本文終わり後／底本情報）の描画。
//!
//! 参照実装は本文を `general_output`、tail（本文終わり後・底本情報）を
//! `tail_output` で出す。どちらも「行を読む→描画する」同じ手順で、違うのは
//! 枠の HTML・自動リンクの有無・外字記号 ※ を出すかだけ。ここはその違いを
//! [`Section`] に持たせ、手順は1つに保つ器。

use crate::ast::{Block, Inline};

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
        /// 参照は `process` の最後に `hyoki` を呼び、その先頭が `<br />` なので、
        /// **最後に描かれたセクション**の閉じ `</div>` の前に `<br />` が 1 つ入る。
        /// 底本行も `［＃本文終わり］` も無い文書では、それが main_text になる。
        is_last: bool,
    },
    /// 本文終わり後（after_text）
    AfterText,
    /// 底本情報（bibliographical_information）
    Bibliographical,
}

impl Section {
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
            Section::Bibliographical => doc.render_bibliographical_header(out),
        }
    }

    fn render_close(&self, doc: &DocumentRenderer, out: &mut String) {
        match self {
            Section::MainText { is_last } => doc.render_main_text_end(out, *is_last),
            Section::AfterText => doc.render_after_text_footer(out),
            Section::Bibliographical => doc.render_bibliographical_footer(out),
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

    /// 畳み終えた [`Block`] 列からセクション1つを描画する。
    ///
    /// 交換形式の JSON から読み戻した木も、テキストから畳んだ木も、同じここを通る。
    pub fn render_ast(&mut self, out: &mut String, section: Section, blocks: &[Block]) {
        if blocks.is_empty() && section.skip_when_empty() {
            return;
        }

        self.br.set_tail(section.is_tail());
        section.render_open(self.doc, out);

        let body = self.br.render_body(blocks);
        if section.is_tail() {
            out.push_str(&auto_link(&body));
        } else {
            out.push_str(&body);
        }

        section.render_close(self.doc, out);
    }

    /// 木の中の文字列からくの字点を数える（AST から描くとき用）。
    ///
    /// 参照実装は生のソース行を走査する。注記の中に書かれていても拾うためだが、
    /// 木は注記の原文（`Note.raw`）まで保つので、木の中の文字列をすべて見れば
    /// 同じ結果になる（全コーパスで一致を確認済み）。
    pub fn scan_kunoji_in(&mut self, blocks: &[Block]) {
        for block in blocks {
            match block {
                Block::Line { inline, .. } | Block::LineWrap { inline, .. } => {
                    self.scan_kunoji_inlines(inline)
                }
                Block::Nested { children, .. } => self.scan_kunoji_in(children),
            }
        }
    }

    fn scan_kunoji_inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            for text in inline.kind.texts() {
                self.br.scan_kunoji(text);
            }
            for children in inline.kind.child_lists() {
                self.scan_kunoji_inlines(children);
            }
        }
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
