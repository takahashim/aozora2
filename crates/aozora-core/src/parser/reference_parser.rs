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
        .rfind(|&i| {
            let rest = &content[i + '」'.len_utf8()..];
            ["に", "は", "の"].iter().any(|c| rest.starts_with(c))
                && is_reference_target(&content[start..i])
        })
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
    // 参照実装の dispatch は前方参照を PAT_REF = /^「.+」/ で判定する。すなわち
    // コマンドは「で始まらなければ前方参照にならない（注記になる）。前置きのある
    // 「二つ目、三つ目の「？」は太字」や「２文字目の「i」…」は先頭が「でないので
    // 前方参照にせず注記化する。
    if !content.starts_with('「') {
        return None;
    }
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

    let (connector, spec) = parse_connector(rest)?;
    // 参照は (左|下)に「…」の(ルビ|注記|傍記) を前方参照より先に注記へ回す。
    if is_rest_note_spec(spec) {
        return None;
    }

    // 句点コード指定は他の記法より先に見る（参照実装 exec_style と同じ順序）
    if let Some(jis_code) = super::utils::parse_kuten_gaiji(spec) {
        return Some(CommandResult::KutenGaiji {
            target: target.to_string(),
            jis_code,
            annotation: extract_kuten_annotation(spec),
        });
    }

    // 以降の順序は参照実装 exec_style に合わせる。
    // 縦中横などのインライン要素は見出し・装飾より先に見る。
    if let Some(result) = try_parse_inline_element(target, spec) {
        return Some(result);
    }

    // 見出しかどうか。参照実装 exec_frontref_command は接続詞に関係なく
    // MIDASHI_COMMAND（見出し）に一致すれば前方参照の見出しにするので、
    // 「小沼農場」に大見出し（接続詞 に）も見出しにする。従来は connector=="は"
    // に限っていたため に/の の見出しを取りこぼし、後段で空の見出しブロックに
    // なっていた（対象が本文に残る）。
    {
        // 見出しも部分一致なので、入れ子コマンドの中身は見ない。
        let outer = without_nested_commands(spec);
        if let Some(level) = MidashiLevel::from_command(&outer) {
            let style = MidashiStyle::from_command(&outer);
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
    if let Some(result) = try_parse_annotation_ruby(target, spec) {
        return Some(result);
    }

    // 装飾タイプ
    if let Some(style_type) = StyleType::from_command(spec) {
        return Some(CommandResult::Style {
            target: target.to_string(),
            connector: connector.to_string(),
            style_type,
        });
    }

    None
}

/// 句点コード指定のうち、注記形 `「対象」に「＜注記＞」の注記` の ＜注記＞ を取り出す。
///
/// 置換形（`「5」はローマ数字、1-13-25` のように対象を外字画像へ差し替えるもの）は
/// 注記を持たないので `None`。＜注記＞ は外字表記を含みうる文字列で、ノード化は
/// 呼び出し側（パーサのノード構築層）が行う。
fn extract_kuten_annotation(spec: &str) -> Option<String> {
    spec.strip_suffix("の注記")
        .and_then(|s| s.strip_prefix('「'))
        .and_then(|s| s.strip_suffix('」'))
        .map(str::to_string)
}

/// 注記ルビパターンを解析（「対象」に「注記」の注記）
fn try_parse_annotation_ruby(target: &str, spec: &str) -> Option<CommandResult> {
    // 参照実装 PAT_CHUUKI = /「(.+?)」の注記/ は spec 全体に対する**部分一致**で、
    // 「の注記」の後ろに続きがあってもよい（例:「（ママ）」の注記、正しくは「十三」／
    // 「（ママ）」の注記がある）。従来は spec.ends_with("の注記") に限っていたため、
    // 末尾に続きがある注記ルビを取りこぼして注記化していた。
    // 参照は入れ子の ［＃…］ を**先に解決してタグにしてから** PAT_CHUUKI を当てるので、
    // 照合には入れ子を除いた文字列を使う。除いた結果 `」の注記` が消えるなら外側は
    // 注記のまま（例:「書卓が」は底本では「□□［＃「□□」に「二字欠落」の注記］が」）。
    // 逆に外側に `」の注記` が残るなら、入れ子は解決されてルビ文字の一部になる
    // （例:「シー・ラヴァーズ」に「16［＃「16」は縦中横］日に結成式」の注記）。
    let (flat, map) = strip_nested_commands(spec);
    if !flat.contains("」の注記") {
        return None;
    }

    // 「」で囲まれた注記内容を抽出する。参照実装の /「(.+?)」の注記/ と同じく、
    // 「」の注記」が続く位置まで伸ばす。注記の中に「」が入れ子になっていても切らない。
    // 位置は入れ子を除いた文字列で求め、切り出しは入れ子を含む元の文字列から行う。
    let flat_start = flat.find('「')? + '「'.len_utf8();
    let flat_end = flat[flat_start..]
        .match_indices("」の注記")
        .map(|(i, _)| flat_start + i)
        .next()?;
    if flat_end <= flat_start {
        return None;
    }

    let annotation = &spec[map[flat_start]..map[flat_end]];
    if annotation.is_empty() {
        return None;
    }
    Some(CommandResult::AnnotationRuby {
        target: target.to_string(),
        annotation: annotation.to_string(),
    })
}

/// 接続詞を解析し、(接続詞, 仕様部分) を返す。
///
/// 参照 `PAT_FRONTREF = 「…」[にはの](「.+」の)*(.+)` の接続詞は **1 文字**で、
/// `左に` は接続詞ではなく仕様の一部として `exec_style` に渡る。つまり
/// `「あ」の左に縦中横` は縦中横が効き（方向は無視）、`「あ」の左に大見出し` は
/// 見出しが効く（いずれも実測）。方向が意味を持つのは装飾だけで、
/// それは `StyleType::from_command` が `左に傍点` を正準名として持つことで扱う。
fn parse_connector(rest: &str) -> Option<(&'static str, &str)> {
    for connector in ["に", "は", "の"] {
        if let Some(spec) = rest.strip_prefix(connector) {
            return Some((connector, spec));
        }
    }
    None
}

/// 参照 `PAT_REST_NOTES = /(左|下)に「(.*)」の(ルビ|注記|傍記)/` に当たる指定か。
///
/// 参照の dispatch はこれを前方参照より**先**に見て注記へ回すので、
/// 前方参照としては解決しない（`「あ」の左に「い」の注記` は注記）。
fn is_rest_note_spec(spec: &str) -> bool {
    let Some(rest) = spec
        .strip_prefix("左に")
        .or_else(|| spec.strip_prefix("下に"))
    else {
        return false;
    };
    rest.starts_with('「')
        && ["」のルビ", "」の注記", "」の傍記"]
            .iter()
            .any(|p| rest.contains(p))
}

/// 入れ子コマンド `［…］` の範囲を取り除いた指定文字列。
///
/// 参照は入れ子の `［＃…］` を**先に解決**してタグにしてから外側の記法語を見るので、
/// 外側からは内側の記法語が見えない。こちらは平坦な文字列照合なので、
/// `contains` で見る前に入れ子を落とす。落とさないと
/// `［＃「ｍ」の左に「52-［＃「52-」は縦中横］歳」］` の内側の「縦中横」を拾って、
/// 外側を縦中横として解決してしまう（実文書 000311/15995 で発生）。
pub(super) fn without_nested_commands(spec: &str) -> std::borrow::Cow<'_, str> {
    if !spec.contains('［') {
        return std::borrow::Cow::Borrowed(spec);
    }
    std::borrow::Cow::Owned(strip_nested_commands(spec).0)
}

