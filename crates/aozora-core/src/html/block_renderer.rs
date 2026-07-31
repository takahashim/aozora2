//! Aozora AST（[`Block`] 木）を本文HTMLに変換する新バックエンド。
//!
//! docs/plan-neutral-ast.md Phase B4。旧 `renderer.rs`＋`BlockManager` を置き換える
//! ことを目指すが、まだ**最小核**（jisage の Nested と内容行、インラインは Text
//! のみ）。旧経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ足す。
//! バックエンドは木を**状態なしに歩く**だけ（BlockManager を持たない）。

use crate::ast::{
    Block, BlockKind, Break, BurasageGeometry, CloseKind, Inline, InlineKind, OpenKind,
};
use crate::delimiters::GAIJI_MARK_STR;
use crate::gaiji::{
    parse_gaiji, split_nested_gaiji, strip_kuten_prefix, GaijiResult, NestedGaijiSegment,
};
use crate::lower::inline::to_inlines;
use crate::node::{FontSizeType, MidashiLevel, RubyDirection};
use crate::token::TokenKind;
use crate::tokenizer::tokenize;

use super::notation::NotationState;
use super::options::RenderOptions;
use super::presentation::{
    html_escape, jis_code_to_path, midashi_combined_css_class, midashi_html_tag, style_css_class,
    style_html_tag,
};

/// alt の入れ子外字を展開する深さの上限（暴走防止）
const MAX_ALT_DEPTH: usize = 4;

/// 外字1つを描画した結果と、それが**注記になったか**。
///
/// ルビ親文字の中では、画像化できず注記になった外字だけを rb の外へ追い出す
/// （[`BlockRenderer::render_ruby_base`]）。どちらになったかは描画した側が
/// 知っているので、描画済みHTMLを `starts_with` で覗き直さずここで持ち回す。
enum GaijiHtml {
    /// 画像・実体参照など、そのまま置ける形になった
    Direct(String),
    /// 画像化も実体参照化もできず `<span class="notes">…</span>` になった
    Note(String),
}

impl GaijiHtml {
    fn into_html(self) -> String {
        match self {
            GaijiHtml::Direct(s) | GaijiHtml::Note(s) => s,
        }
    }
}

/// Unicode 文字列を数値実体参照列にする。
///
/// 参照実装は `yml/jis2ucs.yml`（`"&#x3000;"`）と `Tag::EmbedGaiji#to_s` の
/// `"&#x#{@unicode};"` のどちらも**大文字16進・最低4桁**なので、それに合わせる
/// （表には5桁の面区点もあるので `{:04X}` の最小幅指定で両方を満たす）。
fn numeric_entities(s: &str) -> String {
    s.chars().map(|c| format!("&#x{:04X};", c as u32)).collect()
}

/// 後付け（底本情報・本文終わり後）の行末に `<br />` を足すか。
///
/// 参照実装 `tail_output` は本文の `general_output` とはまったく別の規則で、
/// **描画済みの行文字列**を正規表現で見て決める（`aozora2html.rb:1287`）。
///
/// ```text
/// (<br />$|</p>$|</h\d>$|<div.*>$|</div>$|^<[^>]*>$)
/// ```
///
/// 当たれば `<br />` を足さない。行がまるごと1つのタグ（`^<[^>]*>$`）という枝が
/// あるので、外字画像だけの行や挿絵だけの行が該当する。ここは AST の
/// [`Break`] では表せない——「描画してみないと分からない」のが参照の仕様そのもの
/// なので、描画した側（＝この関数）で判定する。
fn tail_line_needs_br(line: &str) -> bool {
    let bytes = line.as_bytes();
    // `</h\d>$`
    let ends_with_hn = bytes.len() >= 5 && {
        let n = bytes.len();
        bytes[n - 1] == b'>'
            && bytes[n - 2].is_ascii_digit()
            && bytes[n - 3] == b'h'
            && bytes[n - 4] == b'/'
            && bytes[n - 5] == b'<'
    };
    // `<div.*>$`（「どこかに <div があって、末尾が > 」。開いたままの div が該当）
    let div_and_ends_with_gt = line.ends_with('>') && line.contains("<div");
    // `^<[^>]*>$`（行全体がちょうど1つのタグ）
    let whole_line_is_one_tag = line.len() >= 2
        && line.starts_with('<')
        && line.ends_with('>')
        && !line[1..line.len() - 1].contains('>');

    !(line.ends_with("<br />")
        || line.ends_with("</p>")
        || ends_with_hn
        || div_and_ends_with_gt
        || line.ends_with("</div>")
        || whole_line_is_one_tag)
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
        // 空幅（None）は参照が `jizume_` / `width: em`（不正CSS, Quirk）を出す。
        BlockKind::Jizume { width } => Some(match width {
            Some(w) => format!("<div class=\"jizume_{w}\" style=\"width: {w}em\">"),
            None if empty_indent_css => "<div class=\"jizume_\" style=\"width: em\">".to_string(),
            None => "<div class=\"jizume\">".to_string(),
        }),
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
        // この2つは開始タグが単純な div ではないので、呼び出し側が先に捌く。
        // Burasage は外側 div を作らない per-line 包み（[`BlockRenderer::render_burasage`]）、
        // Midashi は h4/h3/h5＋アンカー id（[`BlockRenderer::render_nested`]）。
        // ここに `_` を置くと変種を足したとき黙って div 非包みになるので網羅させる。
        BlockKind::Burasage(_) | BlockKind::Midashi { .. } => None,
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

/// Aozora AST（[`Block`] 木）を本文HTMLに変換する新バックエンド（状態付き）。
/// 最終的に旧 `renderer.rs`＋`BlockManager`＋`node_renderer` を置き換える先。
/// 木を歩くだけで BlockManager を持たないが、フッタ「表記について」用の使用フラグ・
/// 外字一覧は描画の副作用として蓄積する（参照実装と同じ）。
pub struct BlockRenderer<'a> {
    options: &'a RenderOptions,
    /// フッタ「表記について」の材料（描画の副作用として蓄積する）
    notation: NotationState,
    /// tail セクション（after_text/bibliographical）処理中か
    in_tail: bool,
    /// ルビ親文字を組み立て中か（親文字内では外字記号を個別に出さない）
    in_ruby_base: bool,
    /// alt の入れ子外字展開の深さ（暴走防止）
    alt_depth: usize,
    /// 見出しアンカー id カウンタ（O=+100, Naka=+10, Ko=+1）
    midashi_id_counter: u32,
}

