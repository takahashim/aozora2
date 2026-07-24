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
    fn test_extract_number_before_anchors_to_keyword() {
        // 直前の数字だけを取り、後続の別の数字を巻き込まない
        assert_eq!(extract_number_before("７字下げ、２１字詰め", "字下げ"), Some(7));
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
