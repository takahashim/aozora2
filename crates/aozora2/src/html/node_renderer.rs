//! ノードレンダラー
//!
//! ASTノードをHTMLに変換します。

use aozora_core::gaiji::{parse_gaiji, GaijiResult};
use aozora_core::node::{
    BlockType, FontSizeType, MidashiLevel, MidashiStyle, Node, RubyDirection, StyleType,
};

use super::block_manager::BlockManager;
use super::options::RenderOptions;
use super::presentation::{
    html_escape, jis_code_to_path, midashi_combined_css_class, midashi_html_tag, style_css_class,
    style_html_tag,
};

/// 未変換外字情報
#[derive(Debug, Clone)]
pub struct UnconvertedGaiji {
    /// 外字名（説明の最後の「、」より前の部分）
    pub gaiji_name: String,
    /// ページ-行数（説明の最後の「、」より後の部分）
    pub page_line: String,
}

/// ノードレンダラー
pub struct NodeRenderer<'a> {
    options: &'a RenderOptions,
    /// 注記を使用したかどうか
    pub has_notes: bool,
    /// 外字画像を使用したかどうか
    pub has_gaiji_images: bool,
    /// アクセント記号を使用したかどうか
    pub has_accent: bool,
    /// JIS X 0213文字を使用したかどうか
    pub has_jisx0213: bool,
    /// 未変換外字のリスト
    pub unconverted_gaiji: Vec<UnconvertedGaiji>,
}

impl<'a> NodeRenderer<'a> {
    /// 新しいノードレンダラーを作成
    pub fn new(options: &'a RenderOptions) -> Self {
        Self {
            options,
            has_notes: false,
            has_gaiji_images: false,
            has_accent: false,
            has_jisx0213: false,
            unconverted_gaiji: Vec::new(),
        }
    }

    /// ノード列をHTMLに変換
    pub fn render_nodes(&mut self, nodes: &[Node], block_manager: &mut BlockManager) -> String {
        let mut output = String::new();
        for node in nodes {
            output.push_str(&self.render_node(node, block_manager));
        }
        output
    }

    /// 単一ノードをHTMLに変換
    pub fn render_node(&mut self, node: &Node, block_manager: &mut BlockManager) -> String {
        match node {
            Node::Text(text) => html_escape(text),

            Node::Ruby {
                children,
                ruby,
                direction,
            } => self.render_ruby(children, ruby, *direction, block_manager),

            Node::Style {
                children,
                style_type,
                class_name: _,
            } => self.render_style(children, *style_type, block_manager),

            Node::Midashi {
                children,
                level,
                style,
            } => self.render_midashi(children, *level, *style, block_manager),

            Node::Gaiji {
                description,
                unicode,
                jis_code,
            } => self.render_gaiji(description, unicode.as_deref(), jis_code.as_deref()),

            Node::Accent {
                code,
                name,
                unicode,
            } => {
                self.has_accent = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    if let Some(u) = unicode {
                        u.chars().map(|c| format!("&#{};", c as u32)).collect()
                    } else {
                        String::new()
                    }
                } else {
                    self.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(code);
                    format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir,
                        folder,
                        file,
                        html_escape(name)
                    )
                }
            }

            Node::Img {
                filename,
                alt,
                css_class,
                width,
                height,
            } => self.render_img(filename, alt, css_class, *width, *height),

            Node::Tcy { children } => {
                let inner = self.render_nodes(children, block_manager);
                format!("<span dir=\"ltr\">{inner}</span>")
            }

            Node::Keigakomi { children } => {
                let inner = self.render_nodes(children, block_manager);
                format!("<span class=\"keigakomi\">{inner}</span>")
            }

            Node::Yokogumi { children } => {
                let inner = self.render_nodes(children, block_manager);
                format!("<span class=\"yokogumi\">{inner}</span>")
            }

            Node::Caption { children } => {
                let inner = self.render_nodes(children, block_manager);
                format!("<span class=\"caption\">{inner}</span>")
            }

            Node::Warigaki { upper, lower } => {
                let upper_html = self.render_nodes(upper, block_manager);
                let lower_html = self.render_nodes(lower, block_manager);
                format!(
                    "<span class=\"warichu\"><span class=\"warichu_upper\">{upper_html}</span><span class=\"warichu_lower\">{lower_html}</span></span>"
                )
            }