impl<'a> BlockRenderer<'a> {
    /// 新しいバックエンドを作成
    pub fn new(options: &'a RenderOptions) -> Self {
        Self {
            options,
            notation: NotationState::default(),
            in_tail: false,
            in_ruby_base: false,
            alt_depth: 0,
            midashi_id_counter: 0,
        }
    }

    /// tail セクション（after_text/bibliographical）を処理中かを設定する
    /// （参照 tail_output）。tail では外字記号 ※ のプレフィックスを抑制する。
    pub fn set_tail(&mut self, in_tail: bool) {
        self.in_tail = in_tail;
    }

    /// 描画の副作用として溜まった「表記について」の材料。
    pub fn notation(&self) -> &NotationState {
        &self.notation
    }

    /// くの字点をフッタ「表記について」用に数える（参照 scan_kunoji）。
    /// 注記の中にも書かれうるので、パース後ではなく生のソース行を渡すこと。
    pub fn scan_kunoji(&mut self, text: &str) {
        self.notation.scan_kunoji(text);
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
            Block::Line { inline, brk, .. } if self.in_tail => {
                // 後付けは参照 tail_output が別規則で描く（[`tail_line_needs_br`]）。
                let mut line = String::new();
                self.render_inlines(inline, &mut line);
                self.push_tail_line(&line, out);
            }
            Block::Line { inline, brk, .. } => {
                // 行末 <br /> の要否は Lower 時に確定済み（brk）。バックエンドは木を
                // 状態なしに歩くだけで、描画済みHTMLの詮索はしない。
                self.render_inlines(inline, out);
                if *brk == Break::Br {
                    out.push_str("<br />");
                }
                // NoNewline は行途中クローズの前半。改行は続く閉じタグが出す。
                if *brk != Break::NoNewline {
                    out.push_str("\r\n");
                }
            }
            Block::Nested {
                kind,
                children,
                close,
                open,
                ..
            } => self.render_nested(kind, children, *close, *open, out),
            Block::LineWrap { kinds, inline, .. } => {
                // 行全体をブロック div で1行に包む（行スコープ字下げ／地付き）。
                // 開き直後の改行も内側 <br /> も出さず、行末に `\r\n` のみ。
                // kinds は外側から内側の順（`［＃N字下げ］` は 1 行に複数書ける）。
                let mut line = String::new();
                let mut opened = 0usize;
                for kind in kinds {
                    match block_open_tag(kind, self.options.quirks.empty_indent_css) {
                        Some(open) => {
                            line.push_str(&open);
                            opened += 1;
                        }
                        None => {
                            // LineWrap になるのは行スコープの字下げ・地付きだけ（Lowerer の
                            // `LineKind::LineWrap` は Jisage / Chitsuki しか作らない）ので、
                            // どちらも block_open_tag が Some を返す。ここは到達しない。
                            debug_assert!(false, "LineWrap に div 非包みの種類が来た: {kind:?}");
                        }
                    }
                }
                self.render_inlines(inline, &mut line);
                if self.in_tail {
                    // 後付けは閉じタグを出さない（参照 tail_output）。
                    self.push_tail_line(&line, out);
                    return;
                }
                for _ in 0..opened {
                    line.push_str("</div>");
                }
                line.push_str("\r\n");
                out.push_str(&line);
            }
        }
    }

    /// 後付けの 1 行を出す。行末 `<br />` は [`tail_line_needs_br`] が決める。
    fn push_tail_line(&self, line: &str, out: &mut String) {
        out.push_str(line);
        if tail_line_needs_br(line) {
            out.push_str("<br />");
        }
        out.push_str("\r\n");
    }

    fn render_nested(
        &mut self,
        kind: &BlockKind,
        children: &[Block],
        close: CloseKind,
        open: OpenKind,
        out: &mut String,
    ) {
        // ぶら下げ（折り返し字下げ）は per-line モデル。外側 div を作らず、各内容行を
        // 個別に burasage div で包む（空行は素の `<br />`）。閉じは何も出さない。
        if let BlockKind::Burasage(geometry) = kind {
            self.render_burasage(*geometry, children, out);
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
            // 閉じ `</a></hN>` も互換メタデータに従う。ぶら下げの直下で閉じるときは
            // 参照が閉じタグをバッファへ積むので、その行が per-line の burasage div に
            // 包まれる（CloseKind::BurasageWrapped）。
            let close_tag = format!("</a></{tag}>");
            match close {
                CloseKind::BurasageWrapped(geometry) => {
                    let (margin, text_indent) = self.burasage_style(geometry);
                    out.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\">{close_tag}</div>\r\n"
                    ));
                }
                // 閉じの後ろに同じ行の本文が続くときは改行しない
                // （`［＃ここで窓中見出し終わり］あと` → `</a></h4>あと`）。
                CloseKind::NoBreak => out.push_str(&close_tag),
                _ => out.push_str(&format!("{close_tag}\r\n")),
            }
            return;
        }
        // 閉じタグの出力形は互換メタデータ（CloseKind）で決める。
        let close_nl = match close {
            CloseKind::NoBreak => "</div>".to_string(),
            CloseKind::Newline => "</div>\r\n".to_string(),
            CloseKind::BareBreak => "</div><br />\r\n".to_string(),
            // 閉じタグを外側ぶら下げの per-line div で包む（Lower 時に確定済み）。
            CloseKind::BurasageWrapped(geometry) => {
                let (margin, text_indent) = self.burasage_style(geometry);
                format!(
                    "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\"></div></div>\r\n"
                )
            }
        };
        // 開始タグ（旧 tag_generator の block 形と厳密一致）。複数行ブロックは開き
        // 直後に `\r\n` を出す（行スコープ包みとの違い）。None なら div で包まない。
        match block_open_tag(kind, self.options.quirks.empty_indent_css) {
            Some(open_tag) => {
                out.push_str(&open_tag);
                // 行の途中で開くブロックは同じ行に内容が続くので改行を出さない。
                if open == OpenKind::Newline {
                    out.push_str("\r\n");
                }
                for child in children {
                    self.render_block(child, out);
                }
                out.push_str(&close_nl);
            }
            None => {
                for child in children {
                    self.render_block(child, out);
                }
            }
        }
    }

    /// ぶら下げの `margin-left` と `text-indent`（参照 generate_burasage_start と同じ）。
    ///
    /// 折り返し幅が空（コンマなし記法）のとき、参照は `margin-left: em` という不正な
    /// CSS を出す（Quirk `empty_indent_css`）。オフなら妥当な `0em`。
    fn burasage_style(&self, geometry: BurasageGeometry) -> (String, i32) {
        let margin = geometry
            .wrap_width
            .map(|w| w.to_string())
            .unwrap_or_else(|| {
                if self.options.quirks.empty_indent_css {
                    String::new()
                } else {
                    "0".to_string()
                }
            });
        (margin, geometry.text_indent())
    }

    /// ぶら下げブロックを per-line で描画する（参照 generate_burasage_start）。
    ///
    /// 包むかどうかは [`has_inline_text`] で決める。
    /// 内容行は個別に burasage div で包み（内側 `<br />` なし）、空行は素の `<br />`。
    /// 行以外の子（行スコープ包み・入れ子ブロック・見出し等）は包まずそのまま描画する。
    fn render_burasage(
        &mut self,
        geometry: BurasageGeometry,
        children: &[Block],
        out: &mut String,
    ) {
        let (margin, text_indent) = self.burasage_style(geometry);
        for child in children {
            match child {
                // 参照 general_output と同順で判定する。まず包むか（blank_type==false）、
                // 次に `<br />` を出すか（terprip）。
                Block::Line { inline, .. } if has_inline_text(inline) => {
                    out.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\">"
                    ));
                    self.render_inlines(inline, out);
                    out.push_str("</div>\r\n");
                }
                // 包まない行のうち `@terprip=false` のもの（見出し行など）は、参照が
                // `<br />` ではなく `</div>` を出す（general_output の else）。開きが
                // 無いまま閉じるので div の収支は合わないが、参照がそう出力する。
                // 判定は Lower 時に済んでいるので `brk` を消費する（再導出しない）。
                Block::Line {
                    inline,
                    brk: Break::None,
                    ..
                } => {
                    self.render_inlines(inline, out);
                    out.push_str("</div>\r\n");
                }
                Block::Line { inline, .. } => {
                    // 本文テキストを持たない行は包まず、内容＋素の `<br />`。
                    // 空行はここで内容が空になるので `<br />` だけが出る。
                    self.render_inlines(inline, out);
                    out.push_str("<br />\r\n");
                }
                // 行スコープ字下げ・入れ子ブロック等は burasage で包まない。
                other => self.render_block(other, out),
            }
        }
    }
}

