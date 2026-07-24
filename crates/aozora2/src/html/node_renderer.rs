//! ノードレンダラー
//!
//! ASTノードをHTMLに変換します。

use aozora_core::gaiji::{parse_gaiji, GaijiResult};
use aozora_core::node::{
    BlockType, FontSizeType, MidashiLevel, MidashiStyle, Node, RubyDirection, StyleType,
};
use aozora_core::parser::parse;
use aozora_core::parser::reference_resolver::resolve_inline_ruby;
use aozora_core::tokenizer::tokenize;

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
    /// ページ-行数（説明の最後の「、」より後の部分）。
    /// 同じ外字が複数回現れた場合は出現箇所を順に並べる。
    pub page_lines: Vec<String>,
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
    /// くの字点（／＼）を使用したかどうか
    pub has_kunoji: bool,
    /// 濁点付きくの字点（／″＼）を使用したかどうか
    pub has_dakuten_kunoji: bool,
    /// 未変換外字のリスト
    pub unconverted_gaiji: Vec<UnconvertedGaiji>,
    /// 注記の中身をパースし直す入れ子の深さ
    note_depth: usize,
    /// 本文（main_text）を抜けたあとのセクションを処理中かどうか。
    /// 参照実装の tail_output は general_output と違い外字記号を出さない。
    in_tail: bool,
    /// ルビの親文字を組み立て中かどうか。親文字の中では外字記号を
    /// 個別に出さず、親文字側にまとめて置く。
    in_ruby_base: bool,
    /// alt の入れ子外字展開の深さ（暴走防止）
    alt_depth: usize,
}

/// 画像化できない外字を本文中で示す記号
const GAIJI_MARK: &str = "※";
/// くの字点の1文字目
const KUNOJI_KU: char = '／';
/// くの字点の2文字目
const KUNOJI_NOJI: char = '＼';
/// 濁点付きくの字点の濁点
const KUNOJI_DAKUTEN: char = '″';

impl<'a> NodeRenderer<'a> {
    /// 新しいノードレンダラーを作成
    pub fn new(options: &'a RenderOptions) -> Self {
        Self {
            options,
            has_notes: false,
            has_gaiji_images: false,
            has_accent: false,
            has_jisx0213: false,
            has_kunoji: false,
            has_dakuten_kunoji: false,
            unconverted_gaiji: Vec::new(),
            note_depth: 0,
            in_tail: false,
            in_ruby_base: false,
            alt_depth: 0,
        }
    }

    /// 本文を抜けたことを伝える（after_text・底本情報のレンダリング前に呼ぶ）
    pub fn enter_tail(&mut self) {
        self.in_tail = true;
    }

    /// 外字画像の alt テキストを組み立てる。
    ///
    /// 参照実装は description の中に入れ子の外字記法 `※［＃…］` があると、それを
    /// alt の中で `<img>` タグに展開する（属性値の中にタグが入る不正な HTML）。
    /// quirk `nested_gaiji_in_alt` がオンのときだけこれを再現し、オフなら
    /// 入れ子の記法を素のテキストのまま alt に残す。
    fn gaiji_alt(&mut self, description: &str) -> String {
        const NEST: &str = "※［＃";
        if !self.options.quirks.nested_gaiji_in_alt
            || self.alt_depth >= 4
            || !description.contains(NEST)
        {
            return html_escape(description);
        }
        self.alt_depth += 1;
        let mut out = String::new();
        let mut rest = description;
        while let Some(pos) = rest.find(NEST) {
            out.push_str(&html_escape(&rest[..pos]));
            let after = &rest[pos + NEST.len()..];
            match after.find('］') {
                Some(end) => {
                    let inner = after[..end].to_string();
                    out.push_str(&self.render_gaiji(&inner, None, None));
                    rest = &after[end + '］'.len_utf8()..];
                }
                None => {
                    out.push_str(&html_escape(&rest[pos..]));
                    self.alt_depth -= 1;
                    return out;
                }
            }
        }
        out.push_str(&html_escape(rest));
        self.alt_depth -= 1;
        out
    }

    /// アクセント文字の説明文（画像の alt に入る）。
    ///
    /// 参照実装の表は A^（サーカムフレックス付き A）の説明で字母 A を落として
    /// いる。quirk `accent_name_typos` がオフのときだけこれを訂正する。
    /// 対象の buggy な文字列は他と重複しないので、文字列一致で判定できる。
    fn accent_name(&self, name: &str) -> String {
        if !self.options.quirks.accent_name_typos && name == "サーカムフレックスアクセント付き"
        {
            return "サーカムフレックスアクセント付きA".to_string();
        }
        name.to_string()
    }

