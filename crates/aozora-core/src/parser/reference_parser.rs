//! 後方参照パターンの解析
//!
//! 「対象」に/は/の 装飾 形式のコマンドを解析します。

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::command_parser::CommandResult;

/// 参照実装の PAT_FRONTREF が対象部分に許す形
/// `[^「」]*(?:「.+」)*[^「」]*` かどうか。
///
/// 対象の中に「」の組をひとつ含められる（「魔境「蕨の切り株」」は中見出し など）。
fn is_reference_target(s: &str) -> bool {
    match (s.find('「'), s.rfind('」')) {
        // 「」をまったく含まない
        (None, None) => true,
        // 閉じだけがあるのは対象の途中で切れている
        (None, Some(_)) => false,
        (Some(open), Some(close)) if close > open => {
            // 最初の「より前と、最後の」より後には「」を含まない
            !s[..open].contains('」') && !s[close + '」'.len_utf8()..].contains(['「', '」'])
        }
        _ => false,
    }
}

/// 対象の終わりの `」` を探す。参照実装の正規表現は貪欲なので、
/// 接続詞が続きかつ対象の形が成立する位置のうち最も後ろを選ぶ。
fn find_target_end(content: &str, start: usize) -> Option<usize> {
    content[start..]
        .match_indices('」')
        .map(|(i, _)| start + i)
        .filter(|&i| {
            let rest = &content[i + '」'.len_utf8()..];
            ["に", "は", "の"].iter().any(|c| rest.starts_with(c))
                && is_reference_target(&content[start..i])
        })
        .next_back()
}

/// 後方参照パターンを解析
///
/// 参照実装 PAT_FRONTREF は文字列全体への正規表現照合で、最初の 「 から全体が
/// 一致しなければ次の 「 から再試行する（Ruby 正規表現のバックトラック）。
/// 例:「「古典的」は太字 は最初の 「 だと対象「「古典的が不均衡で一致しないので、
/// 2番目の 「 から対象「古典的」で一致する（先頭の 「 は本文に残る）。
///
/// ただし入れ子コマンド ［…］ の中の 「 は対象候補にしない。参照実装では入れ子の
/// ［＃…］ が先に解決されて外側の正規表現からは 「 が見えないため。これを守らないと
/// ［＃「う［＃「う」に「ママ」の注記］」はママ］ の内側注記を誤って解決してしまう。
pub fn try_parse_reference(content: &str) -> Option<CommandResult> {
    let mut pos = 0;
    while pos < content.len() {
        let ch = content[pos..].chars().next().unwrap();
        if ch == '［' {
            // 入れ子コマンド ［…］（入れ子可）を飛ばす。
            pos = skip_bracketed(content, pos);
            continue;
        }
        if ch == '「' {
            let start = pos + '「'.len_utf8();
            // find_target_end が成立する位置＝参照実装の正規表現が「対象」「[にはの]」
            // まで一致する位置。ここで確定する（＝正規表現がマッチした位置なので
            // 再試行はしない）。スタイルが無ければ None を返し注記になる。対象が
            // 不均衡で find_target_end が None のときだけ次の 「 へ再試行する。
            // 例:「「古典的」は太字 は最初の 「 で不成立→次の 「 で成立。
            //   ２文字目の「i」は下付き小文字、… は最初の 「 で成立するが下付き…、
            //   の spec がスタイル不成立なので注記（次の 「 へは行かない）。
            if find_target_end(content, start).is_some() {
                return try_parse_reference_at(content, start);
            }
        }
        pos += ch.len_utf8();
    }
    None
}

/// `［` から対応する `］` の直後までのバイト位置を返す（入れ子対応）。
/// 対応する `］` が無ければ末尾を返す。
fn skip_bracketed(content: &str, open_pos: usize) -> usize {
    let mut depth = 0usize;
    let mut pos = open_pos;
    for ch in content[open_pos..].chars() {
        match ch {
            '［' => depth += 1,
            '］' => {
                depth -= 1;
                if depth == 0 {
                    return pos + ch.len_utf8();
                }
            }
            _ => {}
        }
        pos += ch.len_utf8();
    }
    content.len()
}

