//! コマンド文字列の解析
//!
//! `［＃...］` 形式のコマンド内容を解析し、適切なノードまたはコマンド情報を返します。

use crate::node::{BlockParams, BlockType, FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::block_parser::{
    parse_block_end, parse_block_start, parse_inline_end, try_parse_burasage,
    try_parse_font_size_start, try_parse_line_chitsuki, try_parse_line_indent,
    try_parse_midashi_start,
};
use super::content_parser::{
    contains_fig_png, dakuten_katakana_num, is_kaeriten, try_parse_image, try_parse_okurigana,
};
use super::reference_parser::{try_parse_left_ruby, try_parse_reference, without_nested_commands};

/// コマンド解析結果
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// 装飾コマンド（後方参照）
    Style {
        target: String,
        connector: String,
        style_type: StyleType,
    },

    /// 見出しコマンド（後方参照）
    Midashi {
        target: String,
        level: MidashiLevel,
        style: MidashiStyle,
    },

    /// フォントサイズコマンド（後方参照）
    FontSize {
        target: String,
        size_type: FontSizeType,
        level: u32,
    },

    /// ブロック開始
    BlockStart {
        block_type: BlockType,
        params: BlockParams,
    },

    /// ブロック終了
    BlockEnd {
        block_type: BlockType,
        /// ［＃ここで…終わり］形式で閉じたか（行末 <br /> を抑制する）
        explicit: bool,
    },

    /// 行単位字下げ
    LineIndent { width: Option<u32> },

    /// 行単位地付き/地から
    LineChitsuki { width: u32 },

    /// 注記
    Note(String),

    /// 画像
    Image {
        filename: String,
        alt: String,
        /// 写真か（false なら挿絵）。参照実装 exec_img_command は説明に「写真」が
        /// 入っていれば写真扱いにする。CSSクラス名の選択はレンダラに委ねる。
        is_photo: bool,
        width: Option<u32>,
        height: Option<u32>,
    },

    /// 返り点
    Kaeriten(String),

    /// 濁点付き片仮名（面区点 `1-7-8N`）。直前の `ワ゛`〜`ヲ゛` を対象にする前方参照。
    DakutenKatakana {
        /// 面区点 `1-7-8N` の末尾番号 N（`2`〜`5`）
        num: String,
    },

    /// 訓点送り仮名
    Okurigana(String),

    /// 縦中横開始
    TcyStart,

    /// 縦中横終了
    TcyEnd,

    /// 割り注開始
    WarichuStart,

    /// 割り注終了
    WarichuEnd,

    /// 装飾開始
    StyleStart { style_type: StyleType },

    /// 装飾終了
    StyleEnd { style_type: StyleType },

    /// 左ルビ指定
    LeftRuby { target: String, ruby: String },

    /// 注記ルビ（「対象」に「注記」の注記）
    AnnotationRuby { target: String, annotation: String },

    /// 縦中横（後方参照）
    InlineTcy { target: String },

    /// 罫囲み（後方参照）
    InlineKeigakomi { target: String },

    /// 横組み（後方参照）
    InlineYokogumi { target: String },

    /// キャプション（後方参照）
    InlineCaption { target: String },

    /// 返り点（後方参照「対象」は返り点）
    InlineKaeriten { target: String },

    /// 訓点送り仮名（後方参照「対象」は訓点送り仮名）
    InlineOkurigana { target: String },

    /// キャプション開始
    CaptionStart,

    /// キャプション終了
    CaptionEnd,

    /// 注記付き範囲開始
    AnnotationRangeStart,

    /// 左に注記付き範囲開始
    LeftAnnotationRangeStart,

    /// 注記付き範囲終了
    AnnotationRangeEnd { annotation: String },

    /// 左に注記付き範囲終了
    LeftAnnotationRangeEnd { annotation: String },

    /// 傍記（工場に「×」の傍記）
    SideNote { target: String, annotation: String },

    /// 句点コード指定による外字画像（「対象」は…、面-区-点）。
    KutenGaiji {
        target: String,
        /// 面-区-点（`1-13-25` 形式）
        jis_code: String,
        /// 注記形 `「対象」に「＜注記＞」の注記` の ＜注記＞ 部分。対象を外字画像へ
        /// 置き換える置換形（`「5」はローマ数字、1-13-25`）では `None`。
        annotation: Option<String>,
    },

    /// 未知のコマンド
    Unknown(String),
}