    /// 画像化できない外字の直前に置く外字記号。
    /// 本文セクションで、かつルビの親文字の外にあるときだけ付く。
    fn gaiji_mark_prefix(&self) -> &'static str {
        if self.in_tail || self.in_ruby_base {
            ""
        } else {
            GAIJI_MARK
        }
    }

    /// くの字点はそのまま出力するので、フッタの「表記について」に
    /// 注記を出すかどうかのフラグだけ立てる。
    ///
    /// 参照実装 aozora2html は `［＃…］` の注記の中に書かれたくの字点も数えるため、
    /// パース後のテキストノードではなく行の生テキストを渡すこと。
    pub fn scan_kunoji(&mut self, text: &str) {
        if self.has_kunoji && self.has_dakuten_kunoji {
            return;
        }
        let chars: Vec<char> = text.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if *c != KUNOJI_KU {
                continue;
            }
            match chars.get(i + 1) {
                Some(&KUNOJI_NOJI) => self.has_kunoji = true,
                Some(&KUNOJI_DAKUTEN) if chars.get(i + 2) == Some(&KUNOJI_NOJI) => {
                    self.has_dakuten_kunoji = true
                }
                _ => {}
            }
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
                        html_escape(&self.accent_name(name))
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

            Node::BlockEnd {
                block_type, params, ..
            } => {
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

            // 行単位字下げは renderer 側で行全体を包む形で処理するので、
            // ここに来ることは通常ない
            Node::LineJisage { width } => {
                format!("<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">")
            }

            Node::Note(text) => {
                self.has_notes = true;
                let inner = self.render_note_content(text, block_manager);
                format!("<span class=\"notes\">［＃{inner}］</span>")
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

            // 解決器で必ず解決 or Note 化されるので通常ここには来ない。
            // 残った場合はもとの文字列のまま注記にする。
            Node::UnresolvedReference { raw, .. } => {
                format!("<span class=\"notes\">［＃{}］</span>", html_escape(raw))
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
        let (base_html, trailing_notes) = self.render_ruby_base(children, block_manager);
        let ruby_html = self.render_nodes(ruby, block_manager);
        // Unicode nbsp (\u{00a0}) を HTML entity &nbsp; に変換
        let ruby_html = ruby_html.replace('\u{00a0}', "&nbsp;");

        match direction {
            RubyDirection::Right => {
                format!(
                    "<ruby><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>{trailing_notes}"
                )
            }
            RubyDirection::Left => {
                format!(
                    "<ruby class=\"leftrb\"><rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>{trailing_notes}"
                )
            }
        }
    }

    /// 注記の中身をレンダリングする。
    ///
    /// 参照実装 aozora2html は `read_to_nest` で注記の中身を TagParser に渡し、
    /// ルビや外字を通常の本文と同じように解決する。ここでも同じように
    /// パースし直す。入れ子の注記で無限に潜らないよう深さを制限する。
    fn render_note_content(&mut self, text: &str, block_manager: &mut BlockManager) -> String {
        const MAX_DEPTH: usize = 4;
        if self.note_depth >= MAX_DEPTH {
            return html_escape(text);
        }
        self.note_depth += 1;
        // 注記の中身は参照実装では別の TagParser が処理し、そちらは
        // 本文かどうかに関わらず外字記号を出す
        let outer_tail = std::mem::replace(&mut self.in_tail, false);
        let outer_ruby_base = std::mem::replace(&mut self.in_ruby_base, false);
        let tokens = tokenize(text);
        let mut nodes = parse(&tokens);
        resolve_inline_ruby(&mut nodes);
        let html = self.render_nodes(&nodes, block_manager);
        self.in_ruby_base = outer_ruby_base;
        self.in_tail = outer_tail;
        self.note_depth -= 1;
        html
    }

    /// ルビの親文字を組み立て、(親文字, ルビの後ろに置く注記) を返す
    ///
    /// 参照実装 aozora2html の `RubyBuffer#create_ruby` は、親文字に画像化できない
    /// 外字が含まれる場合、親文字には外字記号 `※` だけを入れ、その注記は
    /// `</ruby>` の後ろへ回す。
    fn render_ruby_base(
        &mut self,
        children: &[Node],
        block_manager: &mut BlockManager,
    ) -> (String, String) {
        let mut base = String::new();
        let mut trailing_notes = String::new();
        let outer = std::mem::replace(&mut self.in_ruby_base, true);
        for child in children {
            let html = self.render_node(child, block_manager);
            if matches!(child, Node::Gaiji { .. }) && html.starts_with("<span class=\"notes\">") {
                base.push_str(GAIJI_MARK);
                trailing_notes.push_str(&html);
            } else {
                base.push_str(&html);
            }
        }
        self.in_ruby_base = outer;
        (base, trailing_notes)
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
                    let alt = self.gaiji_alt(description);
                    return format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir, folder, file, alt
                    );
                }
            }
            // Unicode: unicodeだけがある場合（JISコードがない）
            (Some(u), None) => {
                if self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                }
                // JISコードがないので画像化できない → 注記として出力。
                // 参照実装の escape_gaiji は「入力者による注」のフラグを立てないので、
                // ここでも has_notes は立てない。
                self.add_unconverted_gaiji(description);
                return format!(
                    "{}<span class=\"notes\">［＃{}］</span>",
                    self.gaiji_mark_prefix(),
                    html_escape(description)
                );
            }
            // JisImage: jis_codeだけがある場合
            (None, Some(jis)) => {
                // 参照実装の kuten2png は面-区-点から画像を作るとき必ず
                // :newjis を立てるので、「表記について」の項目もそれに合わせる
                self.has_jisx0213 = true;
                self.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(jis);
                let alt = self.gaiji_alt(description);
                return format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir, folder, file, alt
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
                    self.add_unconverted_gaiji(description);
                    format!(
                        "{}<span class=\"notes\">［＃{}］</span>",
                        self.gaiji_mark_prefix(),
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
                    let alt = self.gaiji_alt(description);
                    format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir, folder, file, alt
                    )
                }
            }
            GaijiResult::JisImage { jis_code: jis } => {
                self.has_jisx0213 = true;
                self.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(&jis);
                let alt = self.gaiji_alt(description);
                format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir, folder, file, alt
                )
            }
            GaijiResult::Unconvertible => {
                self.add_unconverted_gaiji(description);
                format!(
                    "{}<span class=\"notes\">［＃{}］</span>",
                    self.gaiji_mark_prefix(),
                    html_escape(description)
                )
            }
        }
    }

    /// 未変換外字を外字一覧に記録する。同じ外字が複数回現れたら出現箇所を追記する。
    fn add_unconverted_gaiji(&mut self, description: &str) {
        // descriptionを最後の「、」で外字説明とページ-行数に分解する。
        // 参照実装の PAT_GAIJI は「、」を必須とするので、「、」がなければどちらも空。
        let (gaiji_name, page_line) = match description.rfind('、') {
            Some(pos) => (
                description[..pos].to_string(),
                description[pos + '、'.len_utf8()..].to_string(),
            ),
            None => (String::new(), String::new()),
        };

        if let Some(existing) = self
            .unconverted_gaiji
            .iter_mut()
            .find(|g| g.gaiji_name == gaiji_name)
        {
            existing.page_lines.push(page_line);
            return;
        }
        self.unconverted_gaiji.push(UnconvertedGaiji {
            gaiji_name,
            page_lines: vec![page_line],
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

        // 参照実装 Tag::Img は幅・高さが指定されていなくても width="" height="" と
        // 空の属性を出す（Ruby の文字列展開で nil が空文字になる）。
        // quirk empty_image_dimensions がオンのときだけこれを再現する。
        let dimensions = if self.options.quirks.empty_image_dimensions {
            let w = width.map(|v| v.to_string()).unwrap_or_default();
            let h = height.map(|v| v.to_string()).unwrap_or_default();
            format!(" width=\"{w}\" height=\"{h}\"")
        } else {
            let mut d = String::new();
            if let Some(w) = width {
                d.push_str(&format!(" width=\"{w}\""));
            }
            if let Some(h) = height {
                d.push_str(&format!(" height=\"{h}\""));
            }
            d
        };

        format!(
            "<img class=\"{class}\"{dimensions} src=\"{}\" alt=\"{}\" />",
            filename,
            html_escape(alt)
        )
    }
}
