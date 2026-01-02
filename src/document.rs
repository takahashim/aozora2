//! 文書構造の処理

/// 文書から本文行を抽出
///
/// # 文書構造
/// - 前付け: 最初の空行まで（タイトル、著者名など）
/// - 注記: 空行後、`---`で囲まれたセクション（【テキスト中に現れる記号について】など）
/// - 本文: 注記後から「底本：」まで
/// - 後付け: 「底本：」以降（底本情報、入力者情報など）
///
/// # Examples
///
/// ```
/// use aozora2text::document::extract_body_lines;
///
/// let lines = vec![
///     "タイトル", "著者", "",
///     "-------", "【テキスト中に現れる記号について】", "-------",
///     "本文1行目", "底本：〇〇文庫"
/// ];
/// let body = extract_body_lines(&lines);
/// assert_eq!(body, vec!["本文1行目"]);
/// ```
pub fn extract_body_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut section = Section::Header;

    for line in lines {
        match section {
            Section::Header => {
                // 空行でヘッダー終了
                if line.is_empty() {
                    section = Section::AfterHeader;
                }
            }
            Section::AfterHeader => {
                // 空行後、---で始まれば注記セクション、そうでなければ本文
                if line.starts_with("---") {
                    section = Section::Chuuki;
                } else if line.is_empty() {
                    // 連続する空行はスキップ
                } else {
                    // 本文開始
                    if line.starts_with("底本：") {
                        break;
                    }
                    result.push(*line);
                    section = Section::Body;
                }
            }
            Section::Chuuki => {
                // ---で注記セクション終了
                if line.starts_with("---") {
                    section = Section::Body;
                }
            }
            Section::Body => {
                // 底本：で本文終了
                if line.starts_with("底本：") {
                    break;
                }
                result.push(*line);
            }
        }
    }

    result
}

#[derive(Clone, Copy)]
enum Section {
    Header,
    AfterHeader,
    Chuuki,
    Body,
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
    fn test_with_chuuki() {
        let lines = vec![
            "タイトル",
            "著者名",
            "",
            "-------------------------------------------------------",
            "【テキスト中に現れる記号について】",
            "《》：ルビ",
            "［＃］：入力者注",
            "-------------------------------------------------------",
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
        assert_eq!(body, vec!["本文"]);
    }

    #[test]
    fn test_chuuki_with_multiple_blanks() {
        let lines = vec![
            "タイトル",
            "",
            "",
            "---",
            "注記内容",
            "---",
            "本文",
            "底本：青空文庫",
        ];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文"]);
    }
}
