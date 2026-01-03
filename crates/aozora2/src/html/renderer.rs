//! HTMLレンダラー
//!
//! ASTノードをHTMLに変換します。

use aozora_core::gaiji::{parse_gaiji, GaijiResult};
use aozora_core::node::{
    BlockParams, BlockType, MidashiLevel, MidashiStyle, Node, RubyDirection, StyleType,
};
use aozora_core::parser::parse;
use aozora_core::parser::reference_resolver::resolve_inline_ruby;
use aozora_core::tokenizer::tokenize;

use super::options::RenderOptions;

// ============================================================================
// プレゼンテーションロジック（CSSクラス、HTMLタグのマッピング）
// ============================================================================

/// StyleType のCSSクラス名を取得
fn style_css_class(style_type: StyleType) -> &'static str {
    match style_type {
        StyleType::SesameDot => "sesame_dot",
        StyleType::WhiteSesameDot => "white_sesame_dot",
        StyleType::BlackCircle => "black_circle",
        StyleType::WhiteCircle => "white_circle",
        StyleType::BlackTriangle => "black_up-pointing_triangle",
        StyleType::WhiteTriangle => "white_up-pointing_triangle",
        StyleType::Bullseye => "bullseye",
        StyleType::Fisheye => "fisheye",
        StyleType::Saltire => "saltire",
        StyleType::UnderlineSolid => "underline_solid",
        StyleType::UnderlineDouble => "underline_double",
        StyleType::UnderlineDotted => "underline_dotted",
        StyleType::UnderlineDashed => "underline_dashed",
        StyleType::UnderlineWave => "underline_wave",
        StyleType::Bold => "futoji",
        StyleType::Italic => "shatai",
        StyleType::Subscript => "subscript",
        StyleType::Superscript => "superscript",
    }
}

/// StyleType のHTMLタグ名を取得
fn style_html_tag(style_type: StyleType) -> &'static str {
    match style_type {
        StyleType::Subscript => "sub",
        StyleType::Superscript => "sup",
        StyleType::Bold | StyleType::Italic => "span",
        _ => "em",
    }
}

/// MidashiLevel のCSSクラス名を取得
fn midashi_css_class(level: MidashiLevel) -> &'static str {
    match level {
        MidashiLevel::O => "o-midashi",
        MidashiLevel::Naka => "naka-midashi",
        MidashiLevel::Ko => "ko-midashi",
    }
}

/// MidashiLevel のHTMLタグ名を取得
fn midashi_html_tag(level: MidashiLevel) -> &'static str {
    match level {
        MidashiLevel::O => "h3",
        MidashiLevel::Naka => "h4",
        MidashiLevel::Ko => "h5",
    }
}

/// HTMLレンダラー
#[derive(Debug, Clone)]
pub struct HtmlRenderer {
    options: RenderOptions,
    /// 現在のブロックスタック
    block_stack: Vec<BlockContext>,
}

/// ブロックコンテキスト
#[derive(Debug, Clone)]
struct BlockContext {
    block_type: BlockType,
    params: BlockParams,
}

