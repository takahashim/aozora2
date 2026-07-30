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
    /// 後付け（底本：／［＃本文終わり］以降）
    Trailer,
}

/// バッファの各行が属するセクション（行番号を保ったまま返すための型）。
///
/// [`extract_body_lines`] はこの分類から本文行だけを取り出したもの。
/// エディタ支援（`crate::analysis`）は「その行で記法が効くか」を**行番号を
/// 保ったまま**知りたいので、同じ状態機械を書き直さずこの分類を共有する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSection {
    /// ヘッダ（作品名・著者等）。参照実装 `parse_header` はルビ `《》` と `｜` を
    /// **剥がして**から項目に割り当て、記法は一切処理しない。
    Header,
    /// 注記セクション（罫線で囲まれた凡例）。出力に一切現れない。
    Chuuki,
    /// 本文。
    Body,
    /// 後付け（本文終わり後・底本情報）。本文とは別セクションだが記法は効く。
    Trailer,
}

impl LineSection {
    /// この行で青空文庫記法が効く（＝解釈されて出力に反映される）か。
    ///
    /// ヘッダはルビを剥がした生文字列として出力され、注記セクションは
    /// そもそも出力されないので、どちらも記法は効かない。
    pub fn applies_notation(self) -> bool {
        match self {
            LineSection::Header | LineSection::Chuuki => false,
            LineSection::Body | LineSection::Trailer => true,
        }
    }
}

/// 各行のセクションを行番号を保ったまま判定する（[`extract_body_lines`] の実体）。
pub fn classify_lines(lines: &[&str]) -> Vec<LineSection> {
    let mut out = Vec::with_capacity(lines.len());
    let mut section = SectionType::Header;

    for line in lines {
        match section {
            SectionType::Header => {
                out.push(LineSection::Header);
                // 空行でヘッダー終了
                if line.is_empty() {
                    section = SectionType::AfterHeader;
                }
            }
            SectionType::AfterHeader => {
                // ヘッダー終端の空行の「次の1行」だけで注記セクションかどうかが決まる。
                // 罫線（-だけの行）なら注記セクション、それ以外はその行から本文。
                if is_rule_line(line) {
                    out.push(LineSection::Chuuki);
                    section = SectionType::Chuuki;
                } else if line.starts_with("底本：") {
                    out.push(LineSection::Trailer);
                    section = SectionType::Trailer;
                } else {
                    out.push(LineSection::Body);
                    section = SectionType::Body;
                }
            }
            SectionType::Chuuki => {
                out.push(LineSection::Chuuki);
                // 罫線で注記セクション終了
                if is_rule_line(line) {
                    section = SectionType::Body;
                }
            }
            SectionType::Body => {
                // 底本：または［＃本文終わり］で本文終了
                if line.starts_with("底本：") || *line == "［＃本文終わり］" {
                    out.push(LineSection::Trailer);
                    section = SectionType::Trailer;
                } else {
                    out.push(LineSection::Body);
                }
            }
            SectionType::Trailer => out.push(LineSection::Trailer),
        }
    }

    out
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

    // 最初の空行までをヘッダーとして収集し、参照実装 parse_header と同様に
    // ｜（ルビ親文字区切り）とルビ 《...》 を除去してから各項目に割り当てる。
    let mut stripped: Vec<String> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        stripped.push(strip_header_ruby(line));
    }
    let header_lines: Vec<&str> = stripped.iter().map(|s| s.as_str()).collect();

    // 参照実装 Header#build_header_info は `header_info = { title: @header[0] }` から
    // 始めて `case @header.length` の 2〜6 だけを足す。else が無いので、1 行と
    // **7 行以上はタイトルだけ**になる（Ruby の case は該当なしで何もしない）。
    match header_lines.len() {
        0 => {}
        // 7 行以上をここに含めるのが要点。6 行と同様に処理すると、空行を欠く文書で
        // 注記セクションの罫線が「原副題」に、凡例の箇条書きが「著者」になる
        // （実コーパスで 000124/658 が該当し、<title> にも混入していた）。
        1 | 7.. => {
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
    }

    info
}

