//! 特殊コンテンツの解析
//!
//! 画像、返り点、送り仮名などの特殊コマンドを解析します。

use super::command_parser::CommandResult;
use once_cell::sync::Lazy;
use regex::Regex;

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
    let alt = strip_accent_from_alt(&extract_alt_text(desc_part));

    Some(CommandResult::Image {
        filename,
        // 参照実装 exec_img_command は説明に「写真」が入っていれば写真扱い。
        is_photo: alt.contains("写真"),
        alt,
        width,
        height,
    })
}

/// 画像ファイル名かどうかをチェック
fn is_image_filename(filename: &str) -> bool {
    filename.ends_with(".png") || filename.ends_with(".jpg") || filename.ends_with(".gif")
}

/// 参照実装 dispatch_aozora_command の `/fig(\d)+_(\d)+\.png/` に相当する。
/// 注記内に `fig<数字>_<数字>.png` を含むかどうか（画像コマンド判定用）。
/// 参照はこのパターンで画像ルートへ回すため、`PAT_IMAGE`（`）入る`）が
/// 末尾アンカー無しで一致すれば、`入る` の後ろに `。` 等が続いても画像化される。
/// Ruby(SJIS) の `\d` は ASCII 0-9 のみなので `[0-9]` で固定する。
static PAT_FIG_PNG: Lazy<Regex> = Lazy::new(|| Regex::new(r"fig[0-9]+_[0-9]+\.png").unwrap());

pub fn contains_fig_png(content: &str) -> bool {
    PAT_FIG_PNG.is_match(content)
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
/// 画像 alt からアクセント 〔...〕 の中身を落とす。
///
/// 参照実装の画像 alt は TagParser の @raw（read_char で読んだ生文字の蓄積）由来。
/// アクセント 〔...〕 は専用サブパーサ（AccentParser）が別ストリームで読むため
/// 中身が @raw に入らず、read_char で読んだ開き 〔 だけが残り、中身と閉じ 〕 は
/// 落ちる（例: 折〔衷〕派 → 折〔派、〔金銭出納録〕の表 → 〔の表）。対応する 〕 が
/// 無ければアクセントとして消費されず残る。
fn strip_accent_from_alt(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i] == '〔' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '〕') {
                // 中身と閉じ 〕 を捨てる
                i = i + 1 + rel + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

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
    // 参照実装 PAT_KAERITEN = ^[一二三四五六七八九十レ上中下甲乙丙丁天地人]+$。
    // 従来は五〜十が欠落し、さらに参照に無い「4文字まで」の長さ制限があったため、
    // ［＃五］や長い返り点（一レ二 等）を返り点にできず注記化していた。
    const KAERITEN_CHARS: &[char] = &[
        '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', 'レ', '上', '中', '下', '甲',
        '乙', '丙', '丁', '天', '地', '人',
    ];

    // 1文字以上で、すべての文字が返り点文字（長さ上限なし＝参照の `+`）。
    !content.is_empty() && content.chars().all(|c| KAERITEN_CHARS.contains(&c))
}

