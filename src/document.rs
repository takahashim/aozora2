//! 文書構造の処理

/// 文書から本文行を抽出
///
/// # 文書構造
/// - 前付け: 最初の空行まで（タイトル、著者名など）
/// - 本文: 空行後から「底本：」まで
/// - 後付け: 「底本：」以降（底本情報、入力者情報など）
///
/// # Examples
///
/// ```
/// use aozora2text::document::extract_body_lines;
///
/// let lines = vec!["タイトル", "著者", "", "本文1行目", "本文2行目", "底本：〇〇文庫"];
/// let body = extract_body_lines(&lines);
/// assert_eq!(body, vec!["本文1行目", "本文2行目"]);
/// ```
pub fn extract_body_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut in_body = false;

    for line in lines {
        // 前付け終了判定（最初の空行）
        if !in_body {
            if line.is_empty() {
                in_body = true;
            }
            continue;
        }

        // 後付け開始判定（「底本：」で始まる行）
        if line.starts_with("底本：") {
            break;
        }

        result.push(*line);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_structure() {
        let lines = vec![
            "タイトル",
            "著者名",
            "",
            "本文1行目",
            "本文2行目",
            "底本：青空文庫",
        ];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目"]);
    }

    #[test]
    fn test_no_header() {
        let lines = vec!["", "本文1行目", "本文2行目", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目"]);
    }

    #[test]
    fn test_no_footer() {
        let lines = vec!["タイトル", "", "本文1行目", "本文2行目"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目"]);
    }

    #[test]
    fn test_empty_body() {
        let lines = vec!["タイトル", "", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert!(body.is_empty());
    }

    #[test]
    fn test_multiple_blank_lines() {
        let lines = vec!["タイトル", "", "", "本文", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["", "本文"]);
    }
}
