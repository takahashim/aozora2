//! 行末の改行制御（[`crate::ast::Break`]）を決める規則。
//!
//! 参照実装 `general_output` は、その行の出力が「ブロックの閉じタグで終わる」
//! （`</div>` や `</hN>`）とき `@terprip` を倒して行末の `<br />` を出さない。
//! 描画済み HTML を見て決めていたその判断を、Lower 時にインライン列から決める形に
//! 移したもの（バックエンドの HTML 詮索を無くす。architecture.md §4.3）。
//!
//! 判断の中身は参照実装の HTML 出力形に由来するので、backend-neutral な型を置く
//! [`crate::ast`] ではなく Lowerer 側に置く。

use crate::ast::{BlockKind, Break, Inline, InlineKind};
use crate::node::MidashiStyle;

/// この行のインライン列を描画すると、行末がブロックの閉じタグ（`</div>`/`</hN>`）に
/// なるか。参照実装が行末 `<br />` を抑制する形かどうかの判定で、末尾のインライン
/// だけで決まる（参照は描画済みバッファの行末タグを見るため）。
pub fn line_emits_closing_block_tag(inlines: &[Inline]) -> bool {
    match inlines.last() {
        // 見出しは Normal のみ `</hN>`（dogyo-/mado- はインラインなので br）。
        Some(Inline {
            kind: InlineKind::Midashi { style, .. },
            ..
        }) => *style == MidashiStyle::Normal,
        // 行末で開いた地付き（`…</div>`）。
        Some(Inline {
            kind: InlineKind::ChitsukiInline { .. },
            ..
        }) => true,
        // 同行開閉のブロック形（div で包むか、Normal 見出し）。
        Some(Inline {
            kind: InlineKind::BlockInline { kind, .. },
            ..
        }) => block_kind_emits_closing_tag(kind),
        _ => false,
    }
}

/// `BlockInline` の種類が末尾 `</div>`/`</hN>` になるか。
fn block_kind_emits_closing_tag(kind: &BlockKind) -> bool {
    match kind {
        BlockKind::Midashi { style, .. } => *style == MidashiStyle::Normal,
        // div で包む種類は末尾が `</div>`。
        BlockKind::Jisage { .. }
        | BlockKind::Chitsuki { .. }
        | BlockKind::Jizume { .. }
        | BlockKind::Keigakomi
        | BlockKind::Yokogumi
        | BlockKind::Caption
        | BlockKind::FontSize { .. }
        | BlockKind::Futoji
        | BlockKind::Shatai => true,
        // Burasage は BlockInline には現れない。
        BlockKind::Burasage(_) => false,
    }
}

/// 通常見出し（同行・窓でない）が行のどこかに現れるか。入れ子の中まで見る。
///
/// 参照実装は見出しコマンドに**出会った時点で** `@terprip = false` を立てる
/// （`aozora2html.rb` の OMIDASHI/NAKAMIDASHI/KOMIDASHI_COMMAND と、後方参照形の
/// `midashi_type = :normal` の枝）。行末に来たかどうかは関係ない**行単位の旗**なので、
/// 末尾だけを見る [`line_emits_closing_block_tag`] では次のような行を取りこぼす。
///
/// - `［＃中見出し］あいう［＃中見出し終わり］つづき。` … 見出しの後ろに本文が続く
/// - `｜［＃中見出し］あいう［＃中見出し終わり］《るび》` … 見出しがルビ親文字の中
///
/// 同行見出し・窓見出しは参照も旗を立てないので対象外。
fn contains_normal_midashi(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match &inline.kind {
        InlineKind::UnclosedAccentBreak => false,
        InlineKind::Midashi { style, .. } => *style == MidashiStyle::Normal,
        InlineKind::BlockInline {
            kind: BlockKind::Midashi { style, .. },
            ..
        } => *style == MidashiStyle::Normal,
        // 入れ子（ルビ親文字・装飾の中など）も見る。
        InlineKind::Ruby { base, ruby, .. } => {
            contains_normal_midashi(base) || contains_normal_midashi(ruby)
        }
        InlineKind::Style { children, .. }
        | InlineKind::FontSize { children, .. }
        | InlineKind::Tcy { children }
        | InlineKind::Keigakomi { children }
        | InlineKind::Yokogumi { children }
        | InlineKind::Caption { children }
        | InlineKind::Warigaki { children }
        | InlineKind::ChitsukiInline { children, .. }
        | InlineKind::BlockInline { children, .. } => contains_normal_midashi(children),
        InlineKind::Text(_)
        | InlineKind::Gaiji { .. }
        | InlineKind::Accent { .. }
        | InlineKind::Img { .. }
        | InlineKind::Kaeriten(_)
        | InlineKind::Okurigana { .. }
        | InlineKind::DakutenKatakana { .. }
        | InlineKind::Note { .. }
        | InlineKind::AnnotationEnd { .. }
        | InlineKind::Warichu { .. } => false,
    })
}

/// 内容行の行末改行。
///
/// `［＃ここで…終わり］` を含む行（`has_explicit_close`）と、行末がブロックの閉じタグに
/// なる行では参照が `@terprip` を倒すので `<br />` を出さない。通常見出しは位置に
/// よらず旗を倒すので別に見る（[`contains_normal_midashi`]）。
pub fn content_break(inline: &[Inline], has_explicit_close: bool) -> Break {
    if has_explicit_close || line_emits_closing_block_tag(inline) || contains_normal_midashi(inline)
    {
        Break::None
    } else {
        Break::Br
    }
}

#[cfg(test)]
mod tests {
    use crate::html::{convert, RenderOptions};

    /// 通常見出しは**行のどこにあっても**行末 `<br />` を抑制する。
    /// 参照実装は見出しコマンドに出会った時点で `@terprip = false` を立てるためで、
    /// 末尾だけを見ていた頃は見出しの後ろに本文が続く行を取りこぼしていた。
    /// 期待値は参照実装に同じ入力を与えて実測した。
    #[test]
    fn normal_midashi_suppresses_line_break_wherever_it_appears() {
        let body = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n{line}\r\n\r\n底本：「テスト」\r\n");
            convert(&src, &RenderOptions::default())
        };
        // 見出しの後ろに本文が続く（末尾は Text）。
        let after = body("［＃中見出し］あいう［＃中見出し終わり］つづき。");
        assert!(
            after.contains("</h4>つづき。\r\n"),
            "見出しの後ろに本文が続いても <br /> を出さない: {after:?}"
        );
        // 後方参照形でも同じ。
        let backref = body("あいう［＃「あいう」は中見出し］つづき。");
        assert!(backref.contains("</h4>つづき。\r\n"), "{backref:?}");
        // 見出しがルビ親文字の中にあっても効く。
        let in_ruby = body("｜［＃中見出し］あいう［＃中見出し終わり］《るび》");
        assert!(
            in_ruby.contains("</ruby>\r\n"),
            "ルビに包まれた見出しでも <br /> を出さない: {in_ruby:?}"
        );
        // 同行見出し・窓見出しは参照も旗を立てないので <br /> が付く。
        let dogyo = body("あいう［＃「あいう」は同行大見出し］つづき。");
        assert!(dogyo.contains("つづき。<br />"), "{dogyo:?}");
    }
}
