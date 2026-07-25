//! HTML変換モジュール
//!
//! 青空文庫形式のテキストをHTMLに変換します。

mod block_manager;
mod block_renderer;
mod document_renderer;
mod node_renderer;
mod options;
mod presentation;
mod renderer;
mod tag_generator;

pub use options::{Quirks, RenderOptions};
pub use presentation::html_escape;
pub use renderer::HtmlRenderer;

/// 青空文庫形式のテキストをHTMLに変換
///
/// # Arguments
///
/// * `input` - 青空文庫形式のテキスト
/// * `options` - レンダリングオプション
///
/// # Returns
///
/// HTML文字列
///
/// # Examples
///
/// ```
/// use aozora2::html::{convert, RenderOptions};
///
/// // 青空文庫形式: ヘッダー、空行、本文。行の区切りは CRLF
/// let input = "タイトル\r\n\r\n吾輩《わがはい》は猫である";
/// let html = convert(input, &RenderOptions::default());
/// assert!(html.contains("<ruby>"));
/// ```
pub fn convert(input: &str, options: &RenderOptions) -> String {
    let mut renderer = HtmlRenderer::new(options.clone());
    renderer.render(input)
}

/// 1行をHTMLに変換
pub fn convert_line(line: &str, options: &RenderOptions) -> String {
    let mut renderer = HtmlRenderer::new(options.clone());
    renderer.render_line(line)
}

