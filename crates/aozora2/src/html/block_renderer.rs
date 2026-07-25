//! 中立AST（[`Block`] 木）を本文HTMLに変換する新バックエンド。
//!
//! docs/plan-neutral-ast.md Phase B4。旧 `renderer.rs`＋`BlockManager` を置き換える
//! ことを目指すが、まだ**最小核**（jisage の Nested と内容行、インラインは Text
//! のみ）。旧経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ足す。
//! バックエンドは木を**状態なしに歩く**だけ（BlockManager を持たない）。

use aozora_core::ast::{to_inlines, Block, BlockKind, Break, CloseKind, Inline};
use aozora_core::gaiji::{parse_gaiji, GaijiResult};
use aozora_core::node::{FontSizeType, MidashiLevel, RubyDirection};
use aozora_core::parser::parse;
use aozora_core::parser::reference_resolver::resolve_inline_ruby;
use aozora_core::token::Token;
use aozora_core::tokenizer::tokenize;

use super::options::RenderOptions;
use super::presentation::{
    html_escape, jis_code_to_path, midashi_combined_css_class, midashi_html_tag, style_css_class,
    style_html_tag, UnconvertedGaiji,
};

/// 画像化できない外字を本文中で示す記号
const GAIJI_MARK: &str = "※";

/// くの字点（繰り返し記号）の構成文字（フッタ「表記について」の判定用）。
const KUNOJI_KU: char = '／';
const KUNOJI_NOJI: char = '＼';
const KUNOJI_DAKUTEN: char = '″';

/// 参照実装 kuten2png は alt 生成前に PAT_KUTEN = /「※」[は|の]/ を除去する。
fn strip_kuten_prefix(description: &str) -> String {
    description.replace("「※」は", "").replace("「※」の", "")
}

/// ブロックの開始タグ（`\r\n` を含まない）。複数行 Nested は末尾に `\r\n` を足し、
/// 行スコープ包み（[`Block::LineWrap`]）はそのまま内容を続ける。None は div 非包み。
fn block_open_tag(kind: &BlockKind, empty_indent_css: bool) -> Option<String> {
    match kind {
        // 空幅（None）は参照が `jisage_` / `margin-left: em`（不正CSS, Quirk）を出す。
        // empty_indent_css=false（clean）なら妥当な `class="jisage"`。
        BlockKind::Jisage { width } => Some(match width {
            Some(w) => format!("<div class=\"jisage_{w}\" style=\"margin-left: {w}em\">"),
            None if empty_indent_css => {
                "<div class=\"jisage_\" style=\"margin-left: em\">".to_string()
            }
            None => "<div class=\"jisage\">".to_string(),
        }),
        BlockKind::Chitsuki { width } => Some(format!(
            "<div class=\"chitsuki_{width}\" style=\"text-align:right; margin-right: {width}em\">"
        )),
        BlockKind::Jizume { width } => Some(format!(
            "<div class=\"jizume_{width}\" style=\"width: {width}em\">"
        )),
        BlockKind::Keigakomi => {
            Some("<div class=\"keigakomi\" style=\"border: solid 1px\">".to_string())
        }
        BlockKind::Yokogumi => Some("<div class=\"yokogumi\">".to_string()),
        BlockKind::Caption => Some("<div class=\"caption\">".to_string()),
        BlockKind::FontSize { size_type, level } => {
            let (class, style) = font_size_class_style(*size_type, *level);
            Some(format!("<div class=\"{class}\" style=\"{style}\">"))
        }
        BlockKind::Futoji => Some("<div class=\"futoji\">".to_string()),
        BlockKind::Shatai => Some("<div class=\"shatai\">".to_string()),
        // TODO: Burasage（per-line 包み）・Midashi（id カウンタ）。
        _ => None,
    }
}

/// フォントサイズ（大/小＋段階）の class と style（参照 render_font_size と同じ）。
fn font_size_class_style(size_type: FontSizeType, level: u32) -> (String, String) {
    match size_type {
        FontSizeType::Dai => {
            let size = match level {
                1 => "large",
                2 => "x-large",
                _ => "xx-large",
            };
            (format!("dai{level}"), format!("font-size: {size};"))
        }
        FontSizeType::Sho => {
            let size = match level {
                1 => "small",
                2 => "x-small",
                _ => "xx-small",
            };
            (format!("sho{level}"), format!("font-size: {size};"))
        }
    }
}

