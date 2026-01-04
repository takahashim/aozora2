//! プレゼンテーションロジック
//!
//! CSSクラス名とHTMLタグ名のマッピングを提供します。

use aozora_core::node::{MidashiLevel, MidashiStyle, StyleType};

/// 行のHTML出力タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    /// 空行
    Empty,
    /// ブロック要素（div, h3-h5など）- brタグ不要、ぶら下げラップ不要
    Block,
    /// インラインコンテンツ - brタグ必要、ぶら下げラップ可能
    Inline,
}

/// HTMLの行タイプを判定
pub fn classify_line(html: &str) -> LineType {
    if html.is_empty() {
        return LineType::Empty;
    }

    // ブロック要素の開始/終了で終わる場合
    if html.starts_with("<div class=\"")
        || html.starts_with("<h3")
        || html.starts_with("<h4")
        || html.starts_with("<h5")
        || html.ends_with("</div>")
        || html.ends_with("</h3>")
        || html.ends_with("</h4>")
        || html.ends_with("</h5>")
    {
        return LineType::Block;
    }

    LineType::Inline
}

/// StyleType のCSSクラス名を取得
pub fn style_css_class(style_type: StyleType) -> &'static str {
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
        StyleType::SesameDotAfter => "sesame_dot_after",
        StyleType::WhiteSesameDotAfter => "white_sesame_dot_after",
        StyleType::BlackCircleAfter => "black_circle_after",
        StyleType::WhiteCircleAfter => "white_circle_after",
        StyleType::BlackTriangleAfter => "black_up-pointing_triangle_after",
        StyleType::WhiteTriangleAfter => "white_up-pointing_triangle_after",
        StyleType::BullseyeAfter => "bullseye_after",
        StyleType::FisheyeAfter => "fisheye_after",
        StyleType::SaltireAfter => "saltire_after",
        StyleType::UnderlineSolid => "underline_solid",
        StyleType::UnderlineDouble => "underline_double",
        StyleType::UnderlineDotted => "underline_dotted",
        StyleType::UnderlineDashed => "underline_dashed",
        StyleType::UnderlineWave => "underline_wave",
        StyleType::OverlineSolid => "overline_solid",
        StyleType::OverlineDouble => "overline_double",
        StyleType::OverlineDotted => "overline_dotted",
        StyleType::OverlineDashed => "overline_dashed",
        StyleType::OverlineWave => "overline_wave",
        StyleType::Bold => "futoji",
        StyleType::Italic => "shatai",
        StyleType::Subscript => "subscript",
        StyleType::Superscript => "superscript",
    }
}

/// StyleType のHTMLタグ名を取得
pub fn style_html_tag(style_type: StyleType) -> &'static str {
    match style_type {
        StyleType::Subscript => "sub",
        StyleType::Superscript => "sup",
        StyleType::Bold | StyleType::Italic => "span",
        _ => "em", // すべての傍点・傍線は<em>タグを使用
    }
}

/// MidashiLevel と MidashiStyle から結合CSSクラス名を取得
/// Ruby版と同じ形式: dogyo-o-midashi, mado-naka-midashi など
pub fn midashi_combined_css_class(level: MidashiLevel, style: MidashiStyle) -> String {
    let level_str = match level {
        MidashiLevel::O => "o",
        MidashiLevel::Naka => "naka",
        MidashiLevel::Ko => "ko",
    };

    match style {
        MidashiStyle::Normal => format!("{level_str}-midashi"),
        MidashiStyle::Dogyo => format!("dogyo-{level_str}-midashi"),
        MidashiStyle::Mado => format!("mado-{level_str}-midashi"),
    }
}