/// ヘッダ行から ｜（ルビ親文字区切り）とルビ 《...》 を除去する。
///
/// 参照実装 parse_header の `string.gsub!(RUBY_PREFIX, ''); string.gsub!(PAT_RUBY, '')`
/// に対応。PAT_RUBY = /《.*?》/（非貪欲）なので、対応する 》 が無い 《 は残す。
fn strip_header_ruby(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '｜' => {} // ｜ は全て除去
            '《' => {
                // 次の 》 までをルビとして捨てる。見つからなければ 《 以降を残す。
                let mut buf = String::new();
                let mut closed = false;
                for nc in chars.by_ref() {
                    if nc == '》' {
                        closed = true;
                        break;
                    }
                    buf.push(nc);
                }
                if !closed {
                    out.push('《');
                    out.push_str(&buf);
                }
            }
            _ => out.push(c),
        }
    }
    out
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
    s.chars().all(is_original_title_char)
}

/// 原題に使える文字か。
///
/// 受理する文字の一覧は `data/original_title_chars.json` にある（判定基準そのものが
/// そのデータ。参照実装 `header_element_type` は Shift_JIS のバイト範囲で判定するが、
/// それを Unicode の一覧に書き出してあるので Shift_JIS の対応表が無くても再現できる）。
fn is_original_title_char(c: char) -> bool {
    ORIGINAL_TITLE_CHARS.binary_search(&(c as u32)).is_ok()
}

// build.rs が data/original_title_chars.json から生成した昇順の符号位置列。
include!(concat!(env!("OUT_DIR"), "/original_title_chars.rs"));

/// 注記セクションの区切りに使われる罫線（`-` だけからなる行）かどうか
fn is_rule_line(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| c == '-')
}

/// 文書から本文行を抽出
///
/// # 文書構造
/// - 前付け: 最初の空行まで（タイトル、著者名など）
/// - 注記: 空行の直後が罫線で始まる場合、罫線で囲まれたセクション
///   （【テキスト中に現れる記号について】など）
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
    let sections = classify_lines(lines);
    let mut result = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if sections[i] != LineSection::Body {
            continue;
        }
        // 注記セクションを経ずに、本文が空行以外で始まる場合（＝直前がヘッダ終端の
        // 空行）、参照実装は本文の先頭に <br /> を1つ出すので空行を1行足して揃える。
        if result.is_empty() && i > 0 && sections[i - 1] == LineSection::Header && !line.is_empty()
        {
            result.push("");
        }
        result.push(*line);
    }

    result
}

/// 後付け（[`LineSection::Trailer`]）の開始位置。本文を終わらせた行そのものを指す。
///
/// 後付けが after_text か底本情報かは**この1行だけ**で決まる（`［＃本文終わり］`
/// なら after_text、`底本：` なら底本情報）。本文より前に同じ文字列があっても
/// 反応しないよう、走査ではなくセクション分類から取る。
fn trailer_start(lines: &[&str]) -> Option<usize> {
    classify_lines(lines)
        .iter()
        .position(|s| *s == LineSection::Trailer)
}

/// 文書から本文終わり後のテキスト（after_text）を抽出
///
/// `［＃本文終わり］` の**次の行から最後まで**を返す（マーカー自身は含めない）。
/// 底本情報もここに含まれる。参照実装は `［＃本文終わり］` で後付けセクションへ
/// 移り、そのあとの `底本：` は新しいセクションを開かないため
/// （実測: `<div class="after_text">` の中に底本行がそのまま入る）。
/// `［＃本文終わり］` が無い場合は空。
pub fn extract_after_text_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let start = trailer_start(lines).filter(|i| lines[*i] == "［＃本文終わり］");
    start.map_or_else(Vec::new, |i| lines[i + 1..].to_vec())
}

/// 文書から底本情報（bibliographical information）を抽出
///
/// `底本：` で始まる行から最後までを返す。
/// `［＃本文終わり］` で後付けに入った場合は after_text 側がすべて受け持つので空。
pub fn extract_bibliographical_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let start = trailer_start(lines).filter(|i| lines[*i].starts_with("底本："));
    start.map_or_else(Vec::new, |i| lines[i..].to_vec())
}