/// 入れ子コマンドを除いた文字列と、その各バイト位置に対応する元文字列のバイト位置。
///
/// 記法語の**照合**は入れ子を除いた文字列で行う一方、そこから切り出す内容
/// （注記ルビのルビ文字など）は入れ子コマンドを含む**元の文字列**で欲しい。
/// 対応表は除去後の各バイトに元のバイト位置を持ち、末尾に番兵として元の長さを置く。
fn strip_nested_commands(spec: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(spec.len());
    let mut map = Vec::with_capacity(spec.len() + 1);
    let mut pos = 0;
    while pos < spec.len() {
        let ch = spec[pos..].chars().next().expect("pos は char 境界");
        if ch == '［' {
            pos = skip_bracketed(spec, pos);
            continue;
        }
        for _ in 0..ch.len_utf8() {
            map.push(pos);
        }
        out.push(ch);
        pos += ch.len_utf8();
    }
    map.push(spec.len());
    (out, map)
}

/// インライン要素（縦中横、罫囲み、横組み、キャプション）を解析
fn try_parse_inline_element(target: &str, spec: &str) -> Option<CommandResult> {
    // 参照実装 exec_style は command.match? で部分一致を見る。
    // 「縦中横、行右小書き」のように後ろに指定が続くものも拾う。
    // 入れ子コマンドの中身は参照からは見えないので落とす。
    let spec = &*without_nested_commands(spec);
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
            try_parse_annotation_ruby("書卓が", "底本では「□□［＃「□□」に「二字欠落」の注記］が」"),
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
    fn test_reference_requires_leading_bracket() {
        // 参照 PAT_REF = /^「/: 先頭が「でなければ前方参照にしない（注記化）。
        // 先頭が「なら従来どおり解決する。
        assert!(try_parse_reference("「？」は太字").is_some());
        // 前置きがあると None（→ 呼び出し側で注記になる）。
        assert_eq!(try_parse_reference("二つ目、三つ目の「？」は太字"), None);
        assert_eq!(try_parse_reference("２文字目の「i」は下付き小文字"), None);
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

#[cfg(test)]
mod direction_tests {
    use super::*;

    /// `左に` は接続詞ではなく指定の一部。参照 PAT_FRONTREF の接続詞は
    /// `[にはの]` の 1 文字で、`左に…` は exec_style へそのまま渡る。
    /// つまり方向が意味を持つのは装飾だけで、縦中横・見出し・罫囲みなどは
    /// 方向を無視して適用される。期待値はすべて参照実装で実測した。
    #[test]
    fn left_direction_applies_to_more_than_decorations() {
        assert!(matches!(
            try_parse_reference("「あ」の左に縦中横"),
            Some(CommandResult::InlineTcy { .. })
        ));
        assert!(matches!(
            try_parse_reference("「あ」の左に大見出し"),
            Some(CommandResult::Midashi { .. })
        ));
        assert!(matches!(
            try_parse_reference("「あ」の左に罫囲み"),
            Some(CommandResult::InlineKeigakomi { .. })
        ));
        // 装飾は左向きの変種になる。`下に` も参照は同じ方向として扱う。
        for content in ["「あ」の左に傍点", "「あ」の下に傍点"] {
            let Some(CommandResult::Style { style_type, .. }) = try_parse_reference(content) else {
                panic!("{content} が装飾にならない");
            };
            assert_eq!(style_type, StyleType::SesameDotAfter, "{content}");
        }
    }

    /// `(左|下)に「…」の(ルビ|注記|傍記)` は参照が前方参照より先に注記へ回すので、
    /// ここでは解決しない（None を返して注記にする）。
    #[test]
    fn left_annotation_forms_are_left_to_the_note_path() {
        for content in [
            "「あ」の左に「い」の注記",
            "「あ」の左に「い」のルビ",
            "「あ」の下に「い」の傍記",
        ] {
            assert_eq!(try_parse_reference(content), None, "{content}");
        }
    }

    /// 入れ子コマンドの中身は参照からは見えない（先に解決されてタグになる）。
    /// `contains` で拾うと外側を誤って解決する（実文書 000311/15995 で発生）。
    #[test]
    fn nested_commands_are_invisible_to_the_outer_spec() {
        assert_eq!(
            try_parse_reference("「ｍ」の左に「52-［＃「52-」は縦中横］歳」"),
            None,
            "内側の縦中横を拾ってはいけない"
        );
    }
}
