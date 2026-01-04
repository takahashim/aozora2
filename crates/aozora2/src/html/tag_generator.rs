//! HTMLタグ生成
//!
//! ブロック要素のHTMLタグを生成する純粋関数を提供します。

use aozora_core::node::{BlockParams, BlockType, MidashiLevel, MidashiStyle};

use super::presentation::{
    midashi_combined_css_class, midashi_html_tag, style_css_class, style_html_tag,
};

/// ブロック開始タグを生成
///
/// 見出しの場合は `midashi_id` を使用してアンカーIDを生成します。
pub fn generate_block_start_tag(
    block_type: &BlockType,
    params: &BlockParams,
    midashi_id: Option<u32>,
) -> String {
    match block_type {
        BlockType::Jisage => generate_jisage_start(params),
        BlockType::Chitsuki => generate_chitsuki_start(params),
        BlockType::Jizume => generate_jizume_start(params),
        BlockType::Keigakomi => generate_keigakomi_start(params),
        BlockType::Midashi => generate_midashi_start(params, midashi_id.unwrap_or(0)),
        BlockType::Yokogumi => generate_yokogumi_start(params),
        BlockType::Futoji => "<div class=\"futoji\">".to_string(),
        BlockType::Shatai => "<div class=\"shatai\">".to_string(),
        BlockType::FontDai => generate_font_dai_start(params),
        BlockType::FontSho => generate_font_sho_start(params),
        BlockType::Tcy => "<span dir=\"ltr\">".to_string(),
        BlockType::Caption => generate_caption_start(params),
        BlockType::Warigaki => generate_warigaki_start(params),
        BlockType::Burasage => generate_burasage_start(params),
        BlockType::Style => generate_style_block_start(params),
        // 注記付き範囲はパース段階でRubyノードに解決されるので、ここには来ない
        BlockType::AnnotationRange | BlockType::LeftAnnotationRange => String::new(),
    }
}

/// ブロック終了タグを生成
pub fn generate_block_end_tag(block_type: &BlockType, params: &BlockParams) -> String {
    match block_type {
        BlockType::Jisage
        | BlockType::Chitsuki
        | BlockType::Jizume
        | BlockType::Futoji
        | BlockType::Shatai
        | BlockType::Burasage => "</div>".to_string(),
        BlockType::Keigakomi => generate_keigakomi_end(params),
        BlockType::Yokogumi => generate_yokogumi_end(params),
        BlockType::Midashi => generate_midashi_end(params),
        BlockType::FontDai | BlockType::FontSho => generate_font_end(params),
        BlockType::Tcy => "</span>".to_string(),
        BlockType::Caption => generate_caption_end(params),
        BlockType::Warigaki => generate_warigaki_end(params),
        BlockType::Style => generate_style_block_end(params),
        // 注記付き範囲はパース段階でRubyノードに解決されるので、ここには来ない
        BlockType::AnnotationRange | BlockType::LeftAnnotationRange => String::new(),
    }
}

// 個別タグ生成関数