/// 中立AST（[`Block`] 木）を本文HTMLに変換する新バックエンド（状態付き）。
/// 最終的に旧 `renderer.rs`＋`BlockManager`＋`node_renderer` を置き換える先。
/// 木を歩くだけで BlockManager を持たないが、フッタ「表記について」用の使用フラグ・
/// 外字一覧は描画の副作用として蓄積する（参照実装と同じ）。
pub struct BlockRenderer<'a> {
    options: &'a RenderOptions,
    /// 注記を使用したか
    pub has_notes: bool,
    /// 外字画像を使用したか
    pub has_gaiji_images: bool,
    /// アクセント記号を使用したか
    pub has_accent: bool,
    /// JIS X 0213 文字を使用したか
    pub has_jisx0213: bool,
    /// くの字点を使用したか
    pub has_kunoji: bool,
    /// 濁点付きくの字点を使用したか
    pub has_dakuten_kunoji: bool,
    /// 未変換外字のリスト（表記について）
    pub unconverted_gaiji: Vec<UnconvertedGaiji>,
    /// tail セクション（after_text/bibliographical）処理中か
    in_tail: bool,
    /// ルビ親文字を組み立て中か（親文字内では外字記号を個別に出さない）
    in_ruby_base: bool,
    /// alt の入れ子外字展開の深さ（暴走防止）
    alt_depth: usize,
    /// 注記の中身を再帰描画する深さ
    note_depth: usize,
    /// 見出しアンカー id カウンタ（O=+100, Naka=+10, Ko=+1）
    midashi_id_counter: u32,
}