impl<'a> BlockRenderer<'a> {
    fn render_inlines(&mut self, inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            self.render_inline(inline, out);
        }
    }

    fn render_inline(&mut self, inline: &Inline, out: &mut String) {
        match &inline.kind {
            InlineKind::Text(s) => out.push_str(&html_escape(s)),
            // 未閉じ `〔` が行末に残した素の改行（参照 AccentParser#general_output）。
            InlineKind::UnclosedAccentBreak => out.push_str("<br />\r\n"),
            InlineKind::Style {
                children,
                style_type,
            } => {
                let tag = style_html_tag(*style_type);
                let class = style_css_class(*style_type);
                out.push_str(&format!("<{tag} class=\"{class}\">"));
                self.render_inlines(children, out);
                out.push_str(&format!("</{tag}>"));
            }
            InlineKind::Tcy { children } => self.wrap_span(children, "dir=\"ltr\"", out),
            InlineKind::Keigakomi { children } => {
                self.wrap_span(children, "class=\"keigakomi\"", out)
            }
            InlineKind::Yokogumi { children } => {
                self.wrap_span(children, "class=\"yokogumi\"", out)
            }
            InlineKind::Caption { children } => self.wrap_span(children, "class=\"caption\"", out),
            InlineKind::Warigaki { children } => {
                self.wrap_span(children, "class=\"warigaki\"", out)
            }
            InlineKind::Ruby {
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
            InlineKind::Gaiji {
                description,
                unicode,
                jis_code,
                had_igeta,
            } => {
                let s = self
                    .render_gaiji(
                        description,
                        unicode.as_deref(),
                        jis_code.as_deref(),
                        *had_igeta,
                    )
                    .into_html();
                out.push_str(&s);
            }
            InlineKind::Accent {
                code,
                name,
                unicode,
            } => {
                let s = self.render_accent(code, name, unicode.as_deref());
                out.push_str(&s);
            }
            InlineKind::Img {
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
            InlineKind::DakutenKatakana { num } => {
                let s = self.render_dakuten_katakana(num);
                out.push_str(&s);
            }
            InlineKind::ChitsukiInline { width, children } => {
                out.push_str(&format!(
                    "<div class=\"chitsuki_{width}\" style=\"text-align:right; margin-right: {width}em\">"
                ));
                self.render_inlines(children, out);
                // 後付けでは閉じない（参照 tail_output は閉じタグの配列を持たない）。
                if !self.in_tail {
                    out.push_str("</div>");
                }
            }
            InlineKind::BlockInline { kind, children } => {
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
                } else if let Some(open) =
                    block_open_tag(kind, self.options.quirks.empty_indent_css)
                {
                    out.push_str(&open);
                    self.render_inlines(children, out);
                    out.push_str("</div>");
                } else {
                    self.render_inlines(children, out);
                }
            }
            InlineKind::Kaeriten(text) => out.push_str(&format!(
                "<sub class=\"kaeriten\">{}</sub>",
                html_escape(text)
            )),
            InlineKind::Note { content, .. } => {
                self.notation.mark_notes();
                let inner = self.render_note_content(content);
                out.push_str(&format!("<span class=\"notes\">［＃{inner}］</span>"));
            }
            InlineKind::AnnotationEnd {
                prefix,
                content,
                suffix,
            } => {
                self.notation.mark_notes();
                let mut content_html = String::new();
                self.render_inlines(content, &mut content_html);
                out.push_str(&format!(
                    "<span class=\"notes\">［＃{}{}{}］</span>",
                    html_escape(prefix),
                    content_html,
                    html_escape(suffix)
                ));
            }
            InlineKind::Okurigana { content, .. } => {
                // 参照実装 Tag::Okurigana は注記と同じ描画を通し、外側 （ ） を除去する。
                let inner = self.render_note_content(content).replace(['（', '）'], "");
                out.push_str(&format!("<sup class=\"okurigana\">{inner}</sup>"));
            }
            InlineKind::FontSize {
                children,
                size_type,
                level,
            } => {
                let (class, style) = font_size_class_style(*size_type, *level);
                out.push_str(&format!("<span class=\"{class}\" style=\"{style}\">"));
                self.render_inlines(children, out);
                out.push_str("</span>");
            }
            InlineKind::Midashi {
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
            InlineKind::Warichu {
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

    /// 注記・送り仮名の中身（Lowerer が解決済み）を描画する。参照は別の TagParser で
    /// 処理するので、本文かどうかに関わらず外字記号 ※ を出す（in_tail/in_ruby_base を
    /// 一時的に倒す）。中身の深さは Lower 時に制限済み。
    fn render_note_content(&mut self, content: &[Inline]) -> String {
        let outer_tail = std::mem::replace(&mut self.in_tail, false);
        let outer_ruby_base = std::mem::replace(&mut self.in_ruby_base, false);
        let mut html = String::new();
        self.render_inlines(content, &mut html);
        self.in_ruby_base = outer_ruby_base;
        self.in_tail = outer_tail;
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

    /// アクセント文字（外字画像）を描画する（参照 `Tag::Accent#to_s`）。
    ///
    /// 参照が見るのは `use_jisx0213` **だけ**で、`use_unicode` はアクセントに効かない
    /// （`Tag::Accent` は `@unicode` を持たず、面区点から `JIS2UCS` を引くため）。
    /// 表に無い面区点のときは参照も `nil` を出す＝空文字列になる。
    fn render_accent(&mut self, code: &str, name: &str, unicode: Option<&str>) -> String {
        self.notation.mark_accent();
        if self.options.use_jisx0213 {
            unicode.map(numeric_entities).unwrap_or_default()
        } else {
            self.notation.mark_gaiji_image();
            self.gaiji_img(code, &html_escape(&self.accent_name_for_alt(name)))
        }
    }

    /// alt に出すアクセント文字の説明文。説明文そのものは
    /// [`crate::accent`] の表が持つので、ここでは quirk による訂正だけを行う
    /// （参照実装の表記ゆれ `A^` の字母落ちを、quirk オフなら補う）。
    fn accent_name_for_alt(&self, name: &str) -> String {
        if !self.options.quirks.accent_name_typos && name == "サーカムフレックスアクセント付き"
        {
            return "サーカムフレックスアクセント付きA".to_string();
        }
        name.to_string()
    }

    fn gaiji_mark_prefix(&self) -> &'static str {
        if self.in_tail || self.in_ruby_base {
            ""
        } else {
            GAIJI_MARK_STR
        }
    }

    /// ルビ親文字を描画し、画像化できない外字は rb に記号 ※ だけ残して注記を
    /// trailing（ルビ外）へ振り分ける（参照 render_ruby_base 相当）。
    /// 返り値は (rb の中身, ルビの後ろに続く注記列)。
    fn render_ruby_base(&mut self, base: &[Inline]) -> (String, String) {
        let mut rb = String::new();
        let mut trailing = String::new();
        // 親文字の中では注記に ※ を付けない（rb 側に自分で付けるため）。
        let outer = std::mem::replace(&mut self.in_ruby_base, true);
        for inline in base {
            let InlineKind::Gaiji {
                description,
                unicode,
                jis_code,
                had_igeta,
            } = &inline.kind
            else {
                self.render_inline(inline, &mut rb);
                continue;
            };
            match self.render_gaiji(
                description,
                unicode.as_deref(),
                jis_code.as_deref(),
                *had_igeta,
            ) {
                GaijiHtml::Note(html) => {
                    rb.push_str(GAIJI_MARK_STR);
                    trailing.push_str(&html);
                }
                GaijiHtml::Direct(html) => rb.push_str(&html),
            }
        }
        self.in_ruby_base = outer;
        (rb, trailing)
    }

    /// 外字画像 `<img class="gaiji" />`（参照 `Tag::EmbedGaiji#to_s` / `Tag::Accent#to_s`）。
    fn gaiji_img(&self, jis_code: &str, alt: &str) -> String {
        let (folder, file) = jis_code_to_path(jis_code);
        format!(
            "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
            self.options.gaiji_dir, folder, file, alt
        )
    }

    /// 濁点付き片仮名（`ワ゛［＃1-7-82］`）の外字画像（参照 `Tag::DakutenKatakana#to_s`）。
    ///
    /// 通常の外字と違い、参照はフッタ「表記について」に何も積まない
    /// （`Tag::DakutenKatakana` は `Tag::Inline` 派生で `@chuuki_table` を触らない）。
    /// src の組み立ても専用で、`@gaiji_dir` の末尾 `/` に続けてもう1つ `/` を置く
    /// （Quirk [`crate::html::Quirks::dakuten_katakana_double_slash`]）。
    fn render_dakuten_katakana(&self, num: &str) -> String {
        let sep = if self.options.quirks.dakuten_katakana_double_slash {
            "/"
        } else {
            ""
        };
        format!(
            "<img src=\"{}{sep}1-07/1-07-8{num}.png\" alt=\"※(濁点付き片仮名「{}」、1-07-8{num})\" class=\"gaiji\" />",
            self.options.gaiji_dir,
            crate::node::Node::dakuten_katakana_char(num),
        )
    }

    /// 画像化も実体参照化もできなかった外字（`※［＃…］` の注記）。
    /// フッタの外字一覧にも積む（参照 `@images`）。
    fn unconvertible_note(&mut self, description: &str, had_igeta: bool) -> String {
        self.notation.add_unconverted_gaiji(description, had_igeta);
        let notes_mark = if had_igeta { "＃" } else { "" };
        format!(
            "{}<span class=\"notes\">［{}{}］</span>",
            self.gaiji_mark_prefix(),
            notes_mark,
            html_escape(description)
        )
    }

    /// 外字をHTMLに変換（参照 `Tag::EmbedGaiji#to_s`）。
    ///
    /// AST に解決済みのコードがあればそれを使い、無ければ説明文から引き直す。
    /// どちらも [`GaijiResult`] に正規化してから1箇所で描画する。
    fn render_gaiji(
        &mut self,
        description: &str,
        unicode: Option<&str>,
        jis_code: Option<&str>,
        had_igeta: bool,
    ) -> GaijiHtml {
        let resolved = match (unicode, jis_code) {
            (Some(u), Some(jis)) => GaijiResult::JisConverted {
                jis_code: jis.to_string(),
                unicode: u.to_string(),
            },
            (Some(u), None) => GaijiResult::Unicode(u.to_string()),
            (None, Some(jis)) => GaijiResult::JisImage {
                jis_code: jis.to_string(),
            },
            (None, None) => parse_gaiji(description),
        };
        self.render_gaiji_result(resolved, description, had_igeta)
    }

    /// 正規化済みの外字解決結果を描画する。
    ///
    /// 参照 `Tag::EmbedGaiji#to_s` は JIS コード（`use_jisx0213`）と `U+` 指定
    /// （`use_unicode`）を**別のオプション**で切り替え、片方から他方へは落ちない。
    /// JIS コードしか無い外字は `--use-unicode` でも画像のままになる。
    fn render_gaiji_result(
        &mut self,
        resolved: GaijiResult,
        description: &str,
        had_igeta: bool,
    ) -> GaijiHtml {
        match resolved {
            // `U+` 指定のみ。参照は use_unicode のときだけ実体参照にする。
            GaijiResult::Unicode(u) => {
                if self.options.use_unicode {
                    GaijiHtml::Direct(numeric_entities(&u))
                } else {
                    GaijiHtml::Note(self.unconvertible_note(description, had_igeta))
                }
            }
            // JIS コードから Unicode が引けた外字。参照は use_jisx0213 のときだけ
            // 実体参照にする（use_unicode では画像のまま）。
            GaijiResult::JisConverted { jis_code, unicode } => {
                self.notation.mark_jisx0213();
                if self.options.use_jisx0213 {
                    GaijiHtml::Direct(numeric_entities(&unicode))
                } else {
                    self.notation.mark_gaiji_image();
                    GaijiHtml::Direct(self.gaiji_img_with_alt(&jis_code, description, had_igeta))
                }
            }
            // Unicode が引けないので常に画像。
            GaijiResult::JisImage { jis_code } => {
                self.notation.mark_jisx0213();
                self.notation.mark_gaiji_image();
                GaijiHtml::Direct(self.gaiji_img_with_alt(&jis_code, description, had_igeta))
            }
            GaijiResult::Unconvertible => {
                GaijiHtml::Note(self.unconvertible_note(description, had_igeta))
            }
        }
    }

    /// 外字画像を alt 付きで組み立てる。alt は `＃` 付きの外字注記のときだけ入る
    /// （[`Self::gaiji_alt`] は入れ子外字を展開しうるので、必ずフッタ用フラグを
    /// 立てた**後**に呼ぶ。参照実装と登録順を合わせるため）。
    fn gaiji_img_with_alt(&mut self, jis_code: &str, description: &str, had_igeta: bool) -> String {
        let alt = if had_igeta {
            self.gaiji_alt(description)
        } else {
            String::new()
        };
        self.gaiji_img(jis_code, &alt)
    }

    /// 外字画像の alt を作る。alt の中の入れ子外字 `※［＃…］` は、参照実装が
    /// `<img>` に展開する（属性値の中にタグが入る不正な HTML。Quirk
    /// `nested_gaiji_in_alt`）。記法の切り分けは [`split_nested_gaiji`] に委ねる。
    fn gaiji_alt(&mut self, description: &str) -> String {
        let stripped = strip_kuten_prefix(description);
        if !self.options.quirks.nested_gaiji_in_alt || self.alt_depth >= MAX_ALT_DEPTH {
            return html_escape(&stripped);
        }
        self.alt_depth += 1;
        let mut out = String::new();
        for segment in split_nested_gaiji(&stripped) {
            match segment {
                NestedGaijiSegment::Text(text) => out.push_str(&html_escape(text)),
                NestedGaijiSegment::Gaiji(inner) => {
                    let html = self.render_gaiji(inner, None, None, true).into_html();
                    out.push_str(&html);
                }
            }
        }
        self.alt_depth -= 1;
        out
    }

    /// 画像注記 alt 内の外字を外字一覧・表記フラグに登録する（出力に影響しない）。
    ///
    /// alt の**文字列**は生文字（参照 TagParser の @raw）由来なので
    /// [`Self::gaiji_alt`] が `※［＃` の素の走査で切るのに対し、こちらは alt を
    /// 読む TagParser が @images を共有する副作用の再現なので、本文と同じ
    /// [`tokenize`] で拾う（＃無しの `※［...］` も外字として登録される）。
    /// 2つの経路が違うのは参照実装がこの2つを別物として持っているため
    /// （docs/workflow.md「画像注記 alt 内の外字を外字一覧に登録」）。
    fn register_alt_gaiji(&mut self, alt: &str) {
        for token in tokenize(alt) {
            let TokenKind::Gaiji {
                description,
                had_igeta,
            } = token.kind
            else {
                continue;
            };
            match parse_gaiji(&description) {
                GaijiResult::JisImage { .. } | GaijiResult::JisConverted { .. } => {
                    self.notation.mark_jisx0213();
                    self.notation.mark_gaiji_image();
                }
                GaijiResult::Unconvertible => {
                    self.notation.add_unconverted_gaiji(&description, had_igeta);
                }
                GaijiResult::Unicode(_) => {
                    if !self.options.use_unicode {
                        self.notation.add_unconverted_gaiji(&description, had_igeta);
                    }
                }
            }
        }
    }
}

/// 行が「本文テキスト」を持つか（参照実装 `TextBuffer#blank_type == false`）。
///
/// 参照実装のぶら下げは、バッファに**空でない String** がある行だけを per-line の
/// burasage div で包む。分かれ目は注記の書き方で、参照実装に最小入力を与えて
/// 見出し・太字・傍点・縦中横・罫囲み・横組み・キャプション・外字・アクセント・
/// 返り点・送り仮名で実測した:
///
/// - **範囲形**（`［＃中見出し］abc［＃中見出し終わり］`）は中身が String のまま
///   バッファに残る → **包む**。[`Inline::range_form`] が立つ。
/// - **後方参照形**（`［＃「abc」は中見出し］`）は String をタグへ取り込んで消す
///   → **包まない**。画像・ルビ（親文字を取り込む）・注記も同じく残さない。
///
/// 行全体がブロック div になる行（行スコープの字下げ・地付き）は、そもそも
/// [`Block::Line`] ではなく [`Block::LineWrap`] になるのでこの判定には来ない
/// （参照の `blank_type == :inline` に相当し、包まず行末は `\r\n` だけ）。
fn has_inline_text(inlines: &[Inline]) -> bool {
    inlines.iter().any(inline_has_text)
}

/// [`has_inline_text`] の1要素版。
///
/// **`_` の catch-all を置かないこと。** 置くと [`InlineKind`] に変種を足したとき
/// コンパイラが黙って「本文あり」を選び、ぶら下げの包み判定が静かにずれる
/// （実際に罫囲み・横組み・キャプション・外字・アクセント・返り点・送り仮名の
/// 7種類がこれで誤判定していた）。
fn inline_has_text(inline: &Inline) -> bool {
    match &inline.kind {
        // 素の String。空文字列はバッファに何も残さない。
        InlineKind::Text(s) => !s.is_empty(),
        // 参照 apply_warichu は状態を持たず、開閉を素の文字列でバッファに積む。
        InlineKind::Warichu { .. } => true,
        // 未閉じ `〔` が残す `"<br />\r\n"` も素の String としてバッファに載る。
        InlineKind::UnclosedAccentBreak => true,
        // 範囲形（`［＃中見出し］…［＃中見出し終わり］`）は、参照実装が**開始タグの
        // 文字列そのもの**を push_char でバッファへ積むので、中身が空でも String が
        // 残る（`［＃傍点］［＃傍点終わり］` もぶら下げに包まれる。実測）。
        // 後方参照形（`［＃「…」は中見出し］`）は String をタグに取り込むので残さない。
        InlineKind::Style { children, .. }
        | InlineKind::Midashi { children, .. }
        | InlineKind::FontSize { children, .. }
        | InlineKind::Tcy { children }
        | InlineKind::Keigakomi { children }
        | InlineKind::Yokogumi { children }
        | InlineKind::Caption { children }
        | InlineKind::Warigaki { children }
        // 同一行で開閉するブロック形コマンドは、旧経路の BlockStart/BlockEnd に
        // 対応するブロックマーカー。範囲形なので中身の String は残る。
        | InlineKind::BlockInline { children, .. } => {
            let _ = children;
            inline.range_form
        }
        // 参照実装で `Tag::Inline` 系。バッファに String を残さない
        // （ルビ・画像・外字・アクセント・返り点・送り仮名・注記は自分でタグになる）。
        // 行スコープ地付き（ChitsukiInline）は旧経路の LineJisage 相当のマーカー。
        InlineKind::Ruby { .. }
        | InlineKind::Img { .. }
        | InlineKind::Gaiji { .. }
        | InlineKind::Accent { .. }
        | InlineKind::Kaeriten(_)
        | InlineKind::Okurigana { .. }
        | InlineKind::Note { .. }
        | InlineKind::AnnotationEnd { .. }
        | InlineKind::DakutenKatakana { .. }
        | InlineKind::ChitsukiInline { .. } => false,
    }
}

/// 単一行のインラインHTML（行末 `<br />`/`\r\n` なし）を新経路で描画する。
/// 旧 `HtmlRenderer::render_line` のAozora AST版（インライン列のみ）。
pub fn render_line_inline(line: &str, options: &RenderOptions) -> String {
    use crate::parser::parse;
    use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};

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
    use crate::document::extract_body_lines;
    use crate::html::{convert, RenderOptions};
    use crate::lower::lower_to_blocks;
    use crate::parser::parse_document_raw;

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

        assert_eq!(
            new_inner, old_inner,
            "\n新:{new_inner:?}
旧:{old_inner:?}"
        );
    }

    /// ぶら下げの per-line 包みは、参照 `TextBuffer#blank_type` と同じく
    /// 「バッファに空でない String が残るか」で決まる。範囲形の注記は中身を素の
    /// String として残すので**包む**が、後方参照形はタグに取り込むので包まない。
    ///
    /// 参照実装で実測（見出し・太字・傍点・縦中横で同じ結果）:
    /// - `［＃中見出し］abc［＃中見出し終わり］` → `<div class="burasage">…</div>`
    /// - `abc［＃「abc」は中見出し］`             → 包まず `</h4></div>`
    #[test]
    fn burasage_wraps_range_form_notations_but_not_backreferences() {
        let render = |line: &str| {
            let lines = vec!["［＃ここから５字下げ、折り返して７字下げ］", line];
            let blocks = lower_to_blocks(&parse_document_raw(&lines));
            BlockRenderer::new(&RenderOptions::default()).render_body(&blocks)
        };
        for range_form in [
            // 中身が空でも包む。参照は開始タグの文字列そのものをバッファへ積むので、
            // String が残る（実測）。
            "［＃傍点］［＃傍点終わり］",
            "［＃中見出し］［＃中見出し終わり］",
            "［＃中見出し］abc［＃中見出し終わり］",
            "［＃ここから太字］abc［＃ここで太字終わり］",
            "［＃縦中横］32［＃縦中横終わり］",
            "［＃罫囲み］なにぬ［＃罫囲み終わり］",
            "［＃横組み］はひふ［＃横組み終わり］",
        ] {
            let html = render(range_form);
            assert!(
                html.starts_with("<div class=\"burasage\""),
                "範囲形は包む: {range_form} → {html:?}"
            );
        }
        // 後方参照形と、参照実装で `Tag::Inline` になる記法は包まない。
        // いずれも参照実装に最小入力を与えて実測した（decimal な `_ => true` の
        // catch-all があった頃はここが全部「包む」に倒れていた）。
        for backref in [
            "abc［＃「abc」は中見出し］",
            "abc［＃「abc」は太字］",
            "あいう［＃「あいう」は罫囲み］",
            "かきく［＃「かきく」は横組み］",
            "あいう［＃「あいう」はキャプション］",
            "〔e'〕",
            "［＃（し）］",
            "［＃レ］",
            "※［＃「口＋世」、第3水準1-15-6］",
            "※［＃ゴシック体のハ、U+2F0B］",
        ] {
            let html = render(backref);
            assert!(
                !html.starts_with("<div class=\"burasage\""),
                "包まない: {backref} → {html:?}"
            );
        }
    }

    /// 濁点付き片仮名（`ワ゛［＃1-7-82］`）は直前の `ワ゛`〜`ヲ゛` を消費して外字画像に
    /// なる（参照 `apply_dakuten_katakana` ＋ `Tag::DakutenKatakana#to_s`）。
    ///
    /// 参照実装で実測した性質:
    /// - src は `{gaiji_dir}/1-07/…` で組まれ、gaiji_dir 末尾の `/` と重なって二重になる
    /// - alt の面区点は `1-07-8N` とゼロ詰め（注記中の表記 `1-7-8N` とは別）
    /// - 前方に対象が無ければ解決せず、注記 `［＃1-7-82］` のまま出る
    /// - `※［＃濁点付き片仮名ワ、1-7-82］`（外字記法）は通常の外字経路で、別物
    #[test]
    fn dakuten_katakana_consumes_front_reference() {
        let opts = RenderOptions::default();
        assert_eq!(
            render_line_inline("ワ゛［＃1-7-82］", &opts),
            "<img src=\"../../../gaiji//1-07/1-07-82.png\" \
             alt=\"※(濁点付き片仮名「ワ゛」、1-07-82)\" class=\"gaiji\" />"
        );
        // 説明付きでも面区点が入っていれば同じ経路（参照は位置を問わず判定する）。
        assert_eq!(
            render_line_inline("ヱ゛［＃濁点付き片仮名、1-7-84］", &opts),
            "<img src=\"../../../gaiji//1-07/1-07-84.png\" \
             alt=\"※(濁点付き片仮名「ヱ゛」、1-07-84)\" class=\"gaiji\" />"
        );
        // 対象が前方に無ければ注記のまま（参照 apply_rest_notes へのフォールバック）。
        assert_eq!(
            render_line_inline("ワ［＃1-7-82］", &opts),
            "ワ<span class=\"notes\">［＃1-7-82］</span>"
        );
        // quirk を切るとスラッシュが一重になる。
        let clean = RenderOptions {
            quirks: crate::html::Quirks::none(),
            ..RenderOptions::default()
        };
        assert!(render_line_inline("ワ゛［＃1-7-82］", &clean)
            .contains("src=\"../../../gaiji/1-07/1-07-82.png\""));
    }

    /// 外字・アクセントの実体参照は**大文字16進・最低4桁**（参照 `yml/jis2ucs.yml`
    /// と `Tag::EmbedGaiji#to_s` の `"&#x#{@unicode};"`）。
    ///
    /// またオプションの切り分けは参照 `Tag::EmbedGaiji#to_s` / `Tag::Accent#to_s`
    /// と同じで、JIS コード側は `use_jisx0213`、`U+` 指定側は `use_unicode` だけが
    /// 効く。アクセントは `use_unicode` を見ない。以下はすべて参照実装で実測した。
    #[test]
    fn gaiji_and_accent_entities_follow_reference_options() {
        let opts = |jisx0213: bool, unicode: bool| RenderOptions {
            use_jisx0213: jisx0213,
            use_unicode: unicode,
            ..RenderOptions::default()
        };
        // JIS コードのみの外字: use_jisx0213 でだけ実体参照、use_unicode では画像。
        let jis = "※［＃「口＋世」、第3水準1-15-6］";
        assert_eq!(render_line_inline(jis, &opts(true, false)), "&#x5535;");
        assert!(render_line_inline(jis, &opts(false, true)).starts_with("<img "));
        assert!(render_line_inline(jis, &opts(false, false)).starts_with("<img "));
        // `U+` 指定の外字: use_unicode でだけ実体参照、use_jisx0213 では注記。
        let ucs = "※［＃ゴシック体のハ、U+2F0B］";
        assert_eq!(render_line_inline(ucs, &opts(false, true)), "&#x2F0B;");
        assert!(render_line_inline(ucs, &opts(true, false)).contains("<span class=\"notes\">"));
        // アクセント: use_jisx0213 でだけ実体参照。use_unicode は効かない。
        assert_eq!(render_line_inline("〔e'〕", &opts(true, false)), "&#x00E9;");
        assert!(render_line_inline("〔e'〕", &opts(false, true)).starts_with("<img "));
    }

    /// ぶら下げが明示的に閉じられる行は、参照がその行の出力前にぶら下げを
    /// indent_stack から降ろすので per-line の包みが効かない。行末も出さないため、
    /// 続く行の内容が同じ出力行に繋がる（参照実装で実測）。
    ///
    /// 例: 001699/57254 の `(*20) full beard …髭をいいます。［＃ここで字下げ終わり］`
    #[test]
    fn burasage_line_with_explicit_close_is_not_wrapped() {
        let lines = vec![
            "［＃ここから３字下げ、折り返して４字下げ］",
            "注1。",
            "注2。［＃ここで字下げ終わり］",
            "　次。",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        let html = BlockRenderer::new(&RenderOptions::default()).render_body(&blocks);
        assert_eq!(
            html,
            concat!(
                "<div class=\"burasage\" style=\"margin-left: 4em; text-indent: -1em;\">注1。</div>\r\n",
                "注2。　次。<br />\r\n"
            )
        );
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

    /// 後付け（底本情報）は参照 `tail_output` が本文とは別の規則で描く。
    /// 行末 `<br />` は**描画済みの行文字列**で決まり、閉じタグは出さない。
    /// 以下はすべて参照実装に同じ入力を与えて実測した。
    #[test]
    fn tail_section_follows_tail_output_rules() {
        let convert_tail = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n本文。\r\n\r\n底本：「テスト」\r\n{line}\r\n");
            let html = convert(&src, &RenderOptions::default());
            let open = "<div class=\"bibliographical_information\">";
            let start = html.find(open).expect("底本セクション") + open.len();
            let end = html[start..].find("</div>").expect("閉じ") + start;
            html[start..end].to_string()
        };

        // 行がまるごと 1 つのタグ（^<[^>]*>$）→ <br /> を足さない。
        let gaiji = convert_tail("※［＃「口＋世」、第3水準1-15-6］");
        assert!(
            gaiji.contains("class=\"gaiji\" />\r\n"),
            "外字だけの行に <br /> を足さない: {gaiji:?}"
        );
        // </h\d>$ → 足さない。
        let midashi = convert_tail("あいう［＃「あいう」は同行大見出し］");
        assert!(
            midashi.contains("</h3>\r\n"),
            "見出しで終わる行に <br /> を足さない: {midashi:?}"
        );
        // 行スコープ字下げは開いたまま（閉じタグを出さない）。<div.*>$ に当たるので
        // <br /> も足さない。
        let jisage = convert_tail("［＃２字下げ］あいう");
        assert!(
            jisage.contains("<div class=\"jisage_2\" style=\"margin-left: 2em\">あいう<br />"),
            "後付けの行スコープ字下げは閉じない: {jisage:?}"
        );
        // 行内地付きも閉じない。
        let chitsuki = convert_tail("あいう［＃地付き］");
        assert!(
            chitsuki.contains("margin-right: 0em\">\r\n"),
            "後付けの地付きは閉じない: {chitsuki:?}"
        );
        // 普通の本文行にはこれまでどおり <br /> を足す。
        let plain = convert_tail("ただの行。");
        assert!(plain.contains("ただの行。<br />"), "{plain:?}");
    }

    /// ぶら下げの直下で見出しブロックが閉じる行は、閉じタグ `</a></hN>` が
    /// per-line の burasage div に包まれる。
    ///
    /// 参照 explicit_close は @tag_stack から取り出した閉じタグを push_chars で
    /// バッファへ積むので String が残り、ぶら下げの包み判定（TextBuffer#blank_type）
    /// に入る。装飾系ブロック（罫囲み等）と同じ扱いで、見出しだけ外れていた。
    /// 期待値は参照実装で実測した。
    #[test]
    fn midashi_block_close_is_wrapped_by_burasage() {
        let src = concat!(
            "作品名\r\n著者\r\n\r\n",
            "［＃ここから２字下げ、折り返して４字下げ］\r\n",
            "外側。\r\n",
            "［＃ここから中見出し］\r\n",
            "内側。\r\n",
            "［＃ここで中見出し終わり］\r\n",
            "［＃ここで字下げ終わり］\r\n",
            "\r\n底本：「テスト」\r\n",
        );
        let html = convert(src, &RenderOptions::default());
        assert!(
            html.contains(
                "<div class=\"burasage\" style=\"margin-left: 4em; text-indent: -2em;\">\
                 </a></h4></div>"
            ),
            "見出しブロックの閉じがぶら下げに包まれない: {html}"
        );
    }
}
