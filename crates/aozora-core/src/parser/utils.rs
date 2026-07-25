//! パーサーユーティリティ
//!
//! パース処理で共通して使用するユーティリティ関数です。

/// 文字列から数字を抽出（全角数字も対応）
pub fn extract_number(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .filter_map(|c| {
            if c.is_ascii_digit() {
                Some(c)
            } else if ('０'..='９').contains(&c) {
                // 全角数字をASCII数字に変換
                Some((c as u32 - '０' as u32 + '0' as u32) as u8 as char)
            } else {
                None
            }
        })
        .collect();
    digits.parse().ok()
}

/// 漢数字・全角数字を含む文字列を算用数字（ASCII）へ正規化する。
///
/// 参照実装 aozora2html の `Utils.convert_japanese_number` と同じ。
/// `一字下げ`→`1字下げ`、`二十三`→`23`、`十`→`10` のように、字下げ幅などの
/// 数値指定に使われる漢数字を数字へ直す。全角数字も ASCII にする。
pub fn convert_japanese_number(s: &str) -> String {
    // 全角数字・漢数字（〇一…九）を1文字ずつ算用数字へ
    let mut t: String = s
        .chars()
        .map(|c| match c {
            '０'..='９' => ((c as u32 - '０' as u32) as u8 + b'0') as char,
            '〇' => '0',
            '一' => '1',
            '二' => '2',
            '三' => '3',
            '四' => '4',
            '五' => '5',
            '六' => '6',
            '七' => '7',
            '八' => '8',
            '九' => '9',
            other => other,
        })
        .collect();

    // 十（KANJI_TEN）の合成: (d)十(d)→dd, (d)十→d0, 十(d)→1d, 十→10
    // 参照実装の gsub 順序に合わせて置換する。
    t = replace_ten_between_digits(&t);
    t = replace_digit_ten(&t);
    t = replace_ten_digit(&t);
    t.replace('十', "10")
}

fn replace_ten_between_digits(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '十'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit())
        {
            // 直前の桁は既に out にある。十 を飛ばして次の桁を続ける（d十d→dd）
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn replace_digit_ten(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '十' && i > 0 && chars[i - 1].is_ascii_digit() {
            out.push('0'); // d十 → d0
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn replace_ten_digit(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '十' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
            out.push('1'); // 十d → 1d
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// キーワードの直前に連続する数字（全角・半角）を取り出す。
///
/// 参照実装の `(\d*)字下げ` のように、数字をキーワードに固定して読む。
/// `extract_number` が文字列中のあらゆる数字を拾って連結してしまうのに対し、
/// こちらはキーワード直前だけを見るので、`７字下げ、２１字詰め` から
/// `字下げ` の幅として 7 を取り出せる（21 を巻き込まない）。
pub fn extract_number_before(s: &str, keyword: &str) -> Option<u32> {
    let idx = s.find(keyword)?;
    let digits: String = s[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|c| {
            if c.is_ascii_digit() {
                c
            } else {
                ((c as u32 - '０' as u32 + '0' as u32) as u8) as char
            }
        })
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number() {
        assert_eq!(extract_number("2字下げ"), Some(2));
        assert_eq!(extract_number("10字詰め"), Some(10));
        assert_eq!(extract_number("字下げ"), None);
    }

    #[test]
    fn test_convert_japanese_number() {
        assert_eq!(convert_japanese_number("一字下げ"), "1字下げ");
        assert_eq!(convert_japanese_number("三字下げ"), "3字下げ");
        assert_eq!(convert_japanese_number("十字下げ"), "10字下げ");
        assert_eq!(convert_japanese_number("二十三"), "23");
        assert_eq!(convert_japanese_number("二十"), "20");
        assert_eq!(convert_japanese_number("十五"), "15");
        assert_eq!(convert_japanese_number("１０字下げ"), "10字下げ");
        // 校正注記の誤コマンド: 「一字下げ」を拾い、後続の 200-14 は無視される
        assert_eq!(
            extract_number_before(&convert_japanese_number("一字下げ忘れか？200-14"), "字下げ"),
            Some(1)
        );
    }

    #[test]
    fn test_extract_number_before_anchors_to_keyword() {
        // 直前の数字だけを取り、後続の別の数字を巻き込まない
        assert_eq!(
            extract_number_before("７字下げ、２１字詰め", "字下げ"),
            Some(7)
        );
        assert_eq!(extract_number_before("２字下げ、", "字下げ"), Some(2));
        assert_eq!(extract_number_before("字下げ", "字下げ"), None);
        assert_eq!(extract_number_before("改行天付き", "字下げ"), None);
    }

    #[test]
    fn test_extract_number_fullwidth() {
        assert_eq!(extract_number("２字下げ"), Some(2));
        assert_eq!(extract_number("３字下げ"), Some(3));
        assert_eq!(extract_number("１０字詰め"), Some(10));
    }
}

/// 句点コード指定（`ローマ数字、1-13-25` など）から外字画像のコードを取り出す。
///
/// 参照実装 aozora2html の `kuten2png` と同じ判定:
/// `「※」は` / `「※」の` を除いたうえで面-区-点の並びを探し、
/// 「非0213外字」を含むもの、`※` が2つ以上あるものは対象外にする。
pub fn parse_kuten_gaiji(spec: &str) -> Option<String> {
    let desc = spec.replace("「※」は", "").replace("「※」の", "");
    if desc.contains("非0213外字") || desc.matches('※').count() >= 2 {
        return None;
    }
    find_kuten_code(&desc)
}

/// 面-区-点（`[12]-\d{1,2}-\d{1,2}`）を探して `1-13-25` の形に整える
fn find_kuten_code(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    for start in 0..chars.len() {
        if !matches!(chars[start], '1' | '2') {
            continue;
        }
        let men = chars[start].to_digit(10)?;
        if chars.get(start + 1) != Some(&'-') {
            continue;
        }
        // \d{1,2} は貪欲。2桁で駄目なら1桁に戻して試す。
        for ku_len in [2usize, 1] {
            let Some((ku, after_ku)) = read_digits(&chars, start + 2, ku_len) else {
                continue;
            };
            if chars.get(after_ku) != Some(&'-') {
                continue;
            }
            for ten_len in [2usize, 1] {
                if let Some((ten, _)) = read_digits(&chars, after_ku + 1, ten_len) {
                    return Some(format!("{men}-{ku:02}-{ten:02}"));
                }
            }
        }
    }
    None
}

/// `pos` から `len` 桁の数字を読む
fn read_digits(chars: &[char], pos: usize, len: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    for offset in 0..len {
        let digit = chars.get(pos + offset)?.to_digit(10)?;
        value = value * 10 + digit;
    }
    Some((value, pos + len))
}