impl<'a> BlockRenderer<'a> {
    /// 新しいバックエンドを作成
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
            in_tail: false,
            in_ruby_base: false,
            alt_depth: 0,
            note_depth: 0,
            midashi_id_counter: 0,
        }
    }

    /// tail セクション（after_text/bibliographical）処理に入る（参照 enter_tail）。
    /// 以降、外字記号 ※ のプレフィックスを抑制する。
    pub fn enter_tail(&mut self) {
        self.in_tail = true;
    }

    /// くの字点をフッタ「表記について」用に数える（参照 scan_kunoji）。
    /// 注記の中にも書かれうるので、パース後ではなく生のソース行を渡すこと。
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

    /// ブロック列を本文HTML（main_text の内側）に変換する。
    pub fn render_body(&mut self, blocks: &[Block]) -> String {
        let mut out = String::new();
        for block in blocks {
            self.render_block(block, &mut out);
        }
        out
    }

    fn render_block(&mut self, block: &Block, out: &mut String) {
        match block {
            Block::Line { inline, brk, .. } => {
                // 行末 <br /> の要否は Lower 時に確定済み（brk）。バックエンドは木を
                // 状態なしに歩くだけで、描画済みHTMLの詮索はしない。
                self.render_inlines(inline, out);
                if *brk == Break::Br {
                    out.push_str("<br />");
                }
                out.push_str("\r\n");
            }
            Block::Nested {
                kind,
                children,
                close,
                ..
            } => self.render_nested(kind, children, *close, out),
            Block::LineWrap { kind, inline, .. } => {
                // 行全体をブロック div で1行に包む（行スコープ字下げ／地付き）。
                // 開き直後の改行も内側 <br /> も出さず、行末に `\r\n` のみ。
                if let Some(open) = block_open_tag(kind, self.options.quirks.empty_indent_css) {
                    out.push_str(&open);
                    self.render_inlines(inline, out);
                    out.push_str("</div>\r\n");
                } else {
                    // div で包まない種類（想定外）は内容だけ出す。
                    self.render_inlines(inline, out);
                    out.push_str("\r\n");
                }
            }
        }
    }

    fn render_nested(
        &mut self,
        kind: &BlockKind,
        children: &[Block],
        close: CloseKind,
        out: &mut String,
    ) {
        // ぶら下げ（折り返し字下げ）は per-line モデル。外側 div を作らず、各内容行を
        // 個別に burasage div で包む（空行は素の `<br />`）。閉じは何も出さない。
        if let BlockKind::Burasage { wrap_width, width } = kind {
            self.render_burasage(*wrap_width, *width, children, out);
            return;
        }
        // ブロック形見出し（［＃ここから中見出し］…）。h4/h3/h5 + midashi_anchor で
        // 全内容行を包む。id はインライン見出しと同じカウンタを描画時に発番する。
        if let BlockKind::Midashi { level, style } = kind {
            let tag = midashi_html_tag(*level);
            let class = midashi_combined_css_class(*level, *style);
            let id = self.generate_midashi_id(*level);
            out.push_str(&format!(
                "<{tag} class=\"{class}\"><a class=\"midashi_anchor\" id=\"midashi{id}\">\r\n"
            ));
            for child in children {
                self.render_block(child, out);
            }
            out.push_str(&format!("</a></{tag}>\r\n"));
            return;
        }
        // 閉じタグの出力形は互換メタデータ（CloseKind）で決める。
        let close_nl = match close {
            CloseKind::NoBreak => "</div>",
            CloseKind::Newline => "</div>\r\n",
            CloseKind::BareBreak => "</div><br />\r\n",
        };
        // 開始タグ（旧 tag_generator の block 形と厳密一致）。複数行ブロックは開き
        // 直後に `\r\n` を出す（行スコープ包みとの違い）。None なら div で包まない。
        match block_open_tag(kind, self.options.quirks.empty_indent_css) {
            Some(open) => {
                out.push_str(&open);
                out.push_str("\r\n");
                for child in children {
                    self.render_block(child, out);
                }
                out.push_str(close_nl);
            }
            None => {
                for child in children {
                    self.render_block(child, out);
                }
            }
        }
    }

    /// ぶら下げブロックを per-line で描画する（参照 generate_burasage_start）。
    /// 内容行は個別に burasage div で包み（内側 `<br />` なし）、空行は素の `<br />`。
    /// 行以外の子（行スコープ包み・入れ子ブロック・見出し等）は包まずそのまま描画する。
    fn render_burasage(
        &mut self,
        wrap_width: Option<u32>,
        width: Option<u32>,
        children: &[Block],
        out: &mut String,
    ) {
        // margin-left は折り返し幅。空（None）のとき参照は空文字（Quirk）／clean で 0。
        let margin = wrap_width.map(|w| w.to_string()).unwrap_or_else(|| {
            if self.options.quirks.empty_indent_css {
                String::new()
            } else {
                "0".to_string()
            }
        });
        let text_indent = width.unwrap_or(0) as i32 - wrap_width.unwrap_or(0) as i32;
        for child in children {
            match child {
                Block::Line { inline, .. } if inline.is_empty() => {
                    // 空行は burasage で包まず素の `<br />`。
                    out.push_str("<br />\r\n");
                }
                Block::Line { inline, .. } => {
                    out.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\">"
                    ));
                    self.render_inlines(inline, out);
                    out.push_str("</div>\r\n");
                }
                // 行スコープ字下げ・入れ子ブロック等は burasage で包まない。
                other => self.render_block(other, out),
            }
        }
    }

    fn render_inlines(&mut self, inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            self.render_inline(inline, out);
        }
    }

    fn render_inline(&mut self, inline: &Inline, out: &mut String) {
        match inline {
            Inline::Text(s) => out.push_str(&html_escape(s)),
            Inline::Style {
                children,
                style_type,
            } => {
                let tag = style_html_tag(*style_type);
                let class = style_css_class(*style_type);
                out.push_str(&format!("<{tag} class=\"{class}\">"));
                self.render_inlines(children, out);
                out.push_str(&format!("</{tag}>"));
            }
            Inline::Tcy { children } => self.wrap_span(children, "dir=\"ltr\"", out),
            Inline::Keigakomi { children } => self.wrap_span(children, "class=\"keigakomi\"", out),
            Inline::Yokogumi { children } => self.wrap_span(children, "class=\"yokogumi\"", out),
            Inline::Caption { children } => self.wrap_span(children, "class=\"caption\"", out),
            Inline::Warigaki { children } => self.wrap_span(children, "class=\"warigaki\"", out),
            Inline::Ruby {
                base,
                ruby,
                direction,
                keep_gaiji_notes_in_base,
            } => {
                // 親文字に画像化できない外字があると、参照は rb に外字記号 ※ だけ残し
                // 注記 <span class="notes">…</span> をルビの外（trailing）へ出す。
                // ［＃注記付き］範囲ルビ等（keep_gaiji_notes_in_base）は通常描画のまま。
                let (base_html, trailing_notes) = if *keep_gaiji_notes_in_base {
                    let mut b = String::new();
                    self.render_inlines(base, &mut b);
                    (b, String::new())
                } else {
                    self.render_ruby_base(base)
                };
                let mut ruby_html = String::new();
                self.render_inlines(ruby, &mut ruby_html);
                let ruby_html = ruby_html.replace('\u{00a0}', "&nbsp;");
                let ropen = match direction {
                    RubyDirection::Right => "<ruby>",
                    RubyDirection::Left => "<ruby class=\"leftrb\">",
                };
                out.push_str(&format!(
                    "{ropen}<rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>{trailing_notes}"
                ));
            }
            Inline::Gaiji {
                description,
                unicode,
                jis_code,
                had_igeta,
            } => {
                let s = self.render_gaiji(
                    description,
                    unicode.as_deref(),
                    jis_code.as_deref(),
                    *had_igeta,
                );
                out.push_str(&s);
            }
            Inline::Accent {
                code,
                name,
                unicode,
            } => {
                let s = self.render_accent(code, name, unicode.as_deref());
                out.push_str(&s);
            }
            Inline::Img {
                filename,
                alt,
                is_photo,
                width,
                height,
            } => {
                // 参照は alt 内の外字を外字一覧に登録する副作用を持つ。
                self.register_alt_gaiji(alt);
                let s = self.render_img(filename, alt, *is_photo, *width, *height);
                out.push_str(&s);
            }
            Inline::DakutenKatakana { num } => {
                out.push_str(aozora_core::node::Node::dakuten_katakana_char(num))
            }
            Inline::ChitsukiInline { width, children } => {
                out.push_str(&format!(
                    "<div class=\"chitsuki_{width}\" style=\"text-align:right; margin-right: {width}em\">"
                ));
                self.render_inlines(children, out);
                out.push_str("</div>");
            }
            Inline::BlockInline { kind, children } => {
                // 同行で開閉するブロック形。見出しは h4/a＋id、その他は div で包む。
                if let BlockKind::Midashi { level, style } = kind {
                    let tag = midashi_html_tag(*level);
                    let class = midashi_combined_css_class(*level, *style);
                    let id = self.generate_midashi_id(*level);
                    out.push_str(&format!(
                        "<{tag} class=\"{class}\"><a class=\"midashi_anchor\" id=\"midashi{id}\">"
                    ));
                    self.render_inlines(children, out);
                    out.push_str(&format!("</a></{tag}>"));
                } else if let Some(open) = block_open_tag(kind, self.options.quirks.empty_indent_css)
                {
                    out.push_str(&open);
                    self.render_inlines(children, out);
                    out.push_str("</div>");
                } else {
                    self.render_inlines(children, out);
                }
            }
            Inline::Kaeriten(text) => {
                out.push_str(&format!("<sub class=\"kaeriten\">{}</sub>", html_escape(text)))
            }
            Inline::Note(text) => {
                self.has_notes = true;
                let inner = self.render_note_content(text);
                out.push_str(&format!("<span class=\"notes\">［＃{inner}］</span>"));
            }
            Inline::AnnotationEnd {
                prefix,
                content,
                suffix,
            } => {
                self.has_notes = true;
                let mut content_html = String::new();
                self.render_inlines(content, &mut content_html);
                out.push_str(&format!(
                    "<span class=\"notes\">［＃{}{}{}］</span>",
                    html_escape(prefix),
                    content_html,
                    html_escape(suffix)
                ));
            }
            Inline::Okurigana(text) => {
                // 参照実装 Tag::Okurigana は注記と同じ再パースを通し、外側 （ ） を除去する。
                let inner = self
                    .render_note_content(text)
                    .replace('（', "")
                    .replace('）', "");
                out.push_str(&format!("<sup class=\"okurigana\">{inner}</sup>"));
            }
            Inline::FontSize {
                children,
                size_type,
                level,
            } => {
                let (class, style) = font_size_class_style(*size_type, *level);
                out.push_str(&format!("<span class=\"{class}\" style=\"{style}\">"));
                self.render_inlines(children, out);
                out.push_str("</span>");
            }
            Inline::Midashi {
                children,
                level,
                style,
            } => {
                let mut inner = String::new();
                self.render_inlines(children, &mut inner);
                let tag = midashi_html_tag(*level);
                let class = midashi_combined_css_class(*level, *style);
                let id = self.generate_midashi_id(*level);
                out.push_str(&format!(
                    "<{tag} class=\"{class}\"><a class=\"midashi_anchor\" id=\"midashi{id}\">{inner}</a></{tag}>"
                ));
            }
            Inline::Warichu {
                open,
                suppress_paren,
            } => {
                if *open {
                    let paren = if *suppress_paren { "" } else { "（" };
                    out.push_str(&format!("<span class=\"warichu\">{paren}"));
                } else {
                    let paren = if *suppress_paren { "" } else { "）" };
                    out.push_str(&format!("{paren}</span>"));
                }
            }
        }
    }

    /// 見出しアンカー id を生成する（参照 BlockManager::generate_midashi_id と同じ）。
    fn generate_midashi_id(&mut self, level: MidashiLevel) -> u32 {
        let increment = match level {
            MidashiLevel::O => 100,
            MidashiLevel::Naka => 10,
            MidashiLevel::Ko => 1,
        };
        self.midashi_id_counter += increment;
        self.midashi_id_counter
    }

    /// 注記/送り仮名の中身を再パースして描画する（参照は別 TagParser で処理し、
    /// 本文かどうかに関わらず外字記号を出す＝in_tail/in_ruby_base をリセット）。
    fn render_note_content(&mut self, text: &str) -> String {
        const MAX_DEPTH: usize = 4;
        if self.note_depth >= MAX_DEPTH {
            return html_escape(text);
        }
        self.note_depth += 1;
        let outer_tail = std::mem::replace(&mut self.in_tail, false);
        let outer_ruby_base = std::mem::replace(&mut self.in_ruby_base, false);
        let tokens = tokenize(text);
        let mut nodes = parse(&tokens);
        resolve_inline_ruby(&mut nodes);
        let inlines = to_inlines(&nodes);
        let mut html = String::new();
        self.render_inlines(&inlines, &mut html);
        self.in_ruby_base = outer_ruby_base;
        self.in_tail = outer_tail;
        self.note_depth -= 1;
        html
    }

    fn render_img(
        &self,
        filename: &str,
        alt: &str,
        is_photo: bool,
        width: Option<u32>,
        height: Option<u32>,
    ) -> String {
        let class = if is_photo { "photo" } else { "illustration" };
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
        let alt_out = if self.options.quirks.raw_image_alt {
            alt.to_string()
        } else {
            html_escape(alt)
        };
        format!(
            "<img class=\"{class}\"{dimensions} src=\"{}\" alt=\"{}\" />",
            filename, alt_out
        )
    }

    /// `<span {attr}>{children}</span>` で包む（縦中横・罫囲み・横組み・キャプション等）。
    fn wrap_span(&mut self, children: &[Inline], attr: &str, out: &mut String) {
        out.push_str(&format!("<span {attr}>"));
        self.render_inlines(children, out);
        out.push_str("</span>");
    }

    // --- 以下は旧 node_renderer から移植した外字・アクセント描画（Path B）。 ---
    // node_renderer 撤去後はこちらが唯一の実装になる。挙動は厳密一致。

    /// アクセント文字（外字画像）を描画する。
    fn render_accent(&mut self, code: &str, name: &str, unicode: Option<&str>) -> String {
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

    fn accent_name(&self, name: &str) -> String {
        if !self.options.quirks.accent_name_typos && name == "サーカムフレックスアクセント付き" {
            return "サーカムフレックスアクセント付きA".to_string();
        }
        name.to_string()
    }

    fn gaiji_mark_prefix(&self) -> &'static str {
        if self.in_tail || self.in_ruby_base {
            ""
        } else {
            GAIJI_MARK
        }
    }

    /// ルビ親文字を描画し、画像化できない外字は rb に記号 ※ だけ残して注記を
    /// trailing（ルビ外）へ振り分ける（参照 render_ruby_base 相当）。
    /// 返り値は (rb の中身, ルビの後ろに続く注記列)。
    fn render_ruby_base(&mut self, base: &[Inline]) -> (String, String) {
        let mut rb = String::new();
        let mut trailing = String::new();
        let outer = std::mem::replace(&mut self.in_ruby_base, true);
        for inline in base {
            let mut html = String::new();
            self.render_inline(inline, &mut html);
            if matches!(inline, Inline::Gaiji { .. }) && html.starts_with("<span class=\"notes\">") {
                rb.push_str(GAIJI_MARK);
                trailing.push_str(&html);
            } else {
                rb.push_str(&html);
            }
        }
        self.in_ruby_base = outer;
        (rb, trailing)
    }

    /// 外字をHTMLに変換（node_renderer::render_gaiji と厳密一致）。
    fn render_gaiji(
        &mut self,
        description: &str,
        unicode: Option<&str>,
        jis_code: Option<&str>,
        had_igeta: bool,
    ) -> String {
        let alt_name = |renderer: &mut Self| {
            if had_igeta {
                renderer.gaiji_alt(description)
            } else {
                String::new()
            }
        };
        let notes_mark = if had_igeta { "＃" } else { "" };
        match (unicode, jis_code) {
            (Some(u), Some(jis)) => {
                self.has_jisx0213 = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                } else {
                    self.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(jis);
                    let alt = alt_name(self);
                    return format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir, folder, file, alt
                    );
                }
            }
            (Some(u), None) => {
                if self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                }
                self.add_unconverted_gaiji(description, had_igeta);
                return format!(
                    "{}<span class=\"notes\">［{}{}］</span>",
                    self.gaiji_mark_prefix(),
                    notes_mark,
                    html_escape(description)
                );
            }
            (None, Some(jis)) => {
                self.has_jisx0213 = true;
                self.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(jis);
                let alt = alt_name(self);
                return format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir, folder, file, alt
                );
            }
            (None, None) => {}
        }

        match parse_gaiji(description) {
            GaijiResult::Unicode(s) => {
                if self.options.use_unicode {
                    s.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    self.add_unconverted_gaiji(description, had_igeta);
                    format!(
                        "{}<span class=\"notes\">［{}{}］</span>",
                        self.gaiji_mark_prefix(),
                        notes_mark,
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
                    let alt = alt_name(self);
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
                let alt = alt_name(self);
                format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir, folder, file, alt
                )
            }
            GaijiResult::Unconvertible => {
                self.add_unconverted_gaiji(description, had_igeta);
                format!(
                    "{}<span class=\"notes\">［{}{}］</span>",
                    self.gaiji_mark_prefix(),
                    notes_mark,
                    html_escape(description)
                )
            }
        }
    }

    fn gaiji_alt(&mut self, description: &str) -> String {
        const NEST: &str = "※［＃";
        let stripped = strip_kuten_prefix(description);
        let description = stripped.as_str();
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
                    out.push_str(&self.render_gaiji(&inner, None, None, true));
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

    /// 画像注記 alt 内の外字を外字一覧・表記フラグに登録する（出力に影響しない）。
    fn register_alt_gaiji(&mut self, alt: &str) {
        for token in tokenize(alt) {
            let Token::Gaiji {
                description,
                had_igeta,
            } = token
            else {
                continue;
            };
            match parse_gaiji(&description) {
                GaijiResult::JisImage { .. } | GaijiResult::JisConverted { .. } => {
                    self.has_jisx0213 = true;
                    self.has_gaiji_images = true;
                }
                GaijiResult::Unconvertible => {
                    self.add_unconverted_gaiji(&description, had_igeta);
                }
                GaijiResult::Unicode(_) => {
                    if !self.options.use_unicode {
                        self.add_unconverted_gaiji(&description, had_igeta);
                    }
                }
            }
        }
    }

    fn add_unconverted_gaiji(&mut self, description: &str, had_igeta: bool) {
        let (gaiji_name, page_line) = if !had_igeta {
            (String::new(), String::new())
        } else {
            match description.rfind('、') {
                Some(pos) => (
                    description[..pos].to_string(),
                    description[pos + '、'.len_utf8()..].to_string(),
                ),
                None => (String::new(), String::new()),
            }
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
}

/// 単一行のインラインHTML（行末 `<br />`/`\r\n` なし）を新経路で描画する。
/// 旧 `HtmlRenderer::render_line` の中立AST版（インライン列のみ）。
pub fn render_line_inline(line: &str, options: &RenderOptions) -> String {
    use aozora_core::ast::to_inlines;
    use aozora_core::parser::parse;
    use aozora_core::parser::reference_resolver::{resolve_inline_ruby, resolve_references};

    let tokens = tokenize(line);
    let mut nodes = parse(&tokens);
    resolve_references(&mut nodes);
    resolve_inline_ruby(&mut nodes);
    let inlines = to_inlines(&nodes);
    let mut br = BlockRenderer::new(options);
    let mut out = String::new();
    br.render_inlines(&inlines, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::{convert, RenderOptions};
    use aozora_core::document::extract_body_lines;
    use aozora_core::lower::lower_to_blocks;
    use aozora_core::parser::parse_document_raw;

    /// 旧経路（convert）と新経路（lower_to_blocks→render_body_blocks）の本文HTMLが
    /// jisage 文書で byte 一致することを固定する（垂直スライスの最小核）。
    /// インラインは Text のみなので本文はプレーンテキストに限る。
    fn assert_body_matches(src: &str) {
        let old = convert(src, &RenderOptions::default());
        let open = "<div class=\"main_text\">";
        let close = "</div>\r\n<div class=\"bibliographical_information\">";
        let start = old.find(open).expect("main_text open") + open.len();
        let end = old.find(close).expect("bibliographical close");
        let old_inner = &old[start..end];

        let mut lines: Vec<&str> = src.split("\r\n").collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let body_lines = extract_body_lines(&lines);
        let raw = parse_document_raw(&body_lines);
        let blocks = lower_to_blocks(&raw);
        let new_inner = BlockRenderer::new(&RenderOptions::default()).render_body(&blocks);

        assert_eq!(new_inner, old_inner, "\n新:{new_inner:?}\n旧:{old_inner:?}");
    }

    #[test]
    fn test_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n前文\r\n［＃ここから２字下げ］\r\n内容A\r\n内容B\r\n［＃ここで字下げ終わり］\r\n後文\r\n底本：テスト\r\n",
        );
    }

    /// 連続 jisage は参照では兄弟（implicit_close）。ここから1字下げ→ここから3字下げは
    /// jisage_1 を閉じてから jisage_3 を開く。
    #[test]
    fn test_sibling_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから１字下げ］\r\nA\r\n［＃ここから３字下げ］\r\nB\r\n［＃ここで字下げ終わり］\r\n後\r\n底本：テスト\r\n",
        );
    }

    /// 装飾（傍点＝Style）・縦中横（Tcy）を含む内容行が旧経路と byte 一致すること。
    #[test]
    fn test_inline_style_tcy_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n対象［＃「対象」に傍点］の文と12［＃「12」は縦中横］日\r\n底本：テスト\r\n",
        );
    }

    /// ルビ（外字を含まない一般ケース）が旧経路と byte 一致すること。
    #[test]
    fn test_inline_ruby_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n東京《とうきょう》の｜山手線《やまのてせん》に乗る\r\n底本：テスト\r\n",
        );
    }

    /// jisage の中に装飾を含む内容行。
    #[test]
    fn test_jisage_with_inline_style_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字下げ］\r\n強い［＃「強い」は太字］語\r\n［＃ここで字下げ終わり］\r\n底本：テスト\r\n",
        );
    }

    /// 順に開閉する2つの jisage ブロック。
    #[test]
    fn test_sequential_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字下げ］\r\nA\r\n［＃ここで字下げ終わり］\r\n間\r\n［＃ここから４字下げ］\r\nB\r\n［＃ここで字下げ終わり］\r\n後\r\n底本：テスト\r\n",
        );
    }

    /// chitsuki（地付き）・jizume（字詰め）ブロックが旧経路と byte 一致すること。
    #[test]
    fn test_chitsuki_jizume_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字上げ］\r\n右寄\r\n［＃ここで字上げ終わり］\r\n［＃ここから20字詰め］\r\n詰\r\n［＃ここで字詰め終わり］\r\n底本：テスト\r\n",
        );
    }
}