            Node::FontSize {
                children,
                size_type,
                level,
            } => self.render_font_size(children, *size_type, *level, block_manager),

            Node::Kaeriten(text) => {
                format!("<sub class=\"kaeriten\">{}</sub>", html_escape(text))
            }

            Node::Okurigana(text) => {
                format!("<sup class=\"okurigana\">{}</sup>", html_escape(text))
            }

            Node::BlockStart { block_type, params } => {
                let mut output = String::new();

                // 新しいブロック開始時は、開いている関連ブロックを閉じる
                let closed_blocks = block_manager.close_related_blocks(block_type);
                for (bt, bp) in closed_blocks {
                    output.push_str(&block_manager.render_block_end_tag(&bt, &bp));
                }

                block_manager.push(*block_type, params.clone());
                // Burasageは各行で個別にラップするため、開始タグを出力しない
                if *block_type != BlockType::Burasage {
                    output.push_str(&block_manager.render_block_start_tag(block_type, params));
                }
                output
            }

            Node::BlockEnd { block_type, params } => {
                if let Some(ctx) = block_manager.find_and_close(block_type) {
                    // Burasageは各行で個別にラップするため、終了タグを出力しない
                    if ctx.block_type == BlockType::Burasage {
                        String::new()
                    } else if ctx.block_type == BlockType::Warigaki
                        || ctx.block_type == BlockType::Style
                    {
                        // 割り注/装飾の場合はBlockEndのparamsを使用
                        block_manager.render_block_end_tag(&ctx.block_type, params)
                    } else {
                        block_manager.render_block_end_tag(&ctx.block_type, &ctx.params)
                    }
                } else {
                    String::new()
                }
            }

            Node::Note(text) => {
                self.has_notes = true;
                format!("<span class=\"notes\">［＃{}］</span>", html_escape(text))
            }

            Node::AnnotationEnd {
                prefix,
                content,
                suffix,
            } => {
                self.has_notes = true;
                let content_html = self.render_nodes(content, block_manager);
                format!(
                    "<span class=\"notes\">［＃{}{}{}］</span>",
                    html_escape(prefix),
                    content_html,
                    html_escape(suffix)
                )
            }

            Node::UnresolvedReference {
                target,
                spec,
                connector,
            } => {
                format!(
                    "<span class=\"notes\">［＃「{}」{}{}］</span>",
                    html_escape(target),
                    html_escape(connector),
                    html_escape(spec)
                )
            }

