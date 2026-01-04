//! 文書構造の処理

/// 文書セクションの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionType {
    /// ヘッダー（タイトル、著者名など）
    Header,
    /// ヘッダー直後の空行後
    AfterHeader,
    /// 注記セクション（---で囲まれた部分）
    Chuuki,
    /// 本文
    Body,
}

/// 人物の種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersonType {
    /// 著者
    Author,
    /// 翻訳者
    Translator,
    /// 編者
    Editor,
    /// 編訳者
    Henyaku,
}

/// ヘッダー情報
#[derive(Debug, Clone, Default)]
pub struct HeaderInfo {
    /// タイトル
    pub title: Option<String>,
    /// 著者
    pub author: Option<String>,
    /// 副題
    pub subtitle: Option<String>,
    /// 原題
    pub original_title: Option<String>,
    /// 原副題
    pub original_subtitle: Option<String>,
    /// 翻訳者
    pub translator: Option<String>,
    /// 編者
    pub editor: Option<String>,
    /// 編訳者
    pub henyaku: Option<String>,
}

impl HeaderInfo {
    /// title要素用の文字列を生成（著者 訳者 編者 編訳者 タイトル 原題 副題 原副題 形式）
    pub fn html_title(&self) -> String {
        let mut parts = Vec::new();
        if let Some(author) = &self.author {
            parts.push(author.clone());
        }
        if let Some(translator) = &self.translator {
            parts.push(translator.clone());
        }
        if let Some(editor) = &self.editor {
            parts.push(editor.clone());
        }
        if let Some(henyaku) = &self.henyaku {
            parts.push(henyaku.clone());
        }
        if let Some(title) = &self.title {
            parts.push(title.clone());
        }
        if let Some(original_title) = &self.original_title {
            parts.push(original_title.clone());
        }
        if let Some(subtitle) = &self.subtitle {
            parts.push(subtitle.clone());
        }
        if let Some(original_subtitle) = &self.original_subtitle {
            parts.push(original_subtitle.clone());
        }
        parts.join(" ")
    }
}

/// ヘッダー行からヘッダー情報を抽出
///
/// 青空文庫のヘッダー形式:
/// - 1行目: タイトル
/// - 2行目以降: 著者、副題、原題など（行数によって解釈が変わる）
pub fn extract_header_info(lines: &[&str]) -> HeaderInfo {
    let mut info = HeaderInfo::default();
    let mut header_lines = Vec::new();

    // 最初の空行までをヘッダーとして収集
    for line in lines {
        if line.is_empty() {
            break;
        }
        header_lines.push(*line);
    }

    match header_lines.len() {
        0 => {}
        1 => {
            info.title = Some(header_lines[0].to_string());
        }
        2 => {
            info.title = Some(header_lines[0].to_string());
            process_person(header_lines[1], &mut info);
        }
        3 => {
            info.title = Some(header_lines[0].to_string());
            if is_original_title(header_lines[1]) {
                // パターンA: 作品名、原題、著者
                info.original_title = Some(header_lines[1].to_string());
                process_person(header_lines[2], &mut info);
            } else if process_person(header_lines[2], &mut info) == PersonType::Author {
                // パターンB: 作品名、副題、著者
                info.subtitle = Some(header_lines[1].to_string());
            } else {
                // パターンC: 作品名、著者、訳者等
                info.author = Some(header_lines[1].to_string());
            }
        }
        4 => {
            info.title = Some(header_lines[0].to_string());
            if is_original_title(header_lines[1]) {
                info.original_title = Some(header_lines[1].to_string());
            } else {
                info.subtitle = Some(header_lines[1].to_string());
            }
            if process_person(header_lines[3], &mut info) == PersonType::Author {
                info.subtitle = Some(header_lines[2].to_string());
            } else {
                info.author = Some(header_lines[2].to_string());
            }
        }
        5 => {
            info.title = Some(header_lines[0].to_string());
            info.original_title = Some(header_lines[1].to_string());
            info.subtitle = Some(header_lines[2].to_string());
            info.author = Some(header_lines[3].to_string());
            process_person(header_lines[4], &mut info);
        }
        6 => {
            info.title = Some(header_lines[0].to_string());
            info.original_title = Some(header_lines[1].to_string());
            info.subtitle = Some(header_lines[2].to_string());
            info.original_subtitle = Some(header_lines[3].to_string());
            info.author = Some(header_lines[4].to_string());
            process_person(header_lines[5], &mut info);
        }
        _ => {
            // 7行以上は6行と同様に処理
            info.title = Some(header_lines[0].to_string());
            info.original_title = Some(header_lines[1].to_string());
            info.subtitle = Some(header_lines[2].to_string());
            info.original_subtitle = Some(header_lines[3].to_string());
            info.author = Some(header_lines[4].to_string());
            process_person(header_lines[5], &mut info);
        }
    }

    info
}

