//! ブロック開始/終了の解析
//!
//! 「ここから...」「ここで...終わり」形式のコマンドを解析します。

use crate::node::{BlockParams, BlockType, FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::command_parser::CommandResult;
use super::utils::{convert_japanese_number, extract_number, extract_number_before};

/// ブロック開始を解析
pub fn parse_block_start(content: &str) -> CommandResult {
    let content = content.trim_start_matches("ここから");

    // 参照実装 exec_block_start_command は割り注を扱わないので注記のまま出す
    // （インラインの ［＃割り注］…［＃割り注終わり］ だけが割り注になる）
    if content.contains("割り注") {
        return CommandResult::Note(format!("ここから{content}"));
    }
    let mut params = BlockParams::default();
    params.is_block = true; // ここから pattern is block-level

    // ぶら下げパターン: 「N字下げ、折り返してM字下げ」または「改行天付き、折り返してN字下げ」
    if content.contains("折り返して") {
        if let Some(result) = try_parse_burasage(content, &mut params) {
            return result;
        }
    }

    // 幅を抽出。参照実装の apply_jisage は `(\d*)字下げ` のように数字を単位
    // キーワードに固定して読むので、こちらも「字下げ／字詰め／字上げ」直前の
    // 数字を取る。これで `３字下げ、地より１字上げ`（→3。31 にしない）や
    // `２字下げ、地から３字下げ`（→2。23 にしない）を正しく解析する。
    // どの単位も無い場合（段階指定など）は従来どおり全体から数字を拾う。
    // 漢数字（一字下げ 等）も参照実装同様に数字化してから読む。
    let normalized = convert_japanese_number(content);
    // 単位キーワード（字下げ/字詰め/字上げ）を含むなら、参照実装 `(\d*)字下げ` 等に
    // 合わせて**そのキーワード直前の隣接数字だけ**を幅にする。隣接数字が無ければ
    // 空幅（None）＝タグは jisage_ / margin-left: em になる（例:「３　字下げ」の
    // ように全角空白で離れていると空幅）。単位キーワードが無いとき（段階指定など）
    // だけ全体から数字を拾う。従来は空白で離れた数字を extract_number で拾って
    // しまい 3字下げ 扱いになっていた。
    let has_unit = normalized.contains("字下げ")
        || normalized.contains("字詰め")
        || normalized.contains("字上げ");
    if has_unit {
        params.width = extract_number_before(&normalized, "字下げ")
            .or_else(|| extract_number_before(&normalized, "字詰め"))
            .or_else(|| extract_number_before(&normalized, "字上げ"));
    } else if let Some(width) = extract_number(&normalized) {
        params.width = Some(width);
    }

    // 段階を抽出
    if content.contains("段階") {
        if let Some(size) = extract_number(content) {
            params.font_size = Some(size);
        }
    }

    // ブロックタイプを判定
    if let Some(block_type) = BlockType::from_command(content) {
        // 見出しの場合はレベルも設定
        if block_type == BlockType::Midashi {
            params.level = MidashiLevel::from_command(content);
        }
        CommandResult::BlockStart { block_type, params }
    } else {
        CommandResult::Note(format!("ここから{content}"))
    }
}

/// ぶら下げパターンを解析。参照実装 dispatch_aozora_command は
/// `折り返して`（ORIKAESHI_COMMAND）を含むコマンドを他のどの分岐より先に
/// apply_burasage へ回す。`ここから` の有無に関係なくぶら下げになるので、
/// parse_command からも直接呼べるよう pub にしている。
pub fn try_parse_burasage(content: &str, params: &mut BlockParams) -> Option<CommandResult> {
    // 参照実装 apply_burasage は先に漢数字を数字化してからパターン照合する。
    let content = convert_japanese_number(content);
    let parts: Vec<&str> = content.split("折り返して").collect();
    if parts.len() != 2 {
        return None;
    }

    let first_part = parts[0];
    let second_part = parts[1];

    // 参照実装 apply_burasage の3分岐を再現する。折り返し幅・字下げ幅は
    // ともに「字下げ」直前の数字（`折り返して(\d*)字下げ` / `(\d*)字下げ`）。
    if first_part.contains("天付き") {
        // 改行天付き、折り返してN字下げ:
        //   PAT_ORIKAESHI_JISAGE = 折り返して(\d*)字下げ
        //   margin-left = N, text-indent = -N（width=0）
        params.wrap_width = extract_number_before(second_part, "字下げ");
        params.width = Some(0);
    } else if first_part.ends_with("字下げ、") && second_part.contains("字下げ") {
        // N字下げ、折り返してM字下げ（コンマあり）:
        //   PAT_ORIKAESHI_JISAGE2 = (\d*)字下げ、折り返して(\d*)字下げ
        //   margin-left = M, text-indent = N-M
        // 参照実装の正規表現は「字下げ、折り返して…字下げ」と読点（、）を
        // 必須にするので、first_part が「字下げ、」で終わり second_part に
        // 「字下げ」があるときだけこの分岐にする。
        params.wrap_width = extract_number_before(second_part, "字下げ");
        params.width = extract_number_before(first_part, "字下げ");
    } else {
        // 折り返してM字下げ（コンマなし・天付きなし。例:「１０字下げ折り返して
        // １７字下げ」）: 参照実装は PAT_ORIKAESHI_JISAGE2 に一致せず、
        // margin-left が空・text-indent 0 になる。
        params.wrap_width = None;
        params.width = None;
    }

    Some(CommandResult::BlockStart {
        block_type: BlockType::Burasage,
        params: params.clone(),
    })
}

/// ブロック終了を解析
pub fn parse_block_end(content: &str) -> CommandResult {
    // content は原文の全体（例: "ここで字下げ終わり" / "ここで字下げおわり"）。
    // 参照実装は「ここで割り注終わり」も扱わないので注記のまま出す
    if content.contains("割り注") {
        return CommandResult::Note(content.to_string());
    }
    let inner = content
        .trim_start_matches("ここで")
        .trim_end_matches("終わり")
        .trim_end_matches("おわり");

    if let Some(block_type) = BlockType::from_command(inner) {
        CommandResult::BlockEnd {
            block_type,
            explicit: true,
        }
    } else {
        // キーワードに合致しなければ原文をそのまま注記化する
        CommandResult::Note(content.to_string())
    }
}

/// インライン終了を解析
pub fn parse_inline_end(content: &str) -> CommandResult {
    let content = content.trim_end_matches("終わり");

    // 固定パターン
    match content {
        "縦中横" => return CommandResult::TcyEnd,
        "割り注" => return CommandResult::WarigakiEnd,
        "キャプション" => return CommandResult::CaptionEnd,
        _ => {}
    }

    // 装飾終了
    if let Some(style_type) = StyleType::from_command(content) {
        return CommandResult::StyleEnd { style_type };
    }

    // ブロック終了（bare ［＃…終わり］形式）
    if let Some(block_type) = BlockType::from_command(content) {
        return CommandResult::BlockEnd {
            block_type,
            explicit: false,
        };
    }

    CommandResult::Note(format!("{content}終わり"))
}

/// 行単位字下げを解析
pub fn try_parse_line_indent(content: &str) -> Option<CommandResult> {
    if !content.contains("字下げ") {
        return None;
    }

    // 参照実装 apply_jisage は convert_japanese_number 後に `(\d*)字下げ` を取る。
    // 漢数字「一字下げ」も 1 として読み、`一字下げ忘れか？200-14` のような
    // 校正注記も参照実装同様 jisage_1 になる（後続の 200-14 は巻き込まない）。
    let normalized = convert_japanese_number(content);
    let width = extract_number_before(&normalized, "字下げ")
        .or_else(|| extract_number(&normalized))?;
    Some(CommandResult::LineIndent { width })
}

/// 行単位地付き/地からを解析
pub fn try_parse_line_chitsuki(content: &str) -> Option<CommandResult> {
    // 参照実装の PAT_CHITSUKI は /(地付き|字上げ)(終わり)*$/ なので、
    // 「地から」に限らず「地より２字上げ」なども地付き扱いになる。
    let body = content.trim_end_matches("終わり");

    if body.ends_with("地付き") {
        return Some(CommandResult::LineChitsuki { width: 0 });
    }

    if body.ends_with("字上げ") {
        let width = extract_number(body).unwrap_or(0);
        return Some(CommandResult::LineChitsuki { width });
    }

    None
}

/// 見出し開始を解析（ブロック外の見出し）
pub fn try_parse_midashi_start(content: &str) -> Option<CommandResult> {
    let level = MidashiLevel::from_command(content)?;
    let style = MidashiStyle::from_command(content);
    let mut params = BlockParams::default();
    params.level = Some(level);
    params.midashi_style = Some(style);
    Some(CommandResult::BlockStart {
        block_type: BlockType::Midashi,
        params,
    })
}

/// インラインフォントサイズ開始を解析
pub fn try_parse_font_size_start(content: &str) -> Option<CommandResult> {
    let (size_type, level) = FontSizeType::from_command(content)?;
    let mut params = BlockParams::default();
    params.font_size = Some(level);
    Some(CommandResult::BlockStart {
        block_type: match size_type {
            FontSizeType::Dai => BlockType::FontDai,
            FontSizeType::Sho => BlockType::FontSho,
        },
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 参照実装は「ここから割り注」を扱わないので注記のまま出す。
    /// 割り注になるのはインラインの ［＃割り注］…［＃割り注終わり］ だけ。
    #[test]
    fn test_block_warichu_stays_a_note() {
        assert_eq!(
            parse_block_start("ここから割り注"),
            CommandResult::Note("ここから割り注".to_string())
        );
        assert_eq!(
            parse_block_end("ここで割り注終わり"),
            CommandResult::Note("ここで割り注終わり".to_string())
        );
        // インラインの終了は従来どおり割り注として扱う
        assert_eq!(parse_inline_end("割り注終わり"), CommandResult::WarigakiEnd);
    }

    /// 地付きは「地から」に限らず末尾が「字上げ」なら成立する
    #[test]
    fn test_line_chitsuki_accepts_any_jiage_suffix() {
        assert_eq!(
            try_parse_line_chitsuki("地より２字上げ"),
            Some(CommandResult::LineChitsuki { width: 2 })
        );
        assert_eq!(
            try_parse_line_chitsuki("地から3字上げ"),
            Some(CommandResult::LineChitsuki { width: 3 })
        );
        assert_eq!(
            try_parse_line_chitsuki("地付き"),
            Some(CommandResult::LineChitsuki { width: 0 })
        );
        assert_eq!(try_parse_line_chitsuki("ここから２字下げ"), None);
    }

    #[test]
    fn test_burasage_requires_comma_for_jisage2() {
        // コンマあり「N字下げ、折り返してM字下げ」: margin=M, text-indent=N-M。
        let result = parse_block_start("ここから１０字下げ、折り返して１７字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Burasage,
                params: BlockParams {
                    width: Some(10),
                    wrap_width: Some(17),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
        // コンマなし「N字下げ折り返してM字下げ」: 参照実装の PAT_ORIKAESHI_JISAGE2
        // は読点を必須にするため一致せず、margin-left 空・text-indent 0（width も
        // wrap_width も None）になる。
        let result = parse_block_start("ここから１０字下げ折り返して１７字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Burasage,
                params: BlockParams {
                    width: None,
                    wrap_width: None,
                    is_block: true,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn test_parse_block_start_jisage() {
        let result = parse_block_start("ここから2字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Jisage,
                params: BlockParams {
                    width: Some(2),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn test_parse_block_end() {
        let result = parse_block_end("ここで字下げ終わり");
        assert_eq!(
            result,
            CommandResult::BlockEnd {
                block_type: BlockType::Jisage,
                explicit: true,
            }
        );
    }

    #[test]
    fn test_parse_line_indent() {
        let result = try_parse_line_indent("3字下げ");
        assert_eq!(result, Some(CommandResult::LineIndent { width: 3 }));
    }

    #[test]
    fn test_parse_line_chitsuki() {
        let result = try_parse_line_chitsuki("地付き");
        assert_eq!(result, Some(CommandResult::LineChitsuki { width: 0 }));

        let result = try_parse_line_chitsuki("地から3字上げ");
        assert_eq!(result, Some(CommandResult::LineChitsuki { width: 3 }));
    }

    #[test]
    fn test_parse_inline_end() {
        assert_eq!(parse_inline_end("縦中横終わり"), CommandResult::TcyEnd);
        assert_eq!(parse_inline_end("割り注終わり"), CommandResult::WarigakiEnd);
    }
}
