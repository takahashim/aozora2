//! プレゼンテーションロジック
//!
//! CSSクラス名とHTMLタグ名のマッピングを提供します。

use aozora_core::node::{MidashiLevel, MidashiStyle, StyleType};

/// 未変換外字情報（フッタ「表記について」用）。
#[derive(Debug, Clone)]
pub struct UnconvertedGaiji {
    /// 外字名（説明の最後の「、」より前の部分）
    pub gaiji_name: String,
    /// ページ-行数（説明の最後の「、」より後の部分）。
    /// 同じ外字が複数回現れた場合は出現箇所を順に並べる。
    pub page_lines: Vec<String>,
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

    /// style_css_class / style_html_tag が参照実装 aozora2html の
    /// command_table.yml と一致することを、スナップショット
    /// data/command_table.tsv に照合して縛る。参照実装が表を更新したら
    /// スナップショットを取り直し、この照合で drift を検出する（誘因整合）。
    ///
    /// 表に載るのは基底の記法語のみ。左/下/上バリアントは参照実装の
    /// 方向フィルタ（傍点系→末尾 _after、傍線系→under を over に置換）で
    /// 導出されるので、ここでも同じ規則で期待値を作って照合する。
    #[test]
    fn test_style_css_and_tag_match_reference_command_table() {
        use std::collections::HashSet;

        let tsv = include_str!("../../data/command_table.tsv");
        let mut reached: HashSet<StyleType> = HashSet::new();

        for line in tsv.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut cols = line.split('\t');
            let word = cols.next().expect("記法語列");
            let class = cols.next().expect("CSSクラス列");
            let tag = cols.next().expect("HTML要素列");

            // 基底: 記法語 → StyleType → css/tag が表と一致
            let base = StyleType::from_command(word)
                .unwrap_or_else(|| panic!("記法語 {word:?} が from_command で解決できない"));
            assert_eq!(
                style_css_class(base),
                class,
                "{word:?} のCSSクラスが参照表とずれている"
            );
            assert_eq!(
                style_html_tag(base),
                tag,
                "{word:?} のHTML要素が参照表とずれている"
            );
            reached.insert(base);

            // 左/上バリアント: 参照の方向フィルタで期待値を導出して照合
            let after = base.to_after_variant();
            if after != base {
                let expected = if class.starts_with("underline") {
                    // 傍線系: under → over（sub と同じく先頭1回）
                    class.replacen("under", "over", 1)
                } else {
                    // 傍点系: 末尾に _after
                    format!("{class}_after")
                };
                assert_eq!(
                    style_css_class(after),
                    expected,
                    "{word:?} の左/上バリアントCSSクラスが方向フィルタ規則とずれている"
                );
                assert_eq!(style_html_tag(after), "em");
                reached.insert(after);
            }
        }

        // 表＋フィルタで全 StyleType を網羅していること
        // （新バリアントを追加したら表かフィルタのどちらかに必ず現れる）
        for st in StyleType::all() {
            assert!(
                reached.contains(st),
                "{st:?} が参照表・方向フィルタのどちらからも導出されていない"
            );
        }
    }
}