/// 人物名を処理してHeaderInfoに設定、種別を返す
fn process_person(s: &str, info: &mut HeaderInfo) -> PersonType {
    let person_type = detect_person_type(s);
    match person_type {
        PersonType::Editor => info.editor = Some(s.to_string()),
        PersonType::Translator => info.translator = Some(s.to_string()),
        PersonType::Henyaku => info.henyaku = Some(s.to_string()),
        PersonType::Author => info.author = Some(s.to_string()),
    }
    person_type
}

/// 人物の種別を判定
fn detect_person_type(s: &str) -> PersonType {
    if s.ends_with("編訳") {
        PersonType::Henyaku
    } else if s.ends_with("校訂") || s.ends_with('編') || s.ends_with("編集") {
        PersonType::Editor
    } else if s.ends_with('訳') {
        PersonType::Translator
    } else {
        PersonType::Author
    }
}

/// 原題かどうかを判定
///
/// 以下の文字のみで構成される場合に原題と判定:
/// - ASCII文字 (U+0000〜U+007F)
/// - JIS第1水準記号（全角スペース、句読点等）
/// - JIS第6〜7水準（ギリシア文字、キリル文字等）
fn is_original_title(s: &str) -> bool {
    s.chars().all(|c| {
        // ASCII
        if c.is_ascii() {
            return true;
        }
        // 全角スペース、記号類（JIS第1水準）
        // U+3000-U+303F CJK Symbols and Punctuation
        // U+FF00-U+FFEF Halfwidth and Fullwidth Forms
        if ('\u{3000}'..='\u{303F}').contains(&c) || ('\u{FF00}'..='\u{FFEF}').contains(&c) {
            return true;
        }
        // ギリシア文字（JIS第6水準相当）
        if ('\u{0370}'..='\u{03FF}').contains(&c) {
            return true;
        }
        // キリル文字（JIS第7水準相当）
        if ('\u{0400}'..='\u{04FF}').contains(&c) {
            return true;
        }
        false
    })
}

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
/// use aozora_core::document::extract_body_lines;
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
    let mut section = SectionType::Header;

    for line in lines {
        match section {
            SectionType::Header => {
                // 空行でヘッダー終了
                if line.is_empty() {
                    section = SectionType::AfterHeader;
                }
            }
            SectionType::AfterHeader => {
                // 空行後、---で始まれば注記セクション、そうでなければ本文
                if line.starts_with("---") {
                    section = SectionType::Chuuki;
                } else if line.is_empty() {
                    // 連続する空行はスキップ
                } else {
                    // 本文開始
                    if line.starts_with("底本：") {
                        break;
                    }
                    result.push(*line);
                    section = SectionType::Body;
                }
            }
            SectionType::Chuuki => {
                // ---で注記セクション終了
                if line.starts_with("---") {
                    section = SectionType::Body;
                }
            }
            SectionType::Body => {
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

/// 文書から底本情報（bibliographical information）を抽出
///
/// 「底本：」で始まる行から最後までを抽出します。
pub fn extract_bibliographical_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut in_biblio = false;

    for line in lines {
        if line.starts_with("底本：") {
            in_biblio = true;
        }
        if in_biblio {
            result.push(*line);
        }
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
            "",
            "底本：青空文庫",
        ];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目", ""]);
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
            "",
            "底本：青空文庫",
        ];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目", ""]);
    }

    #[test]
    fn test_no_header() {
        let lines = vec!["", "本文1行目", "本文2行目", "", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文1行目", "本文2行目", ""]);
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
        let lines = vec!["タイトル", "", "", "本文", "", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["本文", ""]);
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

    // ヘッダー情報抽出テスト

    #[test]
    fn test_extract_header_1line() {
        let lines = vec!["タイトル", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.author, None);
    }

    #[test]
    fn test_extract_header_2lines() {
        let lines = vec!["タイトル", "著者名", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.author, Some("著者名".to_string()));
    }

    #[test]
    fn test_extract_header_2lines_translator() {
        let lines = vec!["タイトル", "山田太郎訳", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.translator, Some("山田太郎訳".to_string()));
        assert_eq!(info.author, None);
    }

    #[test]
    fn test_extract_header_3lines_with_original() {
        // パターンA: 作品名、原題、著者
        let lines = vec!["タイトル", "TITLE", "著者名", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.original_title, Some("TITLE".to_string()));
        assert_eq!(info.author, Some("著者名".to_string()));
    }

    #[test]
    fn test_extract_header_3lines_with_subtitle() {
        // パターンB: 作品名、副題、著者
        let lines = vec!["タイトル", "副題", "著者名", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.subtitle, Some("副題".to_string()));
        assert_eq!(info.author, Some("著者名".to_string()));
    }

    #[test]
    fn test_extract_header_3lines_author_translator() {
        // パターンC: 作品名、著者、訳者
        let lines = vec!["タイトル", "著者名", "訳者訳", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.author, Some("著者名".to_string()));
        assert_eq!(info.translator, Some("訳者訳".to_string()));
    }

    #[test]
    fn test_extract_header_henyaku() {
        let lines = vec!["タイトル", "編訳者編訳", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.henyaku, Some("編訳者編訳".to_string()));
    }

    #[test]
    fn test_extract_header_editor() {
        let lines = vec!["タイトル", "編者名編", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.editor, Some("編者名編".to_string()));
    }

    #[test]
    fn test_extract_header_6lines() {
        let lines = vec![
            "タイトル",
            "ORIGINAL TITLE",
            "副題",
            "ORIGINAL SUBTITLE",
            "著者名",
            "訳者訳",
            "",
        ];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("タイトル".to_string()));
        assert_eq!(info.original_title, Some("ORIGINAL TITLE".to_string()));
        assert_eq!(info.subtitle, Some("副題".to_string()));
        assert_eq!(info.original_subtitle, Some("ORIGINAL SUBTITLE".to_string()));
        assert_eq!(info.author, Some("著者名".to_string()));
        assert_eq!(info.translator, Some("訳者訳".to_string()));
    }

    #[test]
    fn test_is_original_title_ascii() {
        assert!(is_original_title("The Great Gatsby"));
        assert!(is_original_title("ABC 123"));
    }

    #[test]
    fn test_is_original_title_with_fullwidth() {
        // 全角スペースや句読点を含んでも原題として判定
        assert!(is_original_title("ABC　DEF")); // 全角スペース
    }

    #[test]
    fn test_is_original_title_japanese() {
        // 日本語が含まれる場合は原題ではない
        assert!(!is_original_title("副題です"));
        assert!(!is_original_title("タイトル"));
    }

    #[test]
    fn test_is_original_title_greek() {
        // ギリシア文字は原題として判定
        assert!(is_original_title("Αβγ"));
    }

    #[test]
    fn test_detect_person_type() {
        assert_eq!(detect_person_type("山田太郎"), PersonType::Author);
        assert_eq!(detect_person_type("山田太郎訳"), PersonType::Translator);
        assert_eq!(detect_person_type("山田太郎編"), PersonType::Editor);
        assert_eq!(detect_person_type("山田太郎編集"), PersonType::Editor);
        assert_eq!(detect_person_type("山田太郎校訂"), PersonType::Editor);
        assert_eq!(detect_person_type("山田太郎編訳"), PersonType::Henyaku);
    }

    #[test]
    fn test_html_title() {
        let info = HeaderInfo {
            title: Some("タイトル".to_string()),
            author: Some("著者名".to_string()),
            subtitle: None,
            original_title: None,
            original_subtitle: None,
            translator: Some("訳者訳".to_string()),
            editor: None,
            henyaku: None,
        };
        assert_eq!(info.html_title(), "著者名 訳者訳 タイトル");
    }
}