            Node::DakutenKatakana { num } => match num.as_str() {
                "2" => "ワ゛".to_string(),
                "3" => "ヰ゛".to_string(),
                "4" => "ヱ゛".to_string(),
                "5" => "ヲ゛".to_string(),
                _ => String::new(),
            },
        }
    }

    /// ルビをHTMLに変換
    fn render_ruby(
        &mut self,
        children: &[Node],
        ruby: &[Node],
        direction: RubyDirection,
        block_manager: &mut BlockManager,
    ) -> String {
        let base_html = self.render_nodes(children, block_manager);
        let ruby_html = self.render_nodes(ruby, block_manager);
        // Unicode nbsp (\u{00a0}) を HTML entity &nbsp; に変換
        let ruby_html = ruby_html.replace('\u{00a0}', "&nbsp;");

        match direction {
            RubyDirection::Right => {
                format!(
                    "<ruby><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>"
                )
            }
            RubyDirection::Left => {
                format!(
                    "<ruby class=\"leftrb\"><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>"
                )
            }
        }
    }

    /// 装飾をHTMLに変換
    fn render_style(
        &mut self,
        children: &[Node],
        style_type: StyleType,
        block_manager: &mut BlockManager,
    ) -> String {
        let inner = self.render_nodes(children, block_manager);
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
        block_manager: &mut BlockManager,
    ) -> String {
        let inner = self.render_nodes(children, block_manager);
        let tag = midashi_html_tag(level);
        let class = midashi_combined_css_class(level, style);
        let midashi_id = block_manager.generate_midashi_id(level);

        format!(
            "<{tag} class=\"{class}\"><a class=\"midashi_anchor\" id=\"midashi{midashi_id}\">{inner}</a></{tag}>"
        )
    }

    /// フォントサイズをHTMLに変換
    fn render_font_size(
        &mut self,
        children: &[Node],
        size_type: FontSizeType,
        level: u32,
        block_manager: &mut BlockManager,
    ) -> String {
        let inner = self.render_nodes(children, block_manager);
        let (class, style) = match size_type {
            FontSizeType::Dai => {
                let size_style = match level {
                    1 => "large",
                    2 => "x-large",
                    _ => "xx-large",
                };
                (format!("dai{level}"), format!("font-size: {size_style};"))
            }
            FontSizeType::Sho => {
                let size_style = match level {
                    1 => "small",
                    2 => "x-small",
                    _ => "xx-small",
                };
                (format!("sho{level}"), format!("font-size: {size_style};"))
            }
        };
        format!("<span class=\"{class}\" style=\"{style}\">{inner}</span>")
    }

    /// 外字をHTMLに変換
    fn render_gaiji(
        &mut self,
        description: &str,
        unicode: Option<&str>,
        jis_code: Option<&str>,
    ) -> String {
        match (unicode, jis_code) {
            // JisConverted: unicodeとjis_code両方がある場合
            (Some(u), Some(jis)) => {
                self.has_jisx0213 = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                } else {
                    self.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(jis);
                    return format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir,
                        folder,
                        file,
                        html_escape(description)
                    );
                }
            }
            // Unicode: unicodeだけがある場合（JISコードがない）
            (Some(u), None) => {
                if self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                }
                // JISコードがないので画像化できない → 注記として出力
                self.has_notes = true;
                self.add_unconverted_gaiji(description);
                return format!(
                    "※<span class=\"notes\">［＃{}］</span>",
                    html_escape(description)
                );
            }
            // JisImage: jis_codeだけがある場合
            (None, Some(jis)) => {
                self.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(jis);
                return format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir,
                    folder,
                    file,
                    html_escape(description)
                );
            }
            // 両方Noneの場合は再度パース
            (None, None) => {}
        }

        match parse_gaiji(description) {
            GaijiResult::Unicode(s) => {
                if self.options.use_unicode {
                    s.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    self.has_notes = true;
                    self.add_unconverted_gaiji(description);
                    format!(
                        "※<span class=\"notes\">［＃{}］</span>",
                        html_escape(description)
                    )
                }
            }
            GaijiResult::JisConverted {
                jis_code: jis,
                unicode: u,
            } => {
                self.has_jisx0213 = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    u.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    self.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(&jis);
                    format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir,
                        folder,
                        file,
                        html_escape(description)
                    )
                }
            }
            GaijiResult::JisImage { jis_code: jis } => {
                self.has_gaiji_images = true;
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
                self.has_notes = true;
                self.add_unconverted_gaiji(description);
                format!(
                    "※<span class=\"notes\">［＃{}］</span>",
                    html_escape(description)
                )
            }
        }
    }

    /// 未変換外字を追加（重複を避ける）
    fn add_unconverted_gaiji(&mut self, description: &str) {
        // descriptionを最後の「、」で分解（外字説明とページ-行数を分離）
        let (gaiji_name, page_line) = if let Some(last_comma_pos) = description.rfind('、') {
            let name = &description[..last_comma_pos];
            let line = &description[last_comma_pos + '、'.len_utf8()..];
            (name.to_string(), line.to_string())
        } else {
            (description.to_string(), String::new())
        };

        // 既に追加済みの場合はスキップ
        if self
            .unconverted_gaiji
            .iter()
            .any(|g| g.gaiji_name == gaiji_name)
        {
            return;
        }
        self.unconverted_gaiji.push(UnconvertedGaiji {
            gaiji_name,
            page_line,
        });
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
        let class = if css_class.is_empty() {
            "illustration"
        } else {
            css_class
        };

        let mut attrs = format!("class=\"{class}\"");

        if let Some(w) = width {
            attrs.push_str(&format!(" width=\"{w}\""));
        }

        if let Some(h) = height {
            attrs.push_str(&format!(" height=\"{h}\""));
        }

        attrs.push_str(&format!(
            " src=\"{}\" alt=\"{}\"",
            filename,
            html_escape(alt)
        ));

        format!("<img {attrs} />")
    }
}