/// コマンド文字列を解析する。
///
/// **分岐の順序が仕様そのもの**で、参照実装 `dispatch_aozora_command` の
/// 判定順を写している。前の分岐に当たった時点で決まるので、並べ替えると
/// 別の記法として解釈される（例: 折り返し字下げは「字下げ」より先、
/// 画像は前方参照より先）。番号は上から順の通し番号で、意味は持たない。
pub fn parse_command(content: &str) -> CommandResult {
    // 照合は**原文のまま**行う。参照実装 dispatch_aozora_command は命令文字列を
    // そのまま照合するので、前後に空白があれば `PAT_REF`（`^「.+」`）も
    // `command == '傍点'` も外れて注記になる（`［＃ 「あいう」に傍点 ］` は注記。実測）。
    // 注記化するときに前後空白（全角空白 U+3000 含む）が保たれるのは、
    // apply_rest_notes が命令文字列をそのまま EditorNote にするためで、
    // ここで trim しなければ自然にそうなる。

    // 1. ぶら下げ（折り返して）。参照実装 dispatch_aozora_command は
    //    ORIKAESHI_COMMAND（折り返して）を他のどの分岐より先に判定して
    //    apply_burasage へ回す。`ここから` の有無に関係なくぶら下げになる
    //    （例:「［＃改行天付き、折り返して５字下げ］」）ので最初に見る。
    if content.contains("折り返して") {
        let mut params = BlockParams {
            is_block: true,
            ..Default::default()
        };
        if let Some(result) = try_parse_burasage(content, &mut params) {
            return result;
        }
    }

    // 2. 左ルビパターン（後方参照より先にチェック）
    if content.contains("の左に") && content.contains("のルビ") {
        if let Some(result) = try_parse_left_ruby(content) {
            return result;
        }
    }

    // 3. 割り注。参照 dispatch_aozora_command は `WARICHU_COMMAND`（割り注）を
    //    **部分一致**で、画像・前方参照・字下げより先に見る。そのため
    //    `「細字の部分」は割り注で処理` のような説明文でも割り注の開始になる
    //    （実文書 000933/47196）。`ここから`/`ここで` で始まるものは参照ではさらに
    //    手前でブロック開始・終了として処理されるので、ここでは除く。
    //    照合は入れ子コマンドを除いた文字列で行う。参照は入れ子の ［＃…］ を先に
    //    タグへ解決してから WARICHU_COMMAND を当てるので、内側にしか 割り注 が無い
    //    注記（`「（［＃割り注］…［＃割り注終わり］）」は底本では…`）は割り注に
    //    ならない（実文書 001395/51364）。
    {
        let outer = without_nested_commands(content);
        if outer.contains("割り注")
            && !content.starts_with("ここから")
            && !content.starts_with("ここで")
        {
            return if outer.contains("終わり") {
                CommandResult::WarichuEnd
            } else {
                CommandResult::WarichuStart
            };
        }
    }

    // 4. 画像。参照実装の dispatch_aozora_command は fig…png の判定を
    //    前方参照より先に置くので、ここでも先に見る。
    //
    // 画像ルートへ回す条件は `/fig\d+_\d+\.png/` を**含む**ことだけ。ファイル名が
    // この形でなければ（`photo.png`・`fig_photo_01.png`・`fig1_2.jpg` など）参照は
    // 画像にせず注記のまま出す。`）入る` で終わることを条件に足すと、この振り分けより
    // 広く画像化してしまう。逆に PAT_IMAGE 自体は末尾アンカーが無いので、
    // `…）入る。` のように後続文字があっても画像になる（それは try_parse_image 側）。
    if contains_fig_png(content) {
        if let Some(result) = try_parse_image(content) {
            return result;
        }
    }

    // 5. 後方参照パターン: 「対象」に/は/の 装飾
    if let Some(result) = try_parse_reference(content) {
        return result;
    }

    // 6. ブロック開始: ここから...
    if content.starts_with("ここから") {
        return parse_block_start(content);
    }

    // 7. ブロック終了: ここで...
    // 参照実装 dispatch は「ここで」で始まる命令を exec_block_end_command へ回し、
    // detect_command_mode がキーワード（字下げ等）だけを見て閉じる。終止語（終わり）
    // の綴りは不問で、「字下げ終り」（送り仮名欠き）や「字下げ終わり」」（余分な 」）
    // でも字下げ終了になる。よって「ここで」で始まれば parse_block_end に回し、
    // キーワードが無ければ parse_block_end 側で注記化する。
    if content.starts_with("ここで") {
        return parse_block_end(content);
    }

    // 8. 注記付き範囲パターン
    if let Some(result) = try_parse_annotation_range(content) {
        return result;
    }

    // 9. 傍記パターン（「対象」に「注記」の傍記）
    if let Some(result) = try_parse_side_note(content) {
        return result;
    }

    // 10. インライン終了: ...終わり
    if content.ends_with("終わり") {
        return parse_inline_end(content);
    }

    // 11. 行単位字下げ: N字下げ
    if let Some(result) = try_parse_line_indent(content) {
        return result;
    }

    // 12. 行単位地付き/地から
    if let Some(result) = try_parse_line_chitsuki(content) {
        return result;
    }

    // 13. 濁点付き片仮名（`ワ゛［＃1-7-82］`）。参照 dispatch_aozora_command は
    //    前方参照（PAT_REF）の後・返り点の前に置くので、ここでも同じ位置にする。
    if let Some(num) = dakuten_katakana_num(content) {
        return CommandResult::DakutenKatakana { num };
    }

    // 14. 返り点
    if is_kaeriten(content) {
        return CommandResult::Kaeriten(content.to_string());
    }

    // 15. 訓点送り仮名
    if let Some(okurigana) = try_parse_okurigana(content) {
        return CommandResult::Okurigana(okurigana);
    }

    // 16. 訓点送り仮名（説明付き）
    if content.starts_with("訓点送り仮名") {
        return CommandResult::Note(content.to_string());
    }

    // 17. 縦中横
    if content == "縦中横" {
        return CommandResult::TcyStart;
    }

    // 18. 罫囲み（インライン）
    if content == "罫囲み" {
        return CommandResult::BlockStart {
            block_type: BlockType::Keigakomi,
            params: BlockParams::default(),
        };
    }

    // 19. 横組み（インライン）
    if content == "横組み" {
        return CommandResult::BlockStart {
            block_type: BlockType::Yokogumi,
            params: BlockParams::default(),
        };
    }

    // 20. 割書（インライン）。参照 WARIGAKI_COMMAND='割書' → <span class="warigaki">。
    if content == "割書" {
        return CommandResult::BlockStart {
            block_type: BlockType::Warigaki,
            params: BlockParams::default(),
        };
    }

    // 21. 装飾開始
    if let Some(style_type) = StyleType::from_command(content) {
        return CommandResult::StyleStart { style_type };
    }

    // 22. キャプション開始
    if content == "キャプション" {
        return CommandResult::CaptionStart;
    }

    // 23. 見出し開始
    if let Some(result) = try_parse_midashi_start(content) {
        return result;
    }

    // 24. インラインフォントサイズ開始
    if let Some(result) = try_parse_font_size_start(content) {
        return result;
    }

    // その他は注記（原文のまま。前後空白を保つ）
    CommandResult::Note(content.to_string())
}

