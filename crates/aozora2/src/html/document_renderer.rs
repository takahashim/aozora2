//! ドキュメント構造レンダリング
//!
//! HTMLヘッダー、フッター、メタデータセクションなどの
//! ドキュメント構造を生成します。

use aozora_core::document::HeaderInfo;

use super::node_renderer::UnconvertedGaiji;
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
            html_escape(title)
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
            html_escape(dc_title)
        ));
        output.push_str(&format!(
            "\t<meta name=\"DC.Creator\" content=\"{}\" />\r\n",
            html_escape(dc_creator)
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

        if let Some(title) = &header_info.title {
            output.push_str(&format!(
                "<h1 class=\"title\">{}</h1>\r\n",
                html_escape(title)
            ));
        }

        if let Some(original_title) = &header_info.original_title {
            output.push_str(&format!(
                "<h2 class=\"original_title\">{}</h2>\r\n",
                html_escape(original_title)
            ));
        }

        if let Some(subtitle) = &header_info.subtitle {
            output.push_str(&format!(
                "<h2 class=\"subtitle\">{}</h2>\r\n",
                html_escape(subtitle)
            ));
        }

        if let Some(original_subtitle) = &header_info.original_subtitle {
            output.push_str(&format!(
                "<h2 class=\"original_subtitle\">{}</h2>\r\n",
                html_escape(original_subtitle)
            ));
        }

        if let Some(author) = &header_info.author {
            output.push_str(&format!(
                "<h2 class=\"author\">{}</h2>\r\n",
                html_escape(author)
            ));
        }

        if let Some(editor) = &header_info.editor {
            output.push_str(&format!(
                "<h2 class=\"editor\">{}</h2>\r\n",
                html_escape(editor)
            ));
        }

        if let Some(translator) = &header_info.translator {
            output.push_str(&format!(
                "<h2 class=\"translator\">{}</h2>\r\n",
                html_escape(translator)
            ));
        }

        if let Some(henyaku) = &header_info.henyaku {
            output.push_str(&format!(
                "<h2 class=\"editor-translator\">{}</h2>\r\n",
                html_escape(henyaku)
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
        output.push_str("<br />\r\n");
        output.push_str("<br />\r\n");
        output.push_str("</div>\r\n");
    }

    /// 底本情報セクションヘッダーを出力
    pub fn render_bibliographical_header(&self, output: &mut String) {
        output.push_str("<div class=\"bibliographical_information\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
    }

    /// 底本情報セクションフッターを出力
    pub fn render_bibliographical_footer(&self, output: &mut String) {
        output.push_str("<br />\r\n");
        output.push_str("<br />\r\n");
        output.push_str("</div>\r\n");
    }

    /// 表記についてセクションを出力
    pub fn render_notation_notes(
        &self,
        output: &mut String,
        has_notes: bool,
        has_jisx0213: bool,
        has_accent: bool,
        unconverted_gaiji: &[UnconvertedGaiji],
    ) {
        output.push_str("<div class=\"notation_notes\">\r\n");
        output.push_str("<hr />\r\n");
        output.push_str("<br />\r\n");
        output.push_str("●表記について<br />\r\n");
        output.push_str("<ul>\r\n");

        // XHTML1.1準拠
        output.push_str(
            "\t<li>このファイルは W3C 勧告 XHTML1.1 にそった形式で作成されています。</li>\r\n",
        );

        // 注記を使用した場合
        if has_notes {
            output.push_str("\t<li>［＃…］は、入力者による注を表す記号です。</li>\r\n");
        }

        // JIS X 0213文字を画像化した場合
        if has_jisx0213 && !self.options.use_jisx0213 {
            output.push_str("\t<li>「くの字点」をのぞくJIS X 0213にある文字は、画像化して埋め込みました。</li>\r\n");
        }

        // アクセント符号を使用した場合
        if has_accent && !self.options.use_jisx0213 {
            output.push_str(
                "\t<li>アクセント符号付きラテン文字は、画像化して埋め込みました。</li>\r\n",
            );
        }

        // 未変換外字がある場合
        if !unconverted_gaiji.is_empty() {
            output.push_str("\t<li>この作品には、JIS X 0213にない、以下の文字が用いられています。（数字は、底本中の出現「ページ-行」数。）これらの文字は本文内では「※［＃…］」の形で示しました。</li>\r\n");
        }

        output.push_str("</ul>\r\n");

        // 外字一覧表を出力
        if !unconverted_gaiji.is_empty() {
            output.push_str("<br />\r\n");
            output.push_str("\t\t<table class=\"gaiji_list\">\r\n");
            for gaiji in unconverted_gaiji {
                output.push_str("\t\t\t<tr>\r\n");

                let gaiji_name = html_escape(&gaiji.gaiji_name);
                let page_line = html_escape(&gaiji.page_line);

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

        output.push_str("</div>\r\n");
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

    /// main_text終了タグを出力
    pub fn render_main_text_end(&self, output: &mut String) {
        output.push_str("</div>\r\n");
    }
}