/// MidashiLevel のHTMLタグ名を取得
pub fn midashi_html_tag(level: MidashiLevel) -> &'static str {
    match level {
        MidashiLevel::O => "h3",
        MidashiLevel::Naka => "h4",
        MidashiLevel::Ko => "h5",
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
pub fn jis_code_to_path(jis_code: &str) -> (String, String) {
    // "1-02-22" → ("1-02", "1-02-22")
    let parts: Vec<&str> = jis_code.split('-').collect();
    if parts.len() == 3 {
        let folder = format!("{}-{}", parts[0], parts[1]);
        (folder, jis_code.to_string())
    } else {
        ("".to_string(), jis_code.to_string())
    }
}

/// 行がブロック要素だけかどうかを判定（<br />を追加しない）
/// Ruby版aozora2htmlと同じロジック
pub fn is_block_only_line(html: &str) -> bool {
    // すでに<br />で終わっている
    if html.ends_with("<br />") {
        return true;
    }

    // </p>で終わる
    if html.ends_with("</p>") {
        return true;
    }

    // </h1>, </h2>, etc.で終わる（ただし同行見出しと窓見出しは除く）
    if html.ends_with("</h1>")
        || html.ends_with("</h2>")
        || html.ends_with("</h3>")
        || html.ends_with("</h4>")
        || html.ends_with("</h5>")
        || html.ends_with("</h6>")
    {
        // 同行見出しと窓見出しの場合は<br />を追加する
        if !html.contains("dogyo-") && !html.contains("mado-") {
            return true;
        }
    }

    // </div>で終わる
    if html.ends_with("</div>") {
        return true;
    }

    // <div...>で終わる（開始タグ）
    if html.ends_with(">") {
        if let Some(last_lt) = html.rfind('<') {
            let last_tag = &html[last_lt..];
            if last_tag.starts_with("<div") && !last_tag.starts_with("</div") {
                return true;
            }
            // 見出しの開始アンカー
            if last_tag.starts_with("<a class=\"midashi_anchor\"") {
                return true;
            }
        }
    }

    // 見出し開始タグで終わる
    if html.ends_with("\">") {
        if html.contains("<h3") || html.contains("<h4") || html.contains("<h5") {
            return true;
        }
    }

    // 全体が単一のタグ: ^<[^>]*>$
    if html.starts_with('<') && html.ends_with('>') && html.len() > 2 {
        // 自己終了タグはブロック要素ではない
        if html.ends_with(" />") || html.ends_with("/>") {
            return false;
        }
        let inner = &html[1..html.len() - 1];
        if !inner.contains('>') {
            return true;
        }
    }

    false
}

/// 後付け（bibliographical_information）内のテキストを自動リンク化
///
/// 以下の固定文字列のみをリンク化する：
/// - `info@aozora.gr.jp` → `<a href="mailto:info@aozora.gr.jp">info@aozora.gr.jp</a>`
/// - `青空文庫（http://www.aozora.gr.jp/）` → `<a href="http://www.aozora.gr.jp/">青空文庫（http://www.aozora.gr.jp/）</a>`
pub fn auto_link(text: &str) -> String {
    const EMAIL: &str = "info@aozora.gr.jp";
    const EMAIL_LINK: &str = "<a href=\"mailto:info@aozora.gr.jp\">info@aozora.gr.jp</a>";
    const AOZORA: &str = "青空文庫（http://www.aozora.gr.jp/）";
    const AOZORA_LINK: &str =
        "<a href=\"http://www.aozora.gr.jp/\">青空文庫（http://www.aozora.gr.jp/）</a>";

    text.replace(EMAIL, EMAIL_LINK).replace(AOZORA, AOZORA_LINK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_link_aozora() {
        let input = "青空文庫（http://www.aozora.gr.jp/）";
        let expected =
            "<a href=\"http://www.aozora.gr.jp/\">青空文庫（http://www.aozora.gr.jp/）</a>";
        assert_eq!(auto_link(input), expected);
    }

    #[test]
    fn test_auto_link_aozora_with_prefix() {
        let input = "インターネットの図書館、青空文庫（http://www.aozora.gr.jp/）で作られました";
        let expected = "インターネットの図書館、<a href=\"http://www.aozora.gr.jp/\">青空文庫（http://www.aozora.gr.jp/）</a>で作られました";
        assert_eq!(auto_link(input), expected);
    }

    #[test]
    fn test_auto_link_email() {
        let input = "info@aozora.gr.jp";
        let expected = "<a href=\"mailto:info@aozora.gr.jp\">info@aozora.gr.jp</a>";
        assert_eq!(auto_link(input), expected);
    }

    #[test]
    fn test_auto_link_no_match() {
        // 固定文字列以外はリンク化しない
        let input = "サイト（https://example.com/）";
        assert_eq!(auto_link(input), input);
    }

    #[test]
    fn test_auto_link_plain_text() {
        let input = "普通のテキストです";
        assert_eq!(auto_link(input), input);
    }

    #[test]
    fn test_classify_line_empty() {
        assert_eq!(classify_line(""), LineType::Empty);
    }

    #[test]
    fn test_classify_line_block() {
        assert_eq!(classify_line("<div class=\"test\">"), LineType::Block);
        assert_eq!(classify_line("</div>"), LineType::Block);
        assert_eq!(classify_line("<h3 class=\"midashi\">"), LineType::Block);
        assert_eq!(classify_line("</h3>"), LineType::Block);
        assert_eq!(classify_line("<h4>title</h4>"), LineType::Block);
        assert_eq!(classify_line("<h5>title</h5>"), LineType::Block);
    }

    #[test]
    fn test_classify_line_inline() {
        assert_eq!(classify_line("plain text"), LineType::Inline);
        assert_eq!(
            classify_line("<ruby>漢字<rt>かんじ</rt></ruby>"),
            LineType::Inline
        );
        assert_eq!(
            classify_line("<span class=\"test\">text</span>"),
            LineType::Inline
        );
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

    #[test]
    fn test_is_block_only_line() {
        assert!(is_block_only_line("</div>"));
        assert!(is_block_only_line("<div class=\"test\">"));
        assert!(!is_block_only_line("text"));
    }
}