/// 注記付き範囲パターンを解析
fn try_parse_annotation_range(content: &str) -> Option<CommandResult> {
    // 開始パターン
    if content == "注記付き" {
        return Some(CommandResult::AnnotationRangeStart);
    }
    if content == "左に注記付き" {
        return Some(CommandResult::LeftAnnotationRangeStart);
    }

    // 終了パターン: 「（銘々）」の注記付き終わり
    if content.ends_with("の注記付き終わり") {
        let rest = content.trim_end_matches("の注記付き終わり");

        // 左パターン: 左に「...」の注記付き終わり
        if let Some(rest) = rest.strip_prefix("左に") {
            if let Some(annotation) = extract_bracket_content(rest) {
                return Some(CommandResult::LeftAnnotationRangeEnd {
                    annotation: annotation.to_string(),
                });
            }
        }

        // 通常パターン: 「...」の注記付き終わり
        if let Some(annotation) = extract_bracket_content(rest) {
            return Some(CommandResult::AnnotationRangeEnd {
                annotation: annotation.to_string(),
            });
        }
    }

    None
}

/// 傍記パターンを解析（`「対象」に「注記」の傍記`）。
///
/// 参照 `PAT_BOUKI = /「(.)」の傍記/` は**注記が 1 文字**のときだけ当たる
/// （`「あ」に「××」の傍記` や `「あ」に「」の傍記` は注記になる。実測）。
/// 対象は貪欲に取る——`「あ「い」う」に「×」の傍記` の対象は `あ「い」う`（実測）。
/// そのため末尾から順に剥がしていく。
fn try_parse_side_note(content: &str) -> Option<CommandResult> {
    let rest = content.strip_suffix("の傍記")?;
    // 注記は末尾の `「…」`。`」` は直前に無ければならない（後続文字は許さない）。
    let inner = rest.strip_suffix('」')?;
    let annotation_start = inner.rfind('「')?;
    let annotation = &inner[annotation_start + '「'.len_utf8()..];
    if annotation.chars().count() != 1 {
        return None;
    }
    // 対象は `「…」に` の中身。先頭の `「` から取るので `」` を含んでいてもよい。
    let before = inner[..annotation_start].strip_suffix('に')?;
    let target_body = before.strip_suffix('」')?;
    let target_start = target_body.find('「')?;
    Some(CommandResult::SideNote {
        target: target_body[target_start + '「'.len_utf8()..].to_string(),
        annotation: annotation.to_string(),
    })
}

