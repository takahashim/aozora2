//! ドキュメント構造レンダリング
//!
//! HTMLヘッダー、フッター、メタデータセクションなどの
//! ドキュメント構造を生成します。

use crate::document::HeaderInfo;

use super::notation::NotationState;
use super::options::RenderOptions;
use super::presentation::html_escape;

/// 青空文庫パブリッシャー名
const AOZORA_BUNKO: &str = "青空文庫";

/// ドキュメントレンダラー
pub struct DocumentRenderer<'a> {
    options: &'a RenderOptions,
}

impl<'a> DocumentRenderer<'a> {
    /// 新しいドキュメントレンダラーを作成
    pub fn new(options: &'a RenderOptions) -> Self {
        Self { options }
    }

    /// ヘッダのタイトル・著者等をエスケープする。参照実装は生のまま出すので、
    /// Quirk `raw_header_metadata` がオンのときはエスケープしない。
    fn header_text(&self, s: &str) -> String {
        if self.options.quirks.raw_header_metadata {
            s.to_string()
        } else {
            html_escape(s)
        }
    }

    /// HTMLヘッダーを出力
    pub fn render_html_head(&self, output: &mut String, header_info: &HeaderInfo) {
        // XML宣言とDOCTYPE
        output.push_str("<?xml version=\"1.0\" encoding=\"Shift_JIS\"?>\r\n");
        output.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\"\r\n");
        output.push_str("    \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\r\n");
        output.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"ja\" >\r\n");
        output.push_str("<head>\r\n");

        // メタ情報
        output.push_str(
            "\t<meta http-equiv=\"Content-Type\" content=\"text/html;charset=Shift_JIS\" />\r\n",
        );
        output.push_str("\t<meta http-equiv=\"content-style-type\" content=\"text/css\" />\r\n");

        // CSSリンク
        for css in &self.options.css_files {
            output.push_str(&format!(
                "\t<link rel=\"stylesheet\" type=\"text/css\" href=\"{css}\" />\r\n"
            ));
        }

        // タイトル
        let html_title = if let Some(title) = &self.options.title {
            self.header_text(title)
        } else {
            header_info.html_title()
        };
        output.push_str(&format!("\t<title>{}</title>\r\n", html_title));

        // jQuery
        output.push_str(
            "\t<script type=\"text/javascript\" src=\"../../jquery-1.4.2.min.js\"></script>\r\n",
        );

        // Dublin Core メタデータ
        output
            .push_str("  <link rel=\"Schema.DC\" href=\"http://purl.org/dc/elements/1.1/\" />\r\n");

        let dc_title = header_info.title.as_deref().unwrap_or("");
        let dc_creator = header_info.author.as_deref().unwrap_or("");
        output.push_str(&format!(
            "\t<meta name=\"DC.Title\" content=\"{}\" />\r\n",
            self.header_text(dc_title)
        ));
        output.push_str(&format!(
            "\t<meta name=\"DC.Creator\" content=\"{}\" />\r\n",
            self.header_text(dc_creator)
        ));
        output.push_str(&format!(
            "\t<meta name=\"DC.Publisher\" content=\"{}\" />\r\n",
            AOZORA_BUNKO
        ));

        output.push_str("</head>\r\n");
        output.push_str("<body>\r\n");
    }

    /// メタデータセクションを出力
    pub fn render_metadata_section(&self, output: &mut String, header_info: &HeaderInfo) {
        output.push_str("<div class=\"metadata\">\r\n");

        // 参照 Header#to_html は題名が無くても h1 を出す（`<h1 class="title"></h1>`）。
        // 他の項目（原題・副題・著者など）は out_header_info が有無で分岐するのに対し、
        // 題名だけは常に出る。空行だけの文書で差が出る。
        let title = header_info.title.as_deref().unwrap_or("");
        output.push_str(&format!(
            "<h1 class=\"title\">{}</h1>\r\n",
            self.header_text(title)
        ));

        if let Some(original_title) = &header_info.original_title {
            output.push_str(&format!(
                "<h2 class=\"original_title\">{}</h2>\r\n",
                self.header_text(original_title)
            ));
        }

        if let Some(subtitle) = &header_info.subtitle {
            output.push_str(&format!(
                "<h2 class=\"subtitle\">{}</h2>\r\n",
                self.header_text(subtitle)
            ));
        }

        if let Some(original_subtitle) = &header_info.original_subtitle {
            output.push_str(&format!(
                "<h2 class=\"original_subtitle\">{}</h2>\r\n",
                self.header_text(original_subtitle)
            ));
        }

        if let Some(author) = &header_info.author {
            output.push_str(&format!(
                "<h2 class=\"author\">{}</h2>\r\n",
                self.header_text(author)
            ));
        }

        if let Some(editor) = &header_info.editor {
            output.push_str(&format!(
                "<h2 class=\"editor\">{}</h2>\r\n",
                self.header_text(editor)
            ));
        }

        if let Some(translator) = &header_info.translator {
            output.push_str(&format!(
                "<h2 class=\"translator\">{}</h2>\r\n",
                self.header_text(translator)
            ));
        }

        if let Some(henyaku) = &header_info.henyaku {
            output.push_str(&format!(
                "<h2 class=\"editor-translator\">{}</h2>\r\n",
                self.header_text(henyaku)
            ));
        }

        output.push_str("<br />\r\n<br />\r\n</div>\r\n");
    }

    /// HTMLフッターを出力
    pub fn render_html_foot(&self, output: &mut String) {
        output.push_str("</body>\r\n");
        output.push_str("</html>\r\n");
    }

    /// 本文終わり後のテキスト（after_text）セクションヘッダーを出力
    pub fn render_after_text_header(&self, output: &mut String) {
        output.push_str("<div class=\"after_text\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
    }

    /// 本文終わり後のテキスト（after_text）セクションフッターを出力
    pub fn render_after_text_footer(&self, output: &mut String) {
        self.render_document_tail(output);
    }

    /// 底本情報セクションヘッダーを出力
    pub fn render_bibliographical_header(&self, output: &mut String) {
        output.push_str("<div class=\"bibliographical_information\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
    }

    /// 底本情報セクションフッターを出力
    pub fn render_bibliographical_footer(&self, output: &mut String) {
        self.render_document_tail(output);
    }

    /// 表記についてセクションを出力
    pub fn render_notation_notes(&self, output: &mut String, notation: &NotationState) {
        output.push_str("<div class=\"notation_notes\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
        output.push_str("●表記について<br />\r\n");
        self.render_notation_list(output, notation);
        self.render_gaiji_table(output, notation);
        output.push_str("</div>\r\n");
    }

    /// 「表記について」の箇条書きを出力。どの項目を出すかは使用状況
    /// （[`NotationState`]）とオプションで決まる。
    fn render_notation_list(&self, output: &mut String, notation: &NotationState) {
        output.push_str("<ul>\r\n");

        // XHTML1.1準拠
        output.push_str(
            "\t<li>このファイルは W3C 勧告 XHTML1.1 にそった形式で作成されています。</li>\r\n",
        );

        // 注記を使用した場合
        if notation.has_notes() {
            output.push_str("\t<li>［＃…］は、入力者による注を表す記号です。</li>\r\n");
        }

        // くの字点を使用した場合
        if notation.has_kunoji() {
            if notation.has_dakuten_kunoji() {
                output.push_str(
                    "\t<li>「くの字点」は「／＼」で、「濁点付きくの字点」は「／″＼」で表しました。</li>\r\n",
                );
            } else {
                output.push_str("\t<li>「くの字点」は「／＼」で表しました。</li>\r\n");
            }
        } else if notation.has_dakuten_kunoji() {
            output.push_str("\t<li>「濁点付きくの字点」は「／″＼」で表しました。</li>\r\n");
        }

        // JIS X 0213文字を画像化した場合
        if notation.has_jisx0213() && !self.options.use_jisx0213 {
            output.push_str("\t<li>「くの字点」をのぞくJIS X 0213にある文字は、画像化して埋め込みました。</li>\r\n");
        }

        // アクセント符号を使用した場合
        if notation.has_accent() && !self.options.use_jisx0213 {
            output.push_str(
                "\t<li>アクセント符号付きラテン文字は、画像化して埋め込みました。</li>\r\n",
            );
        }

        // 未変換外字がある場合
        if !notation.unconverted_gaiji().is_empty() {
            output.push_str("\t<li>この作品には、JIS X 0213にない、以下の文字が用いられています。（数字は、底本中の出現「ページ-行」数。）これらの文字は本文内では「※［＃…］」の形で示しました。</li>\r\n");
        }

        output.push_str("</ul>\r\n");
    }

    /// 画像化できない外字の一覧表を出力（該当が無ければ何も出さない）。
    fn render_gaiji_table(&self, output: &mut String, notation: &NotationState) {
        let unconverted_gaiji = notation.unconverted_gaiji();
        if unconverted_gaiji.is_empty() {
            return;
        }

        output.push_str("<br />\r\n");
        output.push_str("\t\t<table class=\"gaiji_list\">\r\n");
        for gaiji in unconverted_gaiji {
            output.push_str("\t\t\t<tr>\r\n");

            let gaiji_name = html_escape(&gaiji.gaiji_name);
            let page_line = html_escape(&gaiji.page_lines.join("、"));

            output.push_str(&format!(
                "\t\t\t\t<td>\r\n\t\t\t\t{}\r\n\t\t\t\t</td>\r\n",
                gaiji_name
            ));
            output.push_str("\t\t\t\t<td>&nbsp;&nbsp;</td>\r\n");
            output.push_str(&format!("\t\t\t\t<td>\r\n{}\t\t\t\t</td>\r\n", page_line));
            // コメント出力
            output.push_str(&format!(
                "\t\t\t\t<!--\r\n\t\t\t\t<td>\r\n\t\t\t\t　　<img src=\"../../../gaiji/others/xxxx.png\" alt=\"{}\" width=32 height=32 />\r\n\t\t\t\t</td>\r\n\t\t\t\t-->\r\n",
                gaiji_name
            ));
            output.push_str("\t\t\t</tr>\r\n");
        }
        output.push_str("\t\t</table>\r\n");
    }

    /// 図書カードセクションを出力
    pub fn render_card_section(&self, output: &mut String) {
        output.push_str("<div id=\"card\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
        output.push_str("<a href=\"JavaScript:goLibCard();\" id=\"goAZLibCard\">●図書カード</a>");
        output.push_str("<script type=\"text/javascript\" src=\"../../contents.js\"></script>\r\n");
        output
            .push_str("<script type=\"text/javascript\" src=\"../../golibcard.js\"></script>\r\n");
        output.push_str("</div>");
    }

    /// main_text開始タグを出力
    pub fn render_main_text_start(&self, output: &mut String) {
        output.push_str(
            "<div id=\"contents\" style=\"display:none\"></div><div class=\"main_text\">",
        );
    }

    /// 文書末尾の `<br />` と閉じ `</div>`。
    ///
    /// 参照は `process` の最後に `hyoki` を呼び、その先頭が `<br />` なので、
    /// **最後に描かれたセクション**の閉じ `</div>` の前に `<br />` が 1 つ入る。
    ///
    /// もう 1 つの `<br />`（`process` 末尾の `tail_output` が空バッファで出すもの）は
    /// ここでは出さない。入力が改行で終わっていればその改行が空行を 1 行作り、
    /// 内容の行として `<br />` になる（＝行として数えられる）。改行で終わっていなければ
    /// 最後の `tail_output` が最終行の内容を流すので、その `<br />` は内容の行の分になる。
    /// どちらも「行」で説明がつくので、行の外に旗を持つ必要はない。
    fn render_document_tail(&self, output: &mut String) {
        output.push_str("<br />\r\n");
        output.push_str("</div>\r\n");
    }

    /// main_text終了タグを出力
    ///
    /// 底本行も `［＃本文終わり］` も無い文書では main_text が最後のセクションになる。
    pub fn render_main_text_end(&self, output: &mut String, is_last: bool) {
        if is_last {
            self.render_document_tail(output);
            return;
        }
        output.push_str("</div>\r\n");
    }
}