#[cfg(test)]
mod tests {
    #[test]
    fn original_title_chars_match_sjis_definition() {
        // 旧定義: Shift_JIS へ符号化し、1 バイトなら <= 0x7f、2 バイトなら
        // 8140-8258 / 839f-8491 に入るか。区点レンジから生成した表がこれと
        // 1 文字も違わないことを BMP 全域で確認する。
        fn sjis_range_reference(c: char) -> bool {
            let mut buf = [0u8; 8];
            let (encoded, _, had_err) = encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buf));
            if had_err {
                return false;
            }
            match encoded.as_ref() {
                [b] => *b <= 0x7f,
                [hi, lo] => {
                    let code = ((*hi as u16) << 8) | *lo as u16;
                    (0x8140..=0x8258).contains(&code) || (0x839f..=0x8491).contains(&code)
                }
                _ => false,
            }
        }

        for cp in 0..=0xFFFFu32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            assert_eq!(
                super::is_original_title_char(c),
                sjis_range_reference(c),
                "U+{cp:04X} ({c:?}) の判定が旧定義と食い違う"
            );
        }
    }

    use super::*;

    /// 注記セクションがなく本文が直接始まる場合、参照実装 aozora2html は
    /// 本文の先頭に <br /> を 1 つ出す。ここでは空行 1 行として表現する。
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
        assert_eq!(body, vec!["", "本文1行目", "本文2行目", ""]);
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
        assert_eq!(body, vec!["", "本文1行目", "本文2行目", ""]);
    }

    #[test]
    fn test_no_footer() {
        let lines = vec!["タイトル", "", "本文1行目", "本文2行目"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["", "本文1行目", "本文2行目"]);
    }

    #[test]
    fn test_empty_body() {
        let lines = vec!["タイトル", "", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert!(body.is_empty());
    }

    /// ヘッダー終端の次が空行なら、その空行自体が本文の先頭の <br /> になる
    #[test]
    fn test_multiple_blank_lines() {
        let lines = vec!["タイトル", "", "", "本文", "", "底本：青空文庫"];
        let body = extract_body_lines(&lines);
        assert_eq!(body, vec!["", "本文", ""]);
    }

    /// 注記セクションかどうかはヘッダー終端の「次の 1 行」だけで決まる。
    /// 空行が挟まると、続く罫線は注記の開始ではなく本文として扱われる
    /// （参照実装 aozora2html の judge_chuuki と同じ）。
    #[test]
    fn test_chuuki_is_decided_by_the_line_right_after_the_header() {
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
        assert_eq!(body, vec!["", "---", "注記内容", "---", "本文"]);
    }

    /// ［＃本文終わり］があると、そのあとの「底本：」は独立した節を作らず
    /// 後付けに入る（参照実装では本文終わりの時点で :tail に移るため）
    #[test]
    fn test_after_text_absorbs_the_bibliography() {
        let lines = vec![
            "タイトル",
            "",
            "本文",
            "［＃本文終わり］",
            "底本：青空文庫",
            "入力：誰か",
        ];
        assert_eq!(
            extract_after_text_lines(&lines),
            vec!["底本：青空文庫", "入力：誰か"]
        );
        assert!(extract_bibliographical_lines(&lines).is_empty());
    }

    /// ［＃本文終わり］がなければ従来どおり底本情報の節になる
    #[test]
    fn test_bibliography_without_an_end_of_text_marker() {
        let lines = vec!["タイトル", "", "本文", "底本：青空文庫"];
        assert!(extract_after_text_lines(&lines).is_empty());
        assert_eq!(
            extract_bibliographical_lines(&lines),
            vec!["底本：青空文庫"]
        );
    }

    /// 罫線は `-` だけからなる行に限る
    #[test]
    fn test_rule_line_must_be_all_dashes() {
        assert!(is_rule_line("-"));
        assert!(is_rule_line("-------"));
        assert!(!is_rule_line(""));
        assert!(!is_rule_line("--- 注記"));
        assert!(!is_rule_line("―――"));
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
    fn test_strip_header_ruby() {
        // ｜ と 《...》 を除去。対応する 》 が無い 《 は残す。
        assert_eq!(strip_header_ruby("田舎｜教師《きょうし》"), "田舎教師");
        assert_eq!(strip_header_ruby("漢字《かんじ》の本《ほん》"), "漢字の本");
        assert_eq!(strip_header_ruby("｜｜あ"), "あ");
        assert_eq!(strip_header_ruby("《未閉じ"), "《未閉じ");
        assert_eq!(strip_header_ruby("普通のタイトル"), "普通のタイトル");
    }

    #[test]
    fn test_extract_header_strips_ruby() {
        // ヘッダのタイトル・著者からルビと ｜ を除去する（参照実装 parse_header）。
        let lines = vec!["田舎｜教師《きょうし》", "著者｜名《めい》", ""];
        let info = extract_header_info(&lines);
        assert_eq!(info.title, Some("田舎教師".to_string()));
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
        assert_eq!(
            info.original_subtitle,
            Some("ORIGINAL SUBTITLE".to_string())
        );
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
    fn test_is_original_title_dashes_and_symbols() {
        // 全角ダッシュ（U+2015, Shift_JIS 815D）を含むラテン語の原題
        assert!(is_original_title("―― Ibi omnis effusus labor ! ――"));
        // キリル文字も原題
        assert!(is_original_title("Война"));
        // 漢字が混じれば原題ではない
        assert!(!is_original_title("―― 副題 ――"));
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

    /// 参照実装 `Header#build_header_info` の `case @header.length` には else が無く、
    /// 2〜6 行だけを扱う。1 行と **7 行以上はタイトルだけ**になる。
    ///
    /// 実コーパスでは 000124/658（小熊秀雄全集）が該当する。ヘッダに空行が無く
    /// 注記セクションの罫線までがヘッダ扱いになる文書で、6 行と同様に処理すると
    /// 罫線が「原副題」に、凡例の箇条書きが「著者」になり `<title>` にも混入した。
    /// この文書はオラクルの対象外なので、ここで固定する。
    #[test]
    fn header_of_seven_or_more_lines_keeps_only_the_title() {
        let lines = vec![
            "小熊秀雄全集",
            "―３―",
            "詩集２　中期詩篇",
            "--------------------------------------------------",
            "［表記について］",
            "●ルビは「漢字《ルビ》」の形式で処理した。",
            "●［＃］は、入力者注を示す。",
            "--------------------------------------------------",
            "",
            "本文",
        ];
        let info = extract_header_info(&lines);
        assert_eq!(info.title.as_deref(), Some("小熊秀雄全集"));
        assert_eq!(info.original_title, None);
        assert_eq!(info.subtitle, None);
        assert_eq!(info.original_subtitle, None);
        assert_eq!(info.author, None);
        // <title> にもタイトルだけが入る。
        assert_eq!(info.html_title(), "小熊秀雄全集");
    }

    /// `［＃本文終わり］` があると、底本情報も含めて後付けがすべて after_text に入る
    /// （参照実装で実測: `<div class="after_text">` の中に底本行がそのまま並ぶ）。
    #[test]
    fn after_text_swallows_the_colophon() {
        let lines = vec![
            "作品名",
            "著者",
            "",
            "本文です。",
            "［＃本文終わり］",
            "あとがき行。",
            "",
            "底本：「テスト」",
            "入力：だれか",
        ];
        assert_eq!(
            extract_after_text_lines(&lines),
            vec!["あとがき行。", "", "底本：「テスト」", "入力：だれか"]
        );
        assert!(extract_bibliographical_lines(&lines).is_empty());
    }

    /// 後付けの判定はセクション分類から取るので、本文より前に同じ文字列があっても
    /// 反応しない（注記セクションの凡例に `底本：` を書いても後付けにならない）。
    #[test]
    fn trailer_is_detected_by_section_not_by_scanning() {
        let lines = vec![
            "作品名",
            "著者",
            "",
            "----------",
            "凡例に底本：と書いてある",
            "----------",
            "本文です。",
            "底本：「ほんもの」",
        ];
        assert_eq!(extract_body_lines(&lines), vec!["本文です。"]);
        assert_eq!(extract_bibliographical_lines(&lines), vec!["底本：「ほんもの」"]);
    }
}
