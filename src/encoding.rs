//! エンコーディング判定・変換

use encoding_rs::SHIFT_JIS;

/// バイト列のエンコーディングを判定してUTF-8文字列に変換
///
/// # 判定ロジック
/// 1. UTF-8 BOMがあればUTF-8
/// 2. UTF-8として妥当ならUTF-8
/// 3. それ以外はShift_JIS
///
/// # Examples
///
/// ```
/// use aozora2text::encoding::decode_to_utf8;
///
/// let utf8_bytes = "こんにちは".as_bytes();
/// assert_eq!(decode_to_utf8(utf8_bytes), "こんにちは");
/// ```
pub fn decode_to_utf8(bytes: &[u8]) -> String {
    // BOMチェック
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..] // BOMをスキップ
    } else {
        bytes
    };

    // UTF-8として妥当かチェック
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }

    // Shift_JISとしてデコード
    let (cow, _, _) = SHIFT_JIS.decode(bytes);
    cow.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8() {
        let bytes = "こんにちは".as_bytes();
        assert_eq!(decode_to_utf8(bytes), "こんにちは");
    }

    #[test]
    fn test_utf8_with_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("こんにちは".as_bytes());
        assert_eq!(decode_to_utf8(&bytes), "こんにちは");
    }

    #[test]
    fn test_shift_jis() {
        // "こんにちは" in Shift_JIS
        let bytes = vec![
            0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD,
        ];
        assert_eq!(decode_to_utf8(&bytes), "こんにちは");
    }
}
