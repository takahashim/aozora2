//! 特殊コンテンツの解析
//!
//! 画像、返り点、送り仮名などの特殊コマンドを解析します。

use super::command_parser::CommandResult;

/// 画像コマンドを解析
pub fn try_parse_image(content: &str) -> Option<CommandResult> {
    // パターン1: 説明（ファイル名、横N×縦M）入る  - 説明が括弧外
    // パターン2: （説明）（ファイル名、横N×縦M）入る - 説明が括弧内
    // パターン3: 「...」のキャプション付きの図（ファイル名、横N×縦M）入る
    let content = content.trim_end_matches("入る").trim();

    // ファイル情報を含む括弧を最後から探す
    let info_start = content.rfind('（')?;
    let info_end = content.rfind('）')?;
    if info_end <= info_start {
        return None;
    }

    let info = &content[info_start + '（'.len_utf8()..info_end];

    // ファイル名とサイズを分離
    let parts: Vec<&str> = info.split('、').collect();
    let filename = parts.first()?.to_string();

    // ファイル名っぽいかチェック
    if !is_image_filename(&filename) {
        return None;
    }

    let (width, height) = parse_image_dimensions(parts.get(1).copied());

    // 説明部分を取得
    let desc_part = content[..info_start].trim();
    let alt = extract_alt_text(desc_part);

    Some(CommandResult::Image {
        filename,
        alt,
        width,
        height,
    })
}

/// 画像ファイル名かどうかをチェック
fn is_image_filename(filename: &str) -> bool {
    filename.ends_with(".png") || filename.ends_with(".jpg") || filename.ends_with(".gif")
}

/// 画像サイズを解析
fn parse_image_dimensions(size_part: Option<&str>) -> (Option<u32>, Option<u32>) {
    let Some(size_part) = size_part else {
        return (None, None);
    };

    let mut width = None;
    let mut height = None;

    // 横N×縦M パターン
    if let Some(w_pos) = size_part.find('横') {
        if let Some(x_pos) = size_part.find('×') {
            let w_str = &size_part[w_pos + '横'.len_utf8()..x_pos];
            width = w_str.parse().ok();
        }
    }

    if let Some(h_pos) = size_part.find('縦') {
        let h_str = &size_part[h_pos + '縦'.len_utf8()..];
        height = h_str
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .ok();
    }

    (width, height)
}

/// 代替テキストを抽出
fn extract_alt_text(desc_part: &str) -> String {
    // キャプション付きの図パターン: 「...」のキャプション付きの図 形式を保持
    if desc_part.ends_with("のキャプション付きの図") {
        return desc_part.to_string();
    }

    // （説明）パターン
    if desc_part.starts_with('（') && desc_part.ends_with('）') {
        return desc_part['（'.len_utf8()..desc_part.len() - '）'.len_utf8()].to_string();
    }

    // 説明がそのまま
    desc_part.to_string()
}

/// 返り点かどうかを判定
pub fn is_kaeriten(content: &str) -> bool {
    const KAERITEN_CHARS: &[char] = &[
        '一', '二', '三', '四', '上', '中', '下', '天', '地', '人', '甲', '乙', '丙', '丁', 'レ',
    ];

    // 短いコマンドで、すべての文字が返り点文字かどうか
    if content.is_empty() || content.chars().count() > 4 {
        return false;
    }

    content.chars().all(|c| KAERITEN_CHARS.contains(&c))
}

/// 訓点送り仮名を解析
pub fn try_parse_okurigana(content: &str) -> Option<String> {
    // （...）形式をチェック
    if content.starts_with('（') && content.ends_with('）') {
        let inner = &content['（'.len_utf8()..content.len() - '）'.len_utf8()];
        let char_count = inner.chars().count();
        if (1..=10).contains(&char_count) && !inner.is_empty() {
            return Some(inner.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_kaeriten() {
        assert!(is_kaeriten("一"));
        assert!(is_kaeriten("レ"));
        assert!(is_kaeriten("上"));
        assert!(is_kaeriten("一二"));
        assert!(!is_kaeriten("あ"));
        assert!(!is_kaeriten(""));
        assert!(!is_kaeriten("一二三四五"));
    }

    #[test]
    fn test_try_parse_okurigana() {
        assert_eq!(try_parse_okurigana("（ノ）"), Some("ノ".to_string()));
        assert_eq!(try_parse_okurigana("（テ）"), Some("テ".to_string()));
        assert_eq!(try_parse_okurigana("テスト"), None);
    }

    #[test]
    fn test_try_parse_image() {
        let result = try_parse_image("挿絵（fig001.png、横100×縦200）入る");
        assert!(result.is_some());
        if let Some(CommandResult::Image {
            filename,
            alt,
            width,
            height,
        }) = result
        {
            assert_eq!(filename, "fig001.png");
            assert_eq!(alt, "挿絵");
            assert_eq!(width, Some(100));
            assert_eq!(height, Some(200));
        }
    }

    #[test]
    fn test_is_image_filename() {
        assert!(is_image_filename("test.png"));
        assert!(is_image_filename("image.jpg"));
        assert!(is_image_filename("fig.gif"));
        assert!(!is_image_filename("document.txt"));
    }
}