/// 旧経路（BlockManager）と新経路（中立AST）の**本文（main_text 内側）**を返す。
/// 中立ASTバックエンドの移行（docs/plan-neutral-ast.md）の一致率計測用。
/// 戻り値 `(old_body, new_body)`。両者が等しければ新経路がその作品の本文を
/// byte 再現できている。
pub fn compare_body(input: &str, options: &RenderOptions) -> (String, String) {
    use aozora_core::document::extract_body_lines;
    use aozora_core::lower::lower_to_blocks;
    use aozora_core::parser::parse_document_raw;

    // 旧: convert 出力から main_text の内側を切り出す。
    let full = convert(input, options);
    let open = "<div class=\"main_text\">";
    let old_body = match full.find(open) {
        Some(s) => {
            let start = s + open.len();
            let rest = &full[start..];
            let end = rest
                .find("</div>\r\n<div class=\"bibliographical_information\">")
                .or_else(|| rest.find("</div>\r\n<div id=\"card\""))
                .unwrap_or(rest.len());
            rest[..end].to_string()
        }
        None => String::new(),
    };

    // 新: body 行 → RawDoc → Vec<Block> → 本文HTML。
    let mut lines: Vec<&str> = input.split("\r\n").collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let body_lines = extract_body_lines(&lines);
    let raw = parse_document_raw(&body_lines);
    let blocks = lower_to_blocks(&raw);
    let new_body = block_renderer::render_body_blocks(&blocks);

    (old_body, new_body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// quirk accent_name_typos: 参照実装の表は A^ の説明で字母 A を落としている。
    /// 既定（オン）では再現し、オフでは「…付きA」に訂正する。
    #[test]
    fn test_accent_name_typo_quirk() {
        let src = "タイトル\r\n\r\n〔A^〕";

        let on = convert(src, &RenderOptions::default());
        assert!(
            on.contains("※(サーカムフレックスアクセント付き)"),
            "実際: {on}"
        );

        let off = convert(src, &RenderOptions::new().with_quirks(Quirks::none()));
        assert!(
            off.contains("※(サーカムフレックスアクセント付きA)"),
            "実際: {off}"
        );
    }

    /// quirk raw_header_metadata: 参照実装はタイトル・著者を生のまま出し
    /// `&<>"` をエスケープしない。既定（オン）では再現し、オフでは
    /// エスケープする。
    #[test]
    fn test_raw_header_metadata_quirk() {
        let src = "THE \"DOOM\" SERIES\r\n著者<A&B>\r\n\r\n本文\r\n";

        let on = convert(src, &RenderOptions::default());
        assert!(
            on.contains("<h1 class=\"title\">THE \"DOOM\" SERIES</h1>"),
            "実際: {on}"
        );
        assert!(
            on.contains("<h2 class=\"author\">著者<A&B></h2>"),
            "実際: {on}"
        );
        assert!(
            on.contains("<meta name=\"DC.Title\" content=\"THE \"DOOM\" SERIES\" />"),
            "実際: {on}"
        );

        let off = convert(src, &RenderOptions::new().with_quirks(Quirks::none()));
        assert!(
            off.contains("<h1 class=\"title\">THE &quot;DOOM&quot; SERIES</h1>"),
            "実際: {off}"
        );
        assert!(
            off.contains("<h2 class=\"author\">著者&lt;A&amp;B&gt;</h2>"),
            "実際: {off}"
        );
    }

    /// quirk raw_image_alt: 参照実装 Tag::Img は alt を無エスケープで出す。
    /// キャプションに `"` や `&` を含むと属性値が壊れる不正な HTML になる。
    /// 既定（オン）では再現し、オフでは HTML エスケープする。
    #[test]
    fn test_raw_image_alt_quirk() {
        let src =
            "タイトル\r\n\r\n［＃「from \"X\" & Y」のキャプション付きの図（fig001_01.png、横1×縦1）入る］";

        let on = convert(src, &RenderOptions::default());
        assert!(
            on.contains("alt=\"「from \"X\" & Y」のキャプション付きの図\""),
            "実際: {on}"
        );

        let off = convert(src, &RenderOptions::new().with_quirks(Quirks::none()));
        assert!(
            off.contains("alt=\"「from &quot;X&quot; &amp; Y」のキャプション付きの図\""),
            "実際: {off}"
        );
    }

    /// quirk empty_image_dimensions: 幅・高さのない画像で参照実装は
    /// width="" height="" と空の属性を出す。既定（オン）では再現し、
    /// オフでは幅・高さの属性を出さない。
    #[test]
    fn test_empty_image_dimensions_quirk() {
        let src = "タイトル\r\n\r\n［＃太陽マジックのうたの楽譜（fig45472_01.png）入る］";

        // 既定（quirk オン）: 空の width="" height="" を出す
        let on = convert(src, &RenderOptions::default());
        assert!(on.contains("width=\"\" height=\"\""), "実際: {on}");

        // quirk オフ: 幅・高さの属性を出さない
        let off = convert(src, &RenderOptions::new().with_quirks(Quirks::none()));
        assert!(!off.contains("width="), "実際: {off}");
        assert!(
            off.contains("<img class=\"illustration\" src="),
            "実際: {off}"
        );
    }

    /// quirk nested_gaiji_in_alt: 参照実装は alt 内の入れ子外字を img に展開する。
    /// 既定（オン）では再現し、オフでは素のテキストのまま残す。
    #[test]
    fn test_nested_gaiji_in_alt_quirk() {
        let src = "タイトル\r\n\r\n※［＃「姉」の正字、「※［＃第3水準1-85-57］」の「木」に代えて「女」、374-10］";

        // 既定（quirk オン）: alt の中に入れ子の <img> が展開される
        let on = convert(src, &RenderOptions::default());
        assert_eq!(on.matches("<img").count(), 2, "実際: {on}");

        // quirk オフ: 入れ子の記法は素のテキストのまま alt に残る
        let off = convert(src, &RenderOptions::new().with_quirks(Quirks::none()));
        assert_eq!(off.matches("<img").count(), 1, "実際: {off}");
        assert!(off.contains("※［＃第3水準1-85-57］"), "実際: {off}");
    }

    #[test]
    fn test_convert_simple() {
        // 青空文庫形式: ヘッダー、空行、本文の構造
        let input = "タイトル\r\n\r\nこんにちは";
        let html = convert(input, &RenderOptions::default());
        assert!(html.contains("こんにちは"));
        assert!(html.contains("<!DOCTYPE"));
    }

    #[test]
    fn test_convert_ruby() {
        // 青空文庫形式: ヘッダー、空行、本文の構造
        let input = "タイトル\r\n\r\n漢字《かんじ》";
        let html = convert(input, &RenderOptions::default());
        assert!(html.contains("<ruby>"));
        assert!(html.contains("漢字"));
        assert!(html.contains("かんじ"));
    }

    #[test]
    fn test_convert_line() {
        let html = convert_line("猫《ねこ》", &RenderOptions::default());
        assert!(html.contains("<ruby>"));
    }

    /// ［＃注記付き］範囲ルビの親文字に変換不能外字があると、その外字注記は
    /// rb の外ではなく rb 内に残る（参照実装は親文字を通常描画してから rb に包む）。
    #[test]
    fn test_annotation_ruby_keeps_gaiji_notes_in_base() {
        let src = "タイトル\r\n\r\n生物の［＃注記付き］※［＃「てへん＋執」、U+22D07、254-8］［＃「マヽ」の注記付き終わり］";
        let html = convert(src, &RenderOptions::default());
        assert!(
            html.contains(
                "<ruby><rb>※<span class=\"notes\">［＃「てへん＋執」、U+22D07、254-8］</span></rb><rp>（</rp><rt>マヽ</rt><rp>）</rp></ruby>"
            ),
            "実際: {html}"
        );
    }

    /// 割り注はペア（同一行）で `<span class="warichu">（…）</span>` になる。
    #[test]
    fn test_warichu_pair() {
        let html = convert_line("前［＃割り注］中身［＃割り注終わり］後", &RenderOptions::default());
        assert!(
            html.contains("前<span class=\"warichu\">（中身）</span>後"),
            "実際: {html}"
        );
    }

    /// 参照実装 apply_warichu の END は状態を持たず、開いている割り注が無くても
    /// 無条件に `）</span>` を出す（『ここから割り注』が注記化されて span が開いて
    /// いない状態での『割り注終わり』）。孤立した割り注終わりでも `）</span>` を出す。
    #[test]
    fn test_warichu_orphan_end() {
        let html = convert_line("前［＃ここから割り注］中身［＃割り注終わり］後", &RenderOptions::default());
        assert!(
            html.contains("中身）</span>後"),
            "孤立した割り注終わりが `）</span>` を出していない: {html}"
        );
    }

    /// 中身が空のルビ 《》 は、ルビ要素にせず 《》 のテキストとして出す
    #[test]
    fn test_empty_ruby_is_literal_text() {
        let html = convert("タイトル\r\n\r\n記号「《》」です", &RenderOptions::default());
        assert!(html.contains("記号「《》」です"), "実際: {html}");
        assert!(
            !html.contains("<ruby><rb></rb>"),
            "空のルビ要素を作らないこと。実際: {html}"
        );
    }

    /// 送り仮名を欠く見出し終了 ［＃中見出終わり］ も見出しを閉じる
    #[test]
    fn test_midashi_end_without_okurigana_closes_the_heading() {
        let html = convert(
            "タイトル\r\n\r\n［＃中見出し］章題［＃中見出終わり］\r\n本文",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("<h4 class=\"naka-midashi\"><a class=\"midashi_anchor\" id=\"midashi10\">章題</a></h4>"),
            "見出しが正しく閉じること。実際: {html}"
        );
    }

    /// 注記ルビの注記部分は入れ子の「」で切れない
    /// （「(.+?)」の注記 の非貪欲マッチが「」の注記」まで伸びる）
    #[test]
    fn test_annotation_ruby_keeps_nested_brackets() {
        let html = convert(
            "タイトル\r\n\r\n語［＃「語」に「「意味」の注」の注記］",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("<rt>「意味」の注</rt>"),
            "注記が入れ子の「」で切れないこと。実際: {html}"
        );
    }

    /// 画像の説明に「写真」を含めば photo クラス、含まなければ illustration クラス
    #[test]
    fn test_image_class_is_photo_only_when_alt_mentions_photo() {
        let html = convert(
            "タイトル\r\n\r\n［＃人物写真（fig01_01.png）入る］\r\n［＃地図（fig01_02.png）入る］",
            &RenderOptions::default(),
        );
        assert!(html.contains("<img class=\"photo\""), "実際: {html}");
        assert!(html.contains("<img class=\"illustration\""), "実際: {html}");
    }

    /// くの字点は文字をそのまま出力し、「表記について」に注記を足す
    #[test]
    fn test_kunoji_note() {
        let html = convert(
            "タイトル\r\n\r\n本文でわざ／＼と使う",
            &RenderOptions::default(),
        );
        assert!(html.contains("<li>「くの字点」は「／＼」で表しました。</li>"));
        assert!(!html.contains("濁点付きくの字点"));
    }

    #[test]
    fn test_dakuten_kunoji_note() {
        let html = convert(
            "タイトル\r\n\r\n本文でしみ／″＼と使う",
            &RenderOptions::default(),
        );
        assert!(html.contains("<li>「濁点付きくの字点」は「／″＼」で表しました。</li>"));
    }

    /// 両方使った場合は 1 行にまとめる
    #[test]
    fn test_both_kunoji_notes_are_combined() {
        let html = convert(
            "タイトル\r\n\r\nわざ／＼としみ／″＼と使う",
            &RenderOptions::default(),
        );
        assert!(html.contains(
            "<li>「くの字点」は「／＼」で、「濁点付きくの字点」は「／″＼」で表しました。</li>"
        ));
    }

    /// くの字点を使っていなければ注記は出さない
    #[test]
    fn test_no_kunoji_note_without_kunoji() {
        let html = convert("タイトル\r\n\r\n／だけ、＼だけ", &RenderOptions::default());
        assert!(!html.contains("くの字点"));
    }

    /// くの字点は ［＃…］ の注記の中に書かれていても数える
    #[test]
    fn test_kunoji_inside_an_editor_note_is_counted() {
        let html = convert(
            "タイトル\r\n\r\nだぶ／＼して［＃「だぶ／＼して」は底本では「だぶ／″＼」］",
            &RenderOptions::default(),
        );
        assert!(html.contains(
            "<li>「くの字点」は「／＼」で、「濁点付きくの字点」は「／″＼」で表しました。</li>"
        ));
    }

    /// 画像化できない外字は「表記について」に外字一覧を出すが、
    /// 「［＃…］は、入力者による注」の項目は出さない
    /// （参照実装の escape_gaiji は :chuki フラグを立てない）
    #[test]
    fn test_unconvertible_gaiji_does_not_imply_an_editor_note() {
        let html = convert(
            "タイトル\r\n\r\n陥※［＃「こざとへん＋井」、U+9631、133-8］",
            &RenderOptions::default(),
        );
        assert!(!html.contains("入力者による注"), "実際: {html}");
        assert!(html.contains("JIS X 0213にない"), "外字一覧の項目は出る");
    }

    /// ［＃ここで…終わり］で閉じた行は行末の <br /> を出さない
    /// （参照実装 exec_block_end_command の @terprip=false 相当）。
    /// bare ［＃…終わり］は抑制しない。
    #[test]
    fn test_kokode_close_suppresses_break_but_bare_does_not() {
        // ここで字下げ終わり + 全角空白 → <br /> なし
        let a = convert(
            "タイトル\r\n\r\n［＃ここから１字下げ］\r\nA\r\n［＃ここで字下げ終わり］　\r\n次",
            &RenderOptions::default(),
        );
        assert!(a.contains("</div>　\r\n次"), "実際: {a}");
    }

    /// 同じ行で開いて閉じたブロック級構文（横組みなど）の行は行末の <br /> を出さない
    /// （参照実装 terpri? が buffer 中の Multiline タグを見て抑制する）
    #[test]
    fn test_same_line_block_suppresses_the_trailing_break() {
        let html = convert(
            "タイトル\r\n\r\n予は［＃ここから横組み］ABC［＃ここで横組み終わり］である。\r\n次の行",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("</div>である。\r\n次の行"),
            "横組みを含む行に <br /> が付かないこと。実際: {html}"
        );
    }

    /// 行末の ［＃N字下げ］ は行全体を字下げの div で包む
    /// （参照実装 apply_jisage の unshift 相当）
    #[test]
    fn test_line_end_jisage_wraps_the_whole_line() {
        let html = convert(
            "タイトル\r\n\r\n本文の行です［＃３字下げ］",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("<div class=\"jisage_3\" style=\"margin-left: 3em\">本文の行です</div>"),
            "実際: {html}"
        );
    }

    /// ［＃N字下げ］が行に単独なら、その行から複数行ブロックになる
    #[test]
    fn test_bare_line_jisage_opens_a_block() {
        let html = convert(
            "タイトル\r\n\r\n［＃３字下げ］\r\n本文A\r\n［＃字下げ終わり］",
            &RenderOptions::default(),
        );
        assert!(
            html.contains(
                "<div class=\"jisage_3\" style=\"margin-left: 3em\">\r\n本文A<br />\r\n</div>"
            ),
            "実際: {html}"
        );
    }

    /// 行の区切りは CRLF だけ。単独の LF は本文の文字として扱う
    /// （参照実装 Jstream が CRLF のみを改行とみなすため）
    #[test]
    fn test_lone_line_feed_is_not_a_line_break() {
        let html = convert("タイトル\r\n\r\nあ\nい", &RenderOptions::default());
        assert!(html.contains("あ\nい<br />"), "実際: {html}");
    }

    /// 句点コード指定の前方参照は対象の文字を外字画像に置き換える
    #[test]
    fn test_kuten_reference_becomes_a_gaiji_image() {
        let html = convert(
            "タイトル\r\n\r\n全集5［＃「5」はローマ数字、1-13-25］巻",
            &RenderOptions::default(),
        );
        assert!(
            html.contains(
                "全集<img src=\"../../../gaiji/1-13/1-13-25.png\" alt=\"※()\" class=\"gaiji\" />巻"
            ),
            "実際: {html}"
        );
    }

    /// 対象が前方に見つからなければ、もとの文字列のまま注記にする
    #[test]
    fn test_unresolved_reference_keeps_the_original_text() {
        let src = "「麾」の「毛」に代えて「公の右上の欠けたもの」、第4水準2-94-57";
        let html = convert(
            &format!("タイトル\r\n\r\n本文［＃{src}］"),
            &RenderOptions::default(),
        );
        assert!(
            html.contains(&format!("<span class=\"notes\">［＃{src}］</span>")),
            "実際: {html}"
        );
    }

    /// 入力末尾の改行の有無で奥付末尾の <br /> の数が変わる
    #[test]
    fn test_trailing_newline_changes_the_final_break_count() {
        let src = "タイトル\r\n\r\n本文\r\n\r\n底本：「甲」乙\r\n入力：誰か";
        let without = convert(src, &RenderOptions::default());
        let with = convert(&format!("{src}\r\n"), &RenderOptions::default());
        assert!(
            without.contains("入力：誰か<br />\r\n<br />\r\n</div>"),
            "実際: {without}"
        );
        assert!(
            with.contains("入力：誰か<br />\r\n<br />\r\n<br />\r\n</div>"),
            "実際: {with}"
        );
    }

    /// 画像化できない外字の ※ は本文セクションでだけ付く。
    /// 参照実装の tail_output は general_output と違い ※ を出さない。
    #[test]
    fn test_gaiji_mark_only_in_the_main_text() {
        let html = convert(
            "タイトル\r\n\r\n刺※［＃「卓＋戈」、U+39B8］\r\n\r\n底本：「甲」乙\r\n刺※［＃「卓＋戈」、U+39B8］\r\n",
            &RenderOptions::default(),
        );
        let main = html.split("bibliographical_information").next().unwrap();
        let tail = html.split("bibliographical_information").nth(1).unwrap();
        assert!(
            main.contains("刺※<span class=\"notes\">"),
            "本文には ※ が付く"
        );
        assert!(
            tail.contains("刺<span class=\"notes\">"),
            "奥付には ※ が付かない"
        );
    }

    /// 注記の中身もルビや外字を解決する
    /// （参照実装は read_to_nest で注記の中身を TagParser に渡す）
    #[test]
    fn test_ruby_inside_a_note_is_resolved() {
        let html = convert(
            "タイトル\r\n\r\n［＃現代語訳「松籟《しょうらい》を聞く」］",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("<ruby><rb>松籟</rb><rp>（</rp><rt>しょうらい</rt><rp>）</rp></ruby>"),
            "実際: {html}"
        );
    }

    /// 同じ外字が複数回現れたら、外字一覧には出現箇所を「、」で並べる
    #[test]
    fn test_gaiji_list_collects_every_occurrence() {
        let html = convert(
            "タイトル\r\n\r\n※［＃「こざとへん＋井」、U+9631、133-8］\r\n※［＃「こざとへん＋井」、U+9631、140-2］",
            &RenderOptions::default(),
        );
        assert!(html.contains("133-8、140-2"), "実際: {html}");
    }

    /// 外字の説明に「、」がなければ、外字一覧の欄はどちらも空になる
    /// （参照実装の PAT_GAIJI が「、」を必須とするため）
    #[test]
    fn test_gaiji_without_a_comma_yields_an_empty_row() {
        let html = convert(
            "タイトル\r\n\r\n大※［＃「大※」に傍点］",
            &RenderOptions::default(),
        );
        assert!(!html.contains("「大※」に傍点</td>"), "実際: {html}");
    }

    /// 注記の直後にルビが来ると、参照実装では注記自身がルビの親文字になる
    #[test]
    fn test_note_before_ruby_becomes_the_ruby_base() {
        let html = convert(
            "タイトル\r\n\r\n鈍［＃「鈍」は底本では「鋭」］《にぶ》い",
            &RenderOptions::default(),
        );
        assert!(
            html.contains(
                "鈍<ruby><rb><span class=\"notes\">［＃「鈍」は底本では「鋭」］</span></rb>"
            ),
            "実際: {html}"
        );
    }

    /// 画像化できない外字がルビの親文字にあると、親文字には ※ だけが入り
    /// 注記は </ruby> の後ろに出る
    #[test]
    fn test_unconvertible_gaiji_note_moves_after_the_ruby() {
        let html = convert(
            "タイトル\r\n\r\n陥※［＃「こざとへん＋井」、U+9631、133-8］《おとしあな》",
            &RenderOptions::default(),
        );
        assert!(
            html.contains("<ruby><rb>陥※</rb><rp>（</rp><rt>おとしあな</rt><rp>）</rp></ruby><span class=\"notes\">"),
            "実際: {html}"
        );
    }
}
