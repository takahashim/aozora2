//! 後方参照パターンの解析
//!
//! 「対象」に/は/の 装飾 形式のコマンドを解析します。

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::command_parser::CommandResult;

/// 後方参照パターンを解析
pub fn try_parse_reference(content: &str) -> Option<CommandResult> {
    // パターン: 「対象」に/は/の 装飾
    let start = content.find('「')?;
    let end = content.find('」')?;
    if end <= start {
        return None;
    }

    let target = &content[start + '「'.len_utf8()..end];
    let rest = &content[end + '」'.len_utf8()..];

    // 接続詞を探す
    // 「の左に」パターンを優先的にチェック
    let (connector, spec, is_left) = parse_connector(target, rest)?;

    // 句点コード指定は他の記法より先に見る（参照実装 exec_style と同じ順序）
    if super::utils::parse_kuten_gaiji(spec).is_some() {
        return Some(CommandResult::KutenGaiji {
            target: target.to_string(),
            connector: connector.to_string(),
            spec: spec.to_string(),
        });
    }

    // 見出しかどうか
    if connector == "は" {
        if let Some(level) = MidashiLevel::from_command(spec) {
            let style = MidashiStyle::from_command(spec);
            return Some(CommandResult::Midashi {
                target: target.to_string(),
                level,
                style,
            });
        }
    }

    // 装飾タイプを取得
    if let Some(mut style_type) = StyleType::from_command(spec) {
        // 「の左に」パターンの場合は_After変種に変換
        if is_left {
            style_type = style_type.to_after_variant();
        }
        return Some(CommandResult::Style {
            target: target.to_string(),
            connector: connector.to_string(),
            style_type,
        });
    }

    // フォントサイズを取得（「対象」は/のN段階大きな/小さな文字）
    if let Some((size_type, level)) = FontSizeType::from_command(spec) {
        return Some(CommandResult::FontSize {
            target: target.to_string(),
            size_type,
            level,
        });
    }

    // 注記ルビ（「対象」に「注記」の注記）
    // ただし「の左に」パターンは対象外（注記として出力）
    if !is_left {
        if let Some(result) = try_parse_annotation_ruby(target, spec) {
            return Some(result);
        }
    }

    // インライン要素
    if let Some(result) = try_parse_inline_element(target, spec) {
        return Some(result);
    }

    None
}

/// 注記ルビパターンを解析（「対象」に「注記」の注記）
fn try_parse_annotation_ruby(target: &str, spec: &str) -> Option<CommandResult> {
    // パターン: 「注記内容」の注記
    if !spec.ends_with("の注記") {
        return None;
    }

    // 「」で囲まれた注記内容を抽出
    let start = spec.find('「')?;
    let end = spec.find('」')?;
    if end <= start {
        return None;
    }

    let annotation = &spec[start + '「'.len_utf8()..end];
    Some(CommandResult::AnnotationRuby {
        target: target.to_string(),
        annotation: annotation.to_string(),
    })
}

/// 接続詞を解析し、(接続詞, 仕様部分, 左ルビフラグ) を返す
fn parse_connector<'a>(_target: &str, rest: &'a str) -> Option<(&'static str, &'a str, bool)> {
    // 接続詞は「対象」の直後に来る。文字列のどこかにあればよいことにすると、
    // 「麾」の「毛」に代えて… のような注記で区切り位置を取り違える。
    if let Some(spec) = rest.strip_prefix("の左に") {
        if rest.contains("のルビ") {
            // 左ルビパターンは別処理で返す
            return None;
        }
        return Some(("の左に", spec, true));
    }

    for connector in ["に", "は", "の"] {
        if let Some(spec) = rest.strip_prefix(connector) {
            return Some((connector, spec, false));
        }
    }
    None
}

/// インライン要素（縦中横、罫囲み、横組み、キャプション）を解析
fn try_parse_inline_element(target: &str, spec: &str) -> Option<CommandResult> {
    // 参照実装 exec_style は command.match? で部分一致を見る。ただし他の記法と
    // 誤って重なるのを避けるため、ここでは末尾一致に留める
    // （「２日～10日の下に縦中横」のような前置き付きを拾うのが目的）。
    if spec.ends_with("縦中横") {
        return Some(CommandResult::InlineTcy {
            target: target.to_string(),
        });
    }
    if spec.ends_with("横組み") {
        return Some(CommandResult::InlineYokogumi {
            target: target.to_string(),
        });
    }
    if spec.ends_with("罫囲み") {
        return Some(CommandResult::InlineKeigakomi {
            target: target.to_string(),
        });
    }
    if spec.ends_with("キャプション") {
        return Some(CommandResult::InlineCaption {
            target: target.to_string(),
        });
    }
    None
}

/// 左ルビパターンを解析
pub fn try_parse_left_ruby(content: &str) -> Option<CommandResult> {
    // パターン: 「親文字」の左に「ルビ」のルビ
    let start = content.find('「')?;
    let first_end = content.find('」')?;
    if first_end <= start {
        return None;
    }

    let target = &content[start + '「'.len_utf8()..first_end];
    let rest = &content[first_end + '」'.len_utf8()..];

    if !rest.contains("の左に") || !rest.contains("のルビ") {
        return None;
    }

    // ルビ部分を抽出
    let ruby_start = rest.find('「')?;
    let ruby_end = rest.rfind('」')?;
    if ruby_end <= ruby_start {
        return None;
    }

    let ruby = &rest[ruby_start + '「'.len_utf8()..ruby_end];
    Some(CommandResult::LeftRuby {
        target: target.to_string(),
        ruby: ruby.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_style_bouten() {
        let result = try_parse_reference("「である」に傍点");
        assert_eq!(
            result,
            Some(CommandResult::Style {
                target: "である".to_string(),
                connector: "に".to_string(),
                style_type: StyleType::SesameDot,
            })
        );
    }

    #[test]
    fn test_parse_midashi() {
        let result = try_parse_reference("「第一章」は大見出し");
        assert_eq!(
            result,
            Some(CommandResult::Midashi {
                target: "第一章".to_string(),
                level: MidashiLevel::O,
                style: MidashiStyle::Normal,
            })
        );
    }

    #[test]
    fn test_parse_inline_tcy() {
        let result = try_parse_reference("「12」は縦中横");
        assert_eq!(
            result,
            Some(CommandResult::InlineTcy {
                target: "12".to_string(),
            })
        );
    }

    #[test]
    fn test_parse_left_ruby() {
        let result = try_parse_left_ruby("「親文字」の左に「ルビ」のルビ");
        assert_eq!(
            result,
            Some(CommandResult::LeftRuby {
                target: "親文字".to_string(),
                ruby: "ルビ".to_string(),
            })
        );
    }
}