/// `「...」` の中身を取り出す。閉じは**最後の** `」` を使う（貪欲）。
///
/// 参照の注記付き終わりは `「…」の注記付き終わり` を貪欲に取るので、
/// `「ちゅ」う」の注記付き終わり` の注記は `ちゅ」う` になる（実測）。
/// 最初の `」` で切ると `ちゅ` になってしまう。
fn extract_bracket_content(s: &str) -> Option<&str> {
    let start = s.find('「')?;
    let end = s.rfind('」')?;
    if end <= start {
        return None;
    }
    Some(&s[start + '「'.len_utf8()..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontref_kaeriten_and_okurigana() {
        // 「対象」は返り点 / 訓点送り仮名 は前方参照として解決する。
        assert_eq!(
            parse_command("「レ」は返り点"),
            CommandResult::InlineKaeriten {
                target: "レ".to_string()
            }
        );
        assert_eq!(
            parse_command("「爾」は訓点送り仮名"),
            CommandResult::InlineOkurigana {
                target: "爾".to_string()
            }
        );
    }

    /// 句点コード指定は、句点コードと注記の切り出しまでを解析側で確定させる。
    /// ノード構築層は受け取った値を写像するだけでよい。
    #[test]
    fn test_parse_kuten_gaiji_carries_parsed_values() {
        // 置換形: 対象を外字画像に置き換える（注記は無い）。
        assert_eq!(
            parse_command("「5」はローマ数字、1-13-25"),
            CommandResult::KutenGaiji {
                target: "5".to_string(),
                jis_code: "1-13-25".to_string(),
                annotation: None,
            }
        );
        // 注記形: ＜注記＞ を取り出す（中身のノード化は後段）。
        assert_eq!(
            parse_command("「すはどり」に「※［＃「尸＋鳥」、第4水準2-94-2］」の注記"),
            CommandResult::KutenGaiji {
                target: "すはどり".to_string(),
                jis_code: "2-94-02".to_string(),
                annotation: Some("※［＃「尸＋鳥」、第4水準2-94-2］".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_style_bouten() {
        let result = parse_command("「である」に傍点");
        assert_eq!(
            result,
            CommandResult::Style {
                target: "である".to_string(),
                connector: "に".to_string(),
                style_type: StyleType::SesameDot,
            }
        );
    }

    #[test]
    fn test_parse_style_bold() {
        let result = parse_command("「重要」に太字");
        assert_eq!(
            result,
            CommandResult::Style {
                target: "重要".to_string(),
                connector: "に".to_string(),
                style_type: StyleType::Bold,
            }
        );
    }

    #[test]
    fn test_parse_midashi() {
        let result = parse_command("「第一章」は大見出し");
        assert_eq!(
            result,
            CommandResult::Midashi {
                target: "第一章".to_string(),
                level: MidashiLevel::O,
                style: MidashiStyle::Normal,
            }
        );
    }

    #[test]
    fn test_parse_midashi_any_connector() {
        // 参照実装は接続詞に関係なく前方参照の見出しにする。「に」大見出しも見出し。
        assert_eq!(
            parse_command("「小沼農場」に大見出し"),
            CommandResult::Midashi {
                target: "小沼農場".to_string(),
                level: MidashiLevel::O,
                style: MidashiStyle::Normal,
            }
        );
        assert_eq!(
            parse_command("「章題」の中見出し"),
            CommandResult::Midashi {
                target: "章題".to_string(),
                level: MidashiLevel::Naka,
                style: MidashiStyle::Normal,
            }
        );
    }

    #[test]
    fn test_parse_midashi_dogyo() {
        let result = parse_command("「一」は同行中見出し");
        assert_eq!(
            result,
            CommandResult::Midashi {
                target: "一".to_string(),
                level: MidashiLevel::Naka,
                style: MidashiStyle::Dogyo,
            }
        );
    }

    #[test]
    fn test_parse_bare_burasage_without_kokokara() {
        // 「ここから」の無い「［＃改行天付き、折り返して５字下げ］」も、参照実装が
        // ORIKAESHI を最優先で見るのでぶら下げになる（jisage ブロックにしない）。
        let result = parse_command("改行天付き、折り返して５字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Burasage,
                params: BlockParams {
                    width: Some(0),
                    wrap_width: Some(5),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
        // 「ここから」ありの従来形も引き続きぶら下げになる。
        let result = parse_command("ここから改行天付き、折り返して５字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Burasage,
                params: BlockParams {
                    width: Some(0),
                    wrap_width: Some(5),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn test_parse_block_start_jisage() {
        let result = parse_command("ここから2字下げ");
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
        let result = parse_command("ここで字下げ終わり");
        assert_eq!(
            result,
            CommandResult::BlockEnd {
                block_type: BlockType::Jisage,
                explicit: true,
            }
        );
    }

    #[test]
    fn test_parse_block_end_lenient_suffix() {
        // 参照 detect_command_mode はキーワードだけで閉じるので、終止語の綴りは不問。
        // 送り仮名欠き「終り」や余分な 」 でも字下げ終了になる。
        for cmd in ["ここで字下げ終り", "ここで字下げ終わり」", "ここで字下げ"]
        {
            assert_eq!(
                parse_command(cmd),
                CommandResult::BlockEnd {
                    block_type: BlockType::Jisage,
                    explicit: true,
                },
                "{cmd} が字下げ終了にならない"
            );
        }
        // キーワードの無い「ここで…」は注記のまま。
        assert!(matches!(
            parse_command("ここで何か"),
            CommandResult::Note(_)
        ));
    }

    #[test]
    fn test_parse_line_indent() {
        let result = parse_command("3字下げ");
        assert_eq!(result, CommandResult::LineIndent { width: Some(3) });
    }

    #[test]
    fn test_parse_tcy() {
        assert_eq!(parse_command("縦中横"), CommandResult::TcyStart);
        assert_eq!(parse_command("縦中横終わり"), CommandResult::TcyEnd);
    }

    #[test]
    fn test_parse_warichu() {
        assert_eq!(parse_command("割り注"), CommandResult::WarichuStart);
        assert_eq!(parse_command("割り注終わり"), CommandResult::WarichuEnd);
    }

    #[test]
    fn test_parse_warigaki() {
        // 参照 WARIGAKI_COMMAND='割書' はインラインスタイル（<span class="warigaki">）。
        // 割り注(warichu)とは別コマンド。
        assert_eq!(
            parse_command("割書"),
            CommandResult::BlockStart {
                block_type: BlockType::Warigaki,
                params: BlockParams::default(),
            }
        );
        assert_eq!(
            parse_command("割書終わり"),
            CommandResult::BlockEnd {
                block_type: BlockType::Warigaki,
                explicit: false,
            }
        );
    }

    #[test]
    fn test_parse_unknown() {
        let result = parse_command("改ページ");
        assert_eq!(result, CommandResult::Note("改ページ".to_string()));
    }

    #[test]
    fn test_parse_block_start_jisage_fullwidth() {
        let result = parse_command("ここから２字下げ");
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
    fn test_parse_line_indent_fullwidth() {
        let result = parse_command("３字下げ");
        assert_eq!(result, CommandResult::LineIndent { width: Some(3) });
    }

    #[test]
    fn test_parse_line_chitsuki() {
        let result = parse_command("地付き");
        assert_eq!(result, CommandResult::LineChitsuki { width: 0 });

        let result = parse_command("地から１字上げ");
        assert_eq!(result, CommandResult::LineChitsuki { width: 1 });

        let result = parse_command("地から3字上げ");
        assert_eq!(result, CommandResult::LineChitsuki { width: 3 });
    }

    /// 照合は原文のまま行う。参照 dispatch_aozora_command は命令文字列を
    /// そのまま照合するので、前後に空白があればどの記法にも当たらず注記になる
    /// （半角・全角とも実測）。trim して照合していた頃は傍点として解釈していた。
    #[test]
    fn surrounding_spaces_make_the_command_a_note() {
        for content in [
            " 「あいう」に傍点 ",
            "\u{3000}「あいう」に傍点\u{3000}",
            " 傍点",
            "傍点 ",
        ] {
            assert_eq!(
                parse_command(content),
                CommandResult::Note(content.to_string()),
                "{content:?} は注記になる"
            );
        }
        // 空白が無ければ従来どおり記法として解釈する。
        assert!(matches!(
            parse_command("傍点"),
            CommandResult::StyleStart { .. }
        ));
    }

    /// 傍記は参照 `PAT_BOUKI = /「(.)」の傍記/` に合わせ、**注記が 1 文字**の
    /// ときだけ成立する。対象は貪欲に取る（`「あ「い」う」…` の対象は `あ「い」う`）。
    /// 期待値はすべて参照実装で実測した。
    #[test]
    fn side_note_takes_one_char_annotation_and_a_greedy_target() {
        assert_eq!(
            parse_command("「工場」に「×」の傍記"),
            CommandResult::SideNote {
                target: "工場".to_string(),
                annotation: "×".to_string()
            }
        );
        // 対象に 」 が入っていても先頭の 「 から取る。
        assert_eq!(
            parse_command("「あ「い」う」に「×」の傍記"),
            CommandResult::SideNote {
                target: "あ「い」う".to_string(),
                annotation: "×".to_string()
            }
        );
        // 注記が 1 文字でなければ傍記にならない（注記になる）。
        for content in ["「あ」に「××」の傍記", "「あ」に「」の傍記"] {
            assert_eq!(
                parse_command(content),
                CommandResult::Note(content.to_string()),
                "{content:?} は傍記にしない"
            );
        }
    }
}