impl HtmlRenderer {
    /// 新しいレンダラーを作成
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            block_stack: Vec::new(),
        }
    }

    /// テキスト全体をHTMLに変換
    pub fn render(&mut self, input: &str) -> String {
        let mut output = String::new();

        if self.options.full_document {
            self.render_html_head(&mut output);
        }

        for line in input.lines() {
            let line_html = self.render_line(line);
            output.push_str(&line_html);
            output.push('\n');
        }

        // 閉じられていないブロックを閉じる
        while let Some(ctx) = self.block_stack.pop() {
            output.push_str(&self.render_block_end_tag(&ctx.block_type, &ctx.params));
        }

        if self.options.full_document {
            self.render_html_foot(&mut output);
        }

        output
    }

    /// 1行をHTMLに変換
    pub fn render_line(&mut self, line: &str) -> String {
        let tokens = tokenize(line);
        let mut nodes = parse(&tokens);

        // 行内ルビを解決
        resolve_inline_ruby(&mut nodes);

        self.render_nodes(&nodes)
    }

    /// ノード列をHTMLに変換
    pub fn render_nodes(&mut self, nodes: &[Node]) -> String {
        let mut output = String::new();

        for node in nodes {
            output.push_str(&self.render_node(node));
        }

        output
    }

    /// 単一ノードをHTMLに変換
    fn render_node(&mut self, node: &Node) -> String {
        match node {
            Node::Text(text) => html_escape(text),

            Node::Ruby {
                children,
                ruby,
                direction,
            } => self.render_ruby(children, ruby, *direction),

            Node::Style {
                children,
                style_type,
                class_name: _,
            } => self.render_style(children, *style_type),

            Node::Midashi {
                children,
                level,
                style,
            } => self.render_midashi(children, *level, *style),

            Node::Gaiji {
                description,
                unicode,
                jis_code,
            } => self.render_gaiji(description, unicode.as_deref(), jis_code.as_deref()),

            Node::Accent {
                code: _,
                name: _,
                unicode,
            } => unicode.clone().unwrap_or_default(),

            Node::Img {
                filename,
                alt,
                css_class,
                width,
                height,
            } => self.render_img(filename, alt, css_class, *width, *height),

            Node::Tcy { children } => {
                let inner = self.render_nodes(children);
                format!("<span class=\"tcy\">{inner}</span>")
            }

            Node::Keigakomi { children } => {
                let inner = self.render_nodes(children);
                format!("<span class=\"keigakomi\">{inner}</span>")
            }

            Node::Caption { children } => {
                let inner = self.render_nodes(children);
                format!("<span class=\"caption\">{inner}</span>")
            }

            Node::Warigaki { upper, lower } => {
                let upper_html = self.render_nodes(upper);
                let lower_html = self.render_nodes(lower);
                format!(
                    "<span class=\"warichu\"><span class=\"warichu_upper\">{upper_html}</span><span class=\"warichu_lower\">{lower_html}</span></span>"
                )
            }

            Node::Kaeriten(text) => {
                format!("<sub class=\"kaeriten\">{}</sub>", html_escape(text))
            }

            Node::Okurigana(text) => {
                format!("<sup class=\"okurigana\">{}</sup>", html_escape(text))
            }

            Node::BlockStart { block_type, params } => {
                self.block_stack.push(BlockContext {
                    block_type: *block_type,
                    params: params.clone(),
                });
                self.render_block_start_tag(block_type, params)
            }

            Node::BlockEnd { block_type } => {
                // スタックから対応するブロックを探して閉じる
                if let Some(pos) = self
                    .block_stack
                    .iter()
                    .rposition(|c| c.block_type == *block_type)
                {
                    let ctx = self.block_stack.remove(pos);
                    self.render_block_end_tag(block_type, &ctx.params)
                } else {
                    // 対応するブロックがない場合は空文字
                    String::new()
                }
            }

            Node::Note(text) => {
                format!("<span class=\"notes\">［＃{}］</span>", html_escape(text))
            }

            Node::UnresolvedReference {
                target,
                spec,
                connector,
            } => {
                // 解決できなかった参照は注記として出力
                format!(
                    "<span class=\"notes\">［＃「{}」{}{}］</span>",
                    html_escape(target),
                    html_escape(connector),
                    html_escape(spec)
                )
            }

            Node::DakutenKatakana { num } => {
                // 濁点カタカナの出力
                match num.as_str() {
                    "2" => "ワ゛".to_string(),
                    "3" => "ヰ゛".to_string(),
                    "4" => "ヱ゛".to_string(),
                    "5" => "ヲ゛".to_string(),
                    _ => String::new(),
                }
            }
        }
    }

    /// ルビをHTMLに変換
    fn render_ruby(
        &mut self,
        children: &[Node],
        ruby: &[Node],
        direction: RubyDirection,
    ) -> String {
        let base_html = self.render_nodes(children);
        let ruby_html = self.render_nodes(ruby);

        match direction {
            RubyDirection::Right => {
                format!(
                    "<ruby><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>"
                )
            }
            RubyDirection::Left => {
                // 左ルビ（縦書き用）
                format!(
                    "<ruby class=\"leftrb\"><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>"
                )
            }
        }
    }

    /// 装飾をHTMLに変換
    fn render_style(&mut self, children: &[Node], style_type: StyleType) -> String {
        let inner = self.render_nodes(children);
        let tag = style_html_tag(style_type);
        let class = style_css_class(style_type);

        format!("<{tag} class=\"{class}\">{inner}</{tag}>")
    }

    /// 見出しをHTMLに変換
    fn render_midashi(
        &mut self,
        children: &[Node],
        level: MidashiLevel,
        style: MidashiStyle,
    ) -> String {
        let inner = self.render_nodes(children);
        let tag = midashi_html_tag(level);
        let class = midashi_css_class(level);

        match style {
            MidashiStyle::Normal => {
                format!("<{tag} class=\"{class}\">{inner}</{tag}>")
            }
            MidashiStyle::Dogyo => {
                // 同行見出し
                format!(
                    "<{} class=\"{} dogyo-midashi\"><a class=\"midashi_anchor\" id=\"midashi{}\"></a>{}</{}>",
                    tag, class, self.generate_midashi_id(), inner, tag
                )
            }
            MidashiStyle::Mado => {
                // 窓見出し
                format!("<{tag} class=\"{class} mado-midashi\">{inner}</{tag}>")
            }
        }
    }

    /// 見出しIDを生成（簡易版）
    fn generate_midashi_id(&self) -> u32 {
        // 実際の実装では一意なIDを生成する
        0
    }

    /// 外字をHTMLに変換
    fn render_gaiji(
        &self,
        description: &str,
        unicode: Option<&str>,
        jis_code: Option<&str>,
    ) -> String {
        // すでにパース済みの情報がある場合はそれを使用
        if let Some(u) = unicode {
            if self.options.use_unicode {
                return u.chars().map(|c| format!("&#{};", c as u32)).collect();
            }
            return u.to_string();
        }

        if let Some(jis) = jis_code {
            // JISコードから画像を生成
            let (folder, file) = jis_code_to_path(jis);
            return format!(
                "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                self.options.gaiji_dir,
                folder,
                file,
                html_escape(description)
            );
        }

        // パース済み情報がない場合は再度パース
        match parse_gaiji(description) {
            GaijiResult::Unicode(s) => {
                if self.options.use_unicode {
                    s.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    s
                }
            }
            GaijiResult::JisConverted {
                jis_code: _,
                unicode: u,
            } => {
                if self.options.use_jisx0213 || self.options.use_unicode {
                    u.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    u
                }
            }
            GaijiResult::JisImage { jis_code: jis } => {
                let (folder, file) = jis_code_to_path(&jis);
                format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir,
                    folder,
                    file,
                    html_escape(description)
                )
            }
            GaijiResult::Unconvertible => {
                format!(
                    "<span class=\"notes\">※［＃{}］</span>",
                    html_escape(description)
                )
            }
        }
    }

    /// 画像をHTMLに変換
    fn render_img(
        &self,
        filename: &str,
        alt: &str,
        css_class: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> String {
        let mut attrs = format!(
            "src=\"{}{}\" alt=\"{}\"",
            self.options.gaiji_dir,
            filename,
            html_escape(alt)
        );

        if !css_class.is_empty() {
            attrs.push_str(&format!(" class=\"{css_class}\""));
        }

        if let Some(w) = width {
            attrs.push_str(&format!(" width=\"{w}\""));
        }

        if let Some(h) = height {
            attrs.push_str(&format!(" height=\"{h}\""));
        }

        format!("<img {attrs} />")
    }

    /// ブロック開始タグを生成
    fn render_block_start_tag(&self, block_type: &BlockType, params: &BlockParams) -> String {
        match block_type {
            BlockType::Jisage => {
                if let Some(width) = params.width {
                    format!("<div class=\"jisage_{width}\">")
                } else {
                    "<div class=\"jisage\">".to_string()
                }
            }
            BlockType::Chitsuki => {
                if let Some(width) = params.width {
                    format!("<div class=\"chitsuki_{width}\">")
                } else {
                    "<div class=\"chitsuki\">".to_string()
                }
            }
            BlockType::Jizume => {
                if let Some(width) = params.width {
                    format!("<div class=\"jizume_{width}\">")
                } else {
                    "<div class=\"jizume\">".to_string()
                }
            }
            BlockType::Keigakomi => "<div class=\"keigakomi\">".to_string(),
            BlockType::Midashi => {
                if let Some(level) = params.level {
                    format!(
                        "<{} class=\"{}\">",
                        midashi_html_tag(level),
                        midashi_css_class(level)
                    )
                } else {
                    "<h3 class=\"o-midashi\">".to_string()
                }
            }
            BlockType::Yokogumi => "<div class=\"yokogumi\">".to_string(),
            BlockType::Futoji => "<div class=\"futoji\">".to_string(),
            BlockType::Shatai => "<div class=\"shatai\">".to_string(),
            BlockType::FontDai => {
                if let Some(size) = params.font_size {
                    format!("<span class=\"dai{size}\">")
                } else {
                    "<span class=\"dai\">".to_string()
                }
            }
            BlockType::FontSho => {
                if let Some(size) = params.font_size {
                    format!("<span class=\"sho{size}\">")
                } else {
                    "<span class=\"sho\">".to_string()
                }
            }
            BlockType::Tcy => "<span class=\"tcy\">".to_string(),
            BlockType::Caption => "<span class=\"caption\">".to_string(),
            BlockType::Warigaki => "<span class=\"warichu\">".to_string(),
        }
    }

    /// ブロック終了タグを生成
    fn render_block_end_tag(&self, block_type: &BlockType, params: &BlockParams) -> String {
        match block_type {
            BlockType::Jisage
            | BlockType::Chitsuki
            | BlockType::Jizume
            | BlockType::Keigakomi
            | BlockType::Yokogumi
            | BlockType::Futoji
            | BlockType::Shatai => "</div>".to_string(),
            BlockType::Midashi => {
                if let Some(level) = params.level {
                    format!("</{}>", midashi_html_tag(level))
                } else {
                    "</h3>".to_string()
                }
            }
            BlockType::FontDai
            | BlockType::FontSho
            | BlockType::Tcy
            | BlockType::Caption
            | BlockType::Warigaki => "</span>".to_string(),
        }
    }

    /// HTMLヘッダーを出力
    fn render_html_head(&self, output: &mut String) {
        output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        output.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n");
        output.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"ja\">\n");
        output.push_str("<head>\n");
        output.push_str(
            "  <meta http-equiv=\"Content-Type\" content=\"text/html; charset=UTF-8\" />\n",
        );
        output.push_str("  <meta http-equiv=\"Content-Style-Type\" content=\"text/css\" />\n");

        if let Some(title) = &self.options.title {
            output.push_str(&format!("  <title>{}</title>\n", html_escape(title)));
        } else {
            output.push_str("  <title></title>\n");
        }

        for css in &self.options.css_files {
            output.push_str(&format!(
                "  <link rel=\"stylesheet\" type=\"text/css\" href=\"{css}\" />\n"
            ));
        }

        output.push_str("</head>\n");
        output.push_str("<body>\n");
    }

    /// HTMLフッターを出力
    fn render_html_foot(&self, output: &mut String) {
        output.push_str("</body>\n");
        output.push_str("</html>\n");
    }
}

/// HTMLエスケープ
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// JISコードをファイルパスに変換
fn jis_code_to_path(jis_code: &str) -> (String, String) {
    // "1-02-22" → ("1-02", "1-02-22")
    let parts: Vec<&str> = jis_code.split('-').collect();
    if parts.len() == 3 {
        let folder = format!("{}-{}", parts[0], parts[1]);
        (folder, jis_code.to_string())
    } else {
        ("".to_string(), jis_code.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_text() {
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render_line("こんにちは");
        assert_eq!(html, "こんにちは");
    }

    #[test]
    fn test_render_ruby() {
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render_line("漢字《かんじ》");
        assert!(html.contains("<ruby>"));
        assert!(html.contains("<rb>漢字</rb>"));
        assert!(html.contains("<rt>かんじ</rt>"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<test>"), "&lt;test&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn test_jis_code_to_path() {
        let (folder, file) = jis_code_to_path("1-02-22");
        assert_eq!(folder, "1-02");
        assert_eq!(file, "1-02-22");
    }
}