fn generate_jisage_start(params: &BlockParams) -> String {
    if let Some(width) = params.width {
        format!("<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">")
    } else {
        "<div class=\"jisage\">".to_string()
    }
}

fn generate_chitsuki_start(params: &BlockParams) -> String {
    let width = params.width.unwrap_or(0);
    format!("<div class=\"chitsuki_{width}\" style=\"text-align:right; margin-right: {width}em\">")
}

fn generate_jizume_start(params: &BlockParams) -> String {
    if let Some(width) = params.width {
        format!("<div class=\"jizume_{width}\" style=\"width: {width}em\">")
    } else {
        "<div class=\"jizume\">".to_string()
    }
}

fn generate_keigakomi_start(params: &BlockParams) -> String {
    if params.is_block {
        "<div class=\"keigakomi\" style=\"border: solid 1px\">".to_string()
    } else {
        "<span class=\"keigakomi\">".to_string()
    }
}

fn generate_keigakomi_end(params: &BlockParams) -> String {
    if params.is_block {
        "</div>".to_string()
    } else {
        "</span>".to_string()
    }
}

fn generate_yokogumi_start(params: &BlockParams) -> String {
    if params.is_block {
        "<div class=\"yokogumi\">".to_string()
    } else {
        "<span class=\"yokogumi\">".to_string()
    }
}

fn generate_yokogumi_end(params: &BlockParams) -> String {
    if params.is_block {
        "</div>".to_string()
    } else {
        "</span>".to_string()
    }
}

fn generate_midashi_start(params: &BlockParams, midashi_id: u32) -> String {
    let level = params.level.unwrap_or(MidashiLevel::O);
    let style = params.midashi_style.unwrap_or(MidashiStyle::Normal);
    let tag = midashi_html_tag(level);
    let class = midashi_combined_css_class(level, style);
    format!("<{tag} class=\"{class}\"><a class=\"midashi_anchor\" id=\"midashi{midashi_id}\">")
}

fn generate_midashi_end(params: &BlockParams) -> String {
    let level = params.level.unwrap_or(MidashiLevel::O);
    format!("</a></{}>", midashi_html_tag(level))
}

fn generate_font_dai_start(params: &BlockParams) -> String {
    let size = params.font_size.unwrap_or(1);
    let style = match size {
        1 => "large",
        2 => "x-large",
        _ => "xx-large",
    };
    let tag = if params.is_block { "div" } else { "span" };
    format!("<{tag} class=\"dai{size}\" style=\"font-size: {style};\">")
}

fn generate_font_sho_start(params: &BlockParams) -> String {
    let size = params.font_size.unwrap_or(1);
    let style = match size {
        1 => "small",
        2 => "x-small",
        _ => "xx-small",
    };
    let tag = if params.is_block { "div" } else { "span" };
    format!("<{tag} class=\"sho{size}\" style=\"font-size: {style};\">")
}

fn generate_font_end(params: &BlockParams) -> String {
    if params.is_block {
        "</div>".to_string()
    } else {
        "</span>".to_string()
    }
}

fn generate_caption_start(params: &BlockParams) -> String {
    if params.is_block {
        "<div class=\"caption\">".to_string()
    } else {
        "<span class=\"caption\">".to_string()
    }
}

fn generate_caption_end(params: &BlockParams) -> String {
    if params.is_block {
        "</div>".to_string()
    } else {
        "</span>".to_string()
    }
}

fn generate_warigaki_start(params: &BlockParams) -> String {
    let open_paren = if params.has_open_paren { "" } else { "（" };
    format!("<span class=\"warichu\">{open_paren}")
}

fn generate_warigaki_end(params: &BlockParams) -> String {
    let close_paren = if params.has_close_paren { "" } else { "）" };
    format!("{close_paren}</span>")
}

fn generate_burasage_start(params: &BlockParams) -> String {
    let wrap_width = params.wrap_width.unwrap_or(1);
    let width = params.width.unwrap_or(0);
    let text_indent = width as i32 - wrap_width as i32;
    format!(
        "<div class=\"burasage\" style=\"margin-left: {wrap_width}em; text-indent: {text_indent}em;\">"
    )
}

fn generate_style_block_start(params: &BlockParams) -> String {
    if let Some(style_type) = params.style_type {
        let tag = style_html_tag(style_type);
        let class = style_css_class(style_type);
        format!("<{tag} class=\"{class}\">")
    } else {
        "<span>".to_string()
    }
}

fn generate_style_block_end(params: &BlockParams) -> String {
    if let Some(style_type) = params.style_type {
        let tag = style_html_tag(style_type);
        format!("</{tag}>")
    } else {
        "</span>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_jisage_start() {
        let params = BlockParams {
            width: Some(2),
            ..Default::default()
        };
        let tag = generate_block_start_tag(&BlockType::Jisage, &params, None);
        assert_eq!(tag, "<div class=\"jisage_2\" style=\"margin-left: 2em\">");
    }

    #[test]
    fn test_generate_caption_start_block() {
        let params = BlockParams {
            is_block: true,
            ..Default::default()
        };
        let tag = generate_block_start_tag(&BlockType::Caption, &params, None);
        assert_eq!(tag, "<div class=\"caption\">");
    }

    #[test]
    fn test_generate_caption_start_inline() {
        let params = BlockParams::default();
        let tag = generate_block_start_tag(&BlockType::Caption, &params, None);
        assert_eq!(tag, "<span class=\"caption\">");
    }

    #[test]
    fn test_generate_block_end() {
        let params = BlockParams::default();
        assert_eq!(
            generate_block_end_tag(&BlockType::Jisage, &params),
            "</div>"
        );
        assert_eq!(generate_block_end_tag(&BlockType::Tcy, &params), "</span>");
    }
}