/// 訓点送り仮名を解析
pub fn try_parse_okurigana(content: &str) -> Option<String> {
    // 参照実装 PAT_OKURIGANA = ^（(.+)）$ は括弧内が1文字以上なら長さ無制限で
    // 送り仮名にする（内側に外字 ※［＃…］ 等を含んでもよい。内側は注記と同じ
    // TagParser で描画される）。従来は 10 文字までに絞っていたため、
    // ［＃（※［＃「低のつくり」、第3水準1-86-47］）］ のような外字入り送り仮名を
    // 取りこぼして注記化していた。
    if content.starts_with('（') && content.ends_with('）') {
        let inner = &content['（'.len_utf8()..content.len() - '）'.len_utf8()];
        if !inner.is_empty() {
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
        // 参照 PAT_KAERITEN は五〜十も含み、長さ上限もない。
        assert!(is_kaeriten("五"));
        assert!(is_kaeriten("十"));
        assert!(is_kaeriten("一二三四五")); // 長さ制限なし（従来は false だった）
        assert!(is_kaeriten("一レ"));
        assert!(!is_kaeriten("あ"));
        assert!(!is_kaeriten(""));
        assert!(!is_kaeriten("一あ")); // 返り点以外を含めば false
    }

    #[test]
    fn test_try_parse_okurigana() {
        assert_eq!(try_parse_okurigana("（ノ）"), Some("ノ".to_string()));
        assert_eq!(try_parse_okurigana("（テ）"), Some("テ".to_string()));
        assert_eq!(try_parse_okurigana("テスト"), None);
        // 参照実装 PAT_OKURIGANA = ^（(.+)）$ は長さ無制限。外字入りも送り仮名に
        // する（内側は描画時に注記と同じ TagParser で img 化される）。
        assert_eq!(
            try_parse_okurigana("（※［＃「低のつくり」、第3水準1-86-47］）"),
            Some("※［＃「低のつくり」、第3水準1-86-47］".to_string())
        );
        // 空括弧は送り仮名にしない。
        assert_eq!(try_parse_okurigana("（）"), None);
    }

    #[test]
    fn test_try_parse_image() {
        let result = try_parse_image("挿絵（fig001.png、横100×縦200）入る");
        assert!(result.is_some());
        if let Some(CommandResult::Image {
            filename,
            alt,
            is_photo,
            width,
            height,
        }) = result
        {
            assert_eq!(filename, "fig001.png");
            assert_eq!(alt, "挿絵");
            assert!(!is_photo);
            assert_eq!(width, Some(100));
            assert_eq!(height, Some(200));
        }
    }

    /// 参照実装 exec_img_command は説明に「写真」が入っていれば写真扱いにする。
    /// この判定はコマンド解析側の責務（描画時の CSS クラス選択はレンダラ）。
    #[test]
    fn test_try_parse_image_detects_photo() {
        let Some(CommandResult::Image { is_photo, alt, .. }) =
            try_parse_image("旅先の写真（fig001.png）入る")
        else {
            panic!("画像として解析されなかった");
        };
        assert_eq!(alt, "旅先の写真");
        assert!(is_photo);
    }

    #[test]
    fn test_contains_fig_png() {
        // 参照 /fig\d+_\d+\.png/ 相当。
        assert!(contains_fig_png("楽譜（fig4206_02.png）入る。"));
        assert!(contains_fig_png("（fig58704_49.png、横600×縦515）入る"));
        // 数字_数字 でないものは不一致。
        assert!(!contains_fig_png("挿絵（fig001.png）入る"));
        assert!(!contains_fig_png("figure.png"));
        assert!(!contains_fig_png("本文に画像が入る"));
    }

    #[test]
    fn test_try_parse_image_trailing_char() {
        // PAT_IMAGE は末尾アンカー無し。`入る` の後に `。` が続いても画像化する
        // （参照実装 4206 のケース）。
        let result =
            try_parse_image("四分音符ミファレに「ネーヱ」の歌詞の楽譜（fig4206_02.png）入る。");
        match result {
            Some(CommandResult::Image {
                filename,
                alt,
                is_photo,
                width,
                height,
            }) => {
                assert_eq!(filename, "fig4206_02.png");
                assert_eq!(alt, "四分音符ミファレに「ネーヱ」の歌詞の楽譜");
                assert!(!is_photo);
                assert_eq!(width, None);
                assert_eq!(height, None);
            }
            other => panic!("画像として解析されない: {other:?}"),
        }
    }

    #[test]
    fn test_strip_accent_from_alt() {
        // 開き 〔 は残し、中身と閉じ 〕 を落とす（参照実装 @raw の挙動）。
        assert_eq!(strip_accent_from_alt("折〔衷〕派"), "折〔派");
        assert_eq!(strip_accent_from_alt("〔金銭出納録〕の表"), "〔の表");
        assert_eq!(strip_accent_from_alt("普通の説明"), "普通の説明");
        // 対応する 〕 が無ければそのまま残す。
        assert_eq!(strip_accent_from_alt("未閉じ〔あ"), "未閉じ〔あ");
    }

    #[test]
    fn test_image_alt_drops_accent_content() {
        // 画像 alt はアクセント 〔...〕 の中身を落とす。
        let result = try_parse_image("折〔衷〕派の図（fig001.png、横1×縦1）入る");
        if let Some(CommandResult::Image { alt, .. }) = result {
            assert_eq!(alt, "折〔派の図");
        } else {
            panic!("画像として解析されなかった");
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