fn try_parse_reference_at(content: &str, start: usize) -> Option<CommandResult> {
    let end = find_target_end(content, start)?;

    let target = &content[start..end];
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

    // 以降の順序は参照実装 exec_style に合わせる。
    // 縦中横などのインライン要素は見出し・装飾より先に見る。
    if !is_left {
        if let Some(result) = try_parse_inline_element(target, spec) {
            return Some(result);
        }
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

    // フォントサイズ（「対象」は/のN段階大きな/小さな文字）
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

    // 装飾タイプ
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

    None
}

/// 注記ルビパターンを解析（「対象」に「注記」の注記）
fn try_parse_annotation_ruby(target: &str, spec: &str) -> Option<CommandResult> {
    // 参照実装 PAT_CHUUKI = /「(.+?)」の注記/ は spec 全体に対する**部分一致**で、
    // 「の注記」の後ろに続きがあってもよい（例:「（ママ）」の注記、正しくは「十三」／
    // 「（ママ）」の注記がある）。従来は spec.ends_with("の注記") に限っていたため、
    // 末尾に続きがある注記ルビを取りこぼして注記化していた。
    if !spec.contains("」の注記") {
        return None;
    }

    // 「」で囲まれた注記内容を抽出する。参照実装の /「(.+?)」の注記/ と同じく、
    // 「」の注記」が続く位置まで伸ばす。注記の中に「」が入れ子になっていても切らない。
    let start = spec.find('「')? + '「'.len_utf8();
    let end = spec[start..]
        .match_indices("」の注記")
        .map(|(i, _)| start + i)
        .next()?;
    if end <= start {
        return None;
    }

    let annotation = &spec[start..end];
    // 抽出した注記内容に入れ子コマンド ［＃ が含まれる場合は注記ルビにしない。
    // 参照実装は入れ子の ［＃…］ を先に解決してから PAT_CHUUKI を当てるので、
    // 例:「書卓が」は底本では「□□［＃「□□」に「二字欠落」の注記］が」 のような
    // 入れ子注記を持つ命令は外側の「」の注記」に一致しない（＝外側は注記のまま）。
    // こちらは平坦な文字列照合なので、注記内容に ［＃ があれば入れ子とみなし除外する。
    if annotation.contains("［＃") {
        return None;
    }
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
    // 参照実装 exec_style は command.match? で部分一致を見る。
    // 「縦中横、行右小書き」のように後ろに指定が続くものも拾う。
    if spec.contains("縦中横") {
        return Some(CommandResult::InlineTcy {
            target: target.to_string(),
        });
    }
    if spec.contains("横組み") {
        return Some(CommandResult::InlineYokogumi {
            target: target.to_string(),
        });
    }
    if spec.contains("罫囲み") {
        return Some(CommandResult::InlineKeigakomi {
            target: target.to_string(),
        });
    }
    if spec.contains("キャプション") {
        return Some(CommandResult::InlineCaption {
            target: target.to_string(),
        });
    }
    // 参照実装 exec_frontref_command はキャプションの後に 返り点・訓点送り仮名 を
    // 前方参照として処理する（「レ」は返り点 → <sub class="kaeriten">レ</sub>）。
    if spec.contains("返り点") {
        return Some(CommandResult::InlineKaeriten {
            target: target.to_string(),
        });
    }
    if spec.contains("訓点送り仮名") {
        return Some(CommandResult::InlineOkurigana {
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
    use crate::node::StyleType;

    /// 前方参照のバックトラック（参照実装 PAT_FRONTREF 相当）。
    #[test]
    fn test_reference_backtracks_on_unbalanced_target() {
        // 最初の 「 だと対象が不均衡（「「古典的）で不成立→次の 「 で 古典的 に成立。
        assert_eq!(
            try_parse_reference("「「古典的」は太字"),
            Some(CommandResult::Style {
                target: "古典的".to_string(),
                connector: "は".to_string(),
                style_type: StyleType::Bold,
            })
        );
    }

    #[test]
    fn test_reference_commits_to_first_valid_bracket() {
        // 最初の 「i」 で対象・接続詞は成立するので確定。spec「下付き小文字、４文字目
        // の「i」は上付き小文字」はスタイル不成立なので注記（次の 「 へ再試行しない）。
        assert_eq!(
            try_parse_reference("２文字目の「i」は下付き小文字、４文字目の「i」は上付き小文字"),
            None
        );
    }

    #[test]
    fn test_reference_skips_bracket_nested_command() {
        // 入れ子コマンド ［＃…］ の中の 「 は対象候補にしない。外側「う［＃…］」はママ
        // は spec ママ がスタイル不成立→注記（内側の注記ルビを誤解決しない）。
        assert_eq!(
            try_parse_reference("「う［＃「う」に「ママ」の注記］」はママ"),
            None
        );
    }

    /// 注記ルビ 「対象」に「注記」の注記 は、「の注記」の後ろに続きがあっても
    /// PAT_CHUUKI（部分一致）で成立する。入れ子の ［＃ を含む注記は除外する。
    #[test]
    fn test_annotation_ruby_substring_and_nested_guard() {
        // 末尾に続きがあっても成立（「（ママ）」の注記、正しくは「十三」）。
        assert_eq!(
            try_parse_annotation_ruby("十四", "「（ママ）」の注記、正しくは「十三」"),
            Some(CommandResult::AnnotationRuby {
                target: "十四".to_string(),
                annotation: "（ママ）".to_string(),
            })
        );
        // 「がある」等が続く形も成立。
        assert_eq!(
            try_parse_annotation_ruby("衍", "「（ママ）」の注記がある"),
            Some(CommandResult::AnnotationRuby {
                target: "衍".to_string(),
                annotation: "（ママ）".to_string(),
            })
        );
        // 注記内容に入れ子 ［＃ を含む場合（底本では…の入れ子注記）は成立させない。
        assert_eq!(
            try_parse_annotation_ruby(
                "書卓が",
                "底本では「□□［＃「□□」に「二字欠落」の注記］が」"
            ),
            None
        );
    }

    /// 対象に「」の組を含む前方参照。参照実装の PAT_FRONTREF は
    /// [^「」]*(?:「.+」)*[^「」]* を対象として許す。
    #[test]
    fn test_reference_target_can_contain_nested_brackets() {
        assert_eq!(
            try_parse_reference("「魔境「蕨の切り株」」は中見出し"),
            Some(CommandResult::Midashi {
                target: "魔境「蕨の切り株」".to_string(),
                level: MidashiLevel::Naka,
                style: MidashiStyle::Normal,
            })
        );
    }

    /// 接続詞のあとに「」が続く形（注記など）では対象を伸ばさない
    #[test]
    fn test_reference_target_stops_before_the_connector() {
        assert_eq!(
            try_parse_reference("「喋」に「ママ」の注記"),
            Some(CommandResult::AnnotationRuby {
                target: "喋".to_string(),
                annotation: "ママ".to_string(),
            })
        );
    }

    #[test]
    fn test_is_reference_target() {
        assert!(is_reference_target("魔境「蕨の切り株」"));
        assert!(is_reference_target("ふつうの文字列"));
        assert!(is_reference_target(""));
        assert!(!is_reference_target("A」に「B"));
        assert!(!is_reference_target("閉じだけ」"));
    }

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
