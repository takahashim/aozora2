//! Lowerer: RawAST（平坦マーカー）→ Aozora AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1/§4.3。
//! 参照実装 `@indent_stack`/`implicit_close`/`@terprip` の逐次モデルを Lower 時に
//! 一度だけ計算し、ブロックを部分木に畳み、行末の改行を [`Break`] メタデータへ
//! 載せる。バックエンドはこの木を状態なしに歩くだけになる。
//!
//! 畳み込みは 3 段に分かれる（`docs/spec-lowerer-constraints.md`）。
//!
//! 1. [`facts`]: 行ごとの事実（制約 1 の役割の割り当て）。行内で閉じる純関数。
//! 2. [`solve`]: 事実の列から [`plan::LowerPlan`] を組む（制約 3・4・6・7・8）。
//! 3. [`plan::materialize`]: `LowerPlan` を Aozora AST へ写す純関数。
//!
//! [`block_kind_of`] が写せないブロック種はトップレベルに落ちる。

pub mod break_policy;
mod facts;
pub mod inline;
mod plan;
mod solve;

use plan::materialize;
use solve::solve;

use crate::ast::{AozoraAst, BlockKind, BurasageGeometry};
use crate::node::BlockType;
use crate::parser::RawDoc;

/// Lower 時に検出できる構造上の診断。エディタ支援用の付加情報で、変換出力には影響しない。
///
/// いずれも**参照実装が変換を中止する入力**に対応する（メッセージつきで
/// `処理を停止します` と出るもの。実測）。木は総関数として作り続け、厳格さは
/// 境界（CLI）がこの診断を見て決める。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerDiagnostic {
    /// 対象の本文行（0 起点）。
    pub line: usize,
    /// 何が起きたか。
    pub kind: LowerDiagnosticKind,
}

/// [`LowerDiagnostic`] の種類。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerDiagnosticKind {
    /// EOF まで閉じられなかったブロック。`line` は**開いた行**。
    /// 参照は「〈種類〉中に本文が終了しました」で停止する（実測）。
    UnclosedBlock(BlockKind),
    /// 閉じる相手が無い終端。`line` は**終端のある行**。
    /// 参照 `check_close_match` は書かれた種類と最内層を比べ、違えば
    /// 「〈種類〉を閉じようとしましたが、〈種類〉中ではありません」で停止する（実測）。
    UnmatchedEnd {
        /// 記法に書かれた種類。
        written: BlockType,
        /// そのとき最も内側で開いていたブロック（何も開いていなければ `None`）。
        innermost: Option<BlockKind>,
    },
}

/// RawDoc（未解決・平坦マーカー）を Aozora AST（[`AozoraAst`]＝トップレベル [`Block`] 列）に畳む。
pub fn lower_to_blocks(raw: &RawDoc) -> AozoraAst {
    lower_to_blocks_with_diagnostics(raw).0
}

/// [`lower_to_blocks`] と同じ畳み込みを行い、加えて構造上の診断（EOF で閉じられなかった
/// ブロック）を返す。**Block 出力は `lower_to_blocks` と完全一致**（診断は追加返却のみで
/// 変換結果には一切影響しない＝オラクル不変）。エディタ支援 `analysis` が使う。
pub fn lower_to_blocks_with_diagnostics(raw: &RawDoc) -> (AozoraAst, Vec<LowerDiagnostic>) {
    materialize(solve(raw))
}

/// RawAST の BlockType＋params をAozora ASTの BlockKind に写す（対応済みのものだけ）。
pub(crate) fn block_kind_of(
    block_type: &BlockType,
    params: &crate::node::BlockParams,
) -> Option<BlockKind> {
    let w = || params.width.unwrap_or(0);
    match block_type {
        BlockType::Jisage => Some(BlockKind::Jisage {
            width: params.width,
        }),
        BlockType::Chitsuki => Some(BlockKind::Chitsuki { width: w() }),
        BlockType::Jizume => Some(BlockKind::Jizume {
            width: params.width,
        }),
        BlockType::Keigakomi => Some(BlockKind::Keigakomi),
        BlockType::Yokogumi => Some(BlockKind::Yokogumi),
        BlockType::Caption => Some(BlockKind::Caption),
        BlockType::FontDai => Some(BlockKind::FontSize {
            size_type: crate::node::FontSizeType::Dai,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::FontSho => Some(BlockKind::FontSize {
            size_type: crate::node::FontSizeType::Sho,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::Futoji => Some(BlockKind::Futoji),
        BlockType::Shatai => Some(BlockKind::Shatai),
        BlockType::Burasage => Some(BlockKind::Burasage(BurasageGeometry {
            wrap_width: params.wrap_width,
            width: params.width,
        })),
        BlockType::Midashi => Some(BlockKind::Midashi {
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;
    use crate::ast::{Block, Break, CloseKind, OpenKind};
    use crate::parser::parse_document_raw;

    /// Aozora ASTの各ブロックが由来の本文行番号（位置情報）を持つ。
    #[test]
    fn blocks_carry_source_line_numbers() {
        // 本文（extract 後を模した行列）。0 起点で数える。
        let lines = vec![
            "本文0",                    // 0
            "［＃ここから２字下げ］",   // 1 (Nested open)
            "内容2",                    // 2
            "［＃ここで字下げ終わり］", // 3
        ];
        let raw = parse_document_raw(&lines);
        let blocks = lower_to_blocks(&raw);
        // [ Line(本文0, line0), Nested(open line1, 子[Line(内容2, line2)]) ]
        assert!(matches!(blocks[0], Block::Line { line: 0, .. }));
        match &blocks[1] {
            Block::Nested { line, children, .. } => {
                assert_eq!(*line, 1, "Nested は開いた行1");
                assert!(matches!(children[0], Block::Line { line: 2, .. }));
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 本文の途中（行末）に現れた「ここで…終わり」もその場でブロックを閉じる。
    ///
    /// 参照実装は行を逐次出力するので、閉じタグより前の本文は閉じるブロックの内側に
    /// 出て、行末の改行は閉じタグが出す。現実の入力では CRLF 区切りの行に孤立 LF が
    /// 混ざる形で現れる（例: 宮本百合子「千世子」000311/15945 の1箇所が
    /// `"\n［＃ここで字下げ終わり］"`）。
    #[test]
    fn block_end_after_text_closes_the_block_on_that_line() {
        let lines = vec![
            "［＃ここから１字下げ］",
            "内容",
            "\n［＃ここで字下げ終わり］",
            "後続",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested {
                children, close, ..
            } => {
                assert_eq!(children.len(), 2, "本文行と行途中クローズ前の本文");
                assert!(matches!(
                    children[1],
                    Block::Line {
                        brk: Break::NoNewline,
                        ..
                    }
                ));
                assert_eq!(*close, CloseKind::Newline);
            }
            other => panic!("Nested を期待: {other:?}"),
        }
        assert!(matches!(blocks[1], Block::Line { line: 3, .. }));
    }

    /// 行途中クローズの後ろに本文が続く場合、閉じタグは `</div>`（改行なし）で、
    /// 行末の改行は後続本文が出す（例: 000081/48220 の
    /// `（正方形にやりますか。）［＃ここで字下げ終わり］どういふ訳か…`）。
    #[test]
    fn block_end_between_texts_closes_and_continues_on_the_same_line() {
        let lines = vec!["［＃ここから４字下げ］", "前［＃ここで字下げ終わり］後"];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { close, .. } => assert_eq!(*close, CloseKind::NoBreak),
            other => panic!("Nested を期待: {other:?}"),
        }
        // 後続本文は同じ行として `\r\n` を出す（explicit なので `<br />` は抑制）。
        assert!(matches!(
            blocks[1],
            Block::Line {
                brk: Break::None,
                line: 1,
                ..
            }
        ));
    }

    /// 行の途中で開く複数行ブロックは、開始タグをその場に出して同じ行に内容を
    /// 続ける（開始タグ直後に改行を出さない＝`OpenKind::NoBreak`）。
    ///
    /// 例: 001065/18361 の `　［＃ここから斜体］Fourscore and seven…`、
    /// 001841/57318 の `［＃ここからキャプション］図３　ペラグラ患者。`
    #[test]
    fn block_start_mid_line_opens_and_continues_on_the_same_line() {
        let lines = vec!["　［＃ここから斜体］前半", "後半［＃ここで斜体終わり］"];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        // 開始タグより前の本文はブロックの外・改行なし。
        assert!(matches!(
            blocks[0],
            Block::Line {
                brk: Break::NoNewline,
                ..
            }
        ));
        match &blocks[1] {
            Block::Nested {
                kind,
                children,
                open,
                ..
            } => {
                assert_eq!(*kind, BlockKind::Shatai);
                assert_eq!(*open, OpenKind::NoBreak);
                assert_eq!(children.len(), 2, "同行の後続本文と次行の本文");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 同じ行で開閉する範囲形は `to_inlines` が BlockInline に畳むので、
    /// ブロックとしては開かない。
    #[test]
    fn block_range_closed_on_the_same_line_stays_inline() {
        let blocks = lower_to_blocks(&parse_document_raw(&[
            "前［＃ここから斜体］中［＃ここで斜体終わり］後",
        ]));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Line { .. }));
    }

    /// 行末クローズの前半は Text だけとは限らない（行途中の地付き・ルビ等）。
    /// 拾うのは「同じ行に対応する開始が無い BlockEnd」で、同じ行で開閉する範囲形は
    /// to_inlines がインラインに畳むので対象外。
    ///
    /// 例: 001848/59607 の
    /// `ウェヌス…蒔《ま》かんとする時、［＃地から２字上げ］（ルクレティウス）［＃ここで字下げ終わり］`
    #[test]
    fn block_end_closes_even_when_head_has_inline_markers() {
        let lines = vec![
            "［＃ここから２字下げ］",
            "本文《ほんぶん》。［＃地から２字上げ］（出典）［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { kind, close, .. } => {
                assert_eq!(*kind, BlockKind::Jisage { width: Some(2) });
                assert_eq!(*close, CloseKind::Newline);
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 複数行ブロックになれない種類の「終わり」でブロックを閉じない。割り注は
    /// 開始側が注記として描画され BlockStart ノードを作らないので、素朴に
    /// 「対応する開始が無い」と見ると外側の字下げを誤って閉じてしまう
    /// （000284/2227 で実際に起きた）。
    #[test]
    fn warichu_end_does_not_close_the_enclosing_block() {
        let lines = vec![
            "［＃ここから３字下げ］",
            "本文。［＃ここから割り注］注［＃ここで割り注終わり］続き",
            "［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { children, .. } => {
                assert_eq!(children.len(), 1, "割り注の行は字下げの中に留まる");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 単独行の `［＃割り注終わり］` はブロックを閉じない。参照 apply_warichu は
    /// indent_stack に触れず `）</span>` を出すだけなので、外側の字下げは続く。
    /// 「単独行の BlockEnd はブロック終了」と素朴に扱っていた頃は、字下げを
    /// 途中で閉じたうえに割り注の出力も落としていた。期待値は参照実装で実測した。
    #[test]
    fn standalone_warichu_end_line_does_not_close_the_enclosing_block() {
        let body = |lines: &[&str]| {
            let src = format!(
                "作品名\r\n著者\r\n\r\n{}\r\n\r\n底本：「テスト」\r\n",
                lines.join("\r\n")
            );
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        let html = body(&[
            "［＃ここから２字下げ］",
            "ほんぶん",
            "［＃割り注終わり］",
            "あとの行",
            "［＃ここで字下げ終わり］",
        ]);
        assert!(
            html.contains("ほんぶん<br />\r\n）</span><br />\r\nあとの行<br />\r\n</div>"),
            "{html}"
        );
        // ブロックの外でも同じく、閉じずに割り注の出力だけを出す。
        let outside = body(&["ぜん", "［＃割り注終わり］", "あと"]);
        assert!(
            outside.contains("ぜん<br />\r\n）</span><br />\r\nあと<br />"),
            "{outside}"
        );
    }

    /// 1行に「終わり」が複数あれば現れた順に閉じる。行末の改行を出すのは最後の
    /// 閉じだけなので `</div></div>\r\n` になる。
    ///
    /// 例: 001097/49825 の `［＃ここで小さな文字終わり］［＃ここで字下げ終わり］`
    #[test]
    fn multiple_block_ends_on_one_line_close_in_order() {
        let lines = vec![
            "［＃ここから７字下げ］",
            "［＃ここから１段階小さな文字］",
            "本文",
            "［＃ここで小さな文字終わり］［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            // 外側の字下げが最後に閉じるので改行はこちらが出す。
            Block::Nested {
                kind,
                close,
                children,
                ..
            } => {
                assert_eq!(*kind, BlockKind::Jisage { width: Some(7) });
                assert_eq!(*close, CloseKind::Newline);
                match &children[0] {
                    // 内側の小さな文字は改行なしの `</div>`。
                    Block::Nested { close, .. } => assert_eq!(*close, CloseKind::NoBreak),
                    other => panic!("Nested を期待: {other:?}"),
                }
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 行途中の地付き（is_block=false の Chitsuki）は行末で閉じるので、同じ行の
    /// `［＃ここで地付き終わり］` は前の行から続く複数行の地付きを閉じる。
    ///
    /// 参照実装で実測（`Ａ［＃地付き］Ｂ［＃ここで地付き終わり］` は
    /// `Ａ<div class="chitsuki_0">Ｂ</div></div>` になる）。
    #[test]
    fn inline_chitsuki_does_not_absorb_the_multiline_chitsuki_end() {
        let lines = vec![
            "［＃ここから地付き］",
            "本文",
            "Ａ［＃地付き］Ｂ［＃ここで地付き終わり］",
            "後の行",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { kind, close, .. } => {
                assert_eq!(*kind, BlockKind::Chitsuki { width: 0 });
                assert_eq!(*close, CloseKind::Newline, "この行で閉じる");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
        assert!(
            matches!(blocks[1], Block::Line { line: 3, .. }),
            "後の行はブロックの外: {:?}",
            blocks[1]
        );
    }

    /// 対応する開きが無ければ閉じタグは出さず、通常の内容行として扱う。
    #[test]
    fn block_end_after_text_without_open_block_is_plain_content() {
        let blocks = lower_to_blocks(&parse_document_raw(&["本文［＃ここで字下げ終わり］"]));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            blocks[0],
            Block::Line {
                brk: Break::None,
                ..
            }
        ));
    }

    /// `［＃N字下げ］` は 1 行に複数書ける。参照 apply_jisage は見つけるたび
    /// バッファ先頭へ unshift するので、**後に書いたものほど外側**の div になる。
    /// また `｜` の直後に書くとトークナイザがルビ親文字へ取り込むが、参照は
    /// ルビの状態に関わらず行を包む。どちらも参照実装で実測した。
    #[test]
    fn line_jisage_can_repeat_and_hide_in_ruby_base() {
        let body = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n{line}\r\n\r\n底本：「テスト」\r\n");
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        // 後に書いた ５字下げ が外側。
        let twice = body("［＃２字下げ］あ［＃５字下げ］い");
        assert!(
            twice.contains(
                "<div class=\"jisage_5\" style=\"margin-left: 5em\">\
                 <div class=\"jisage_2\" style=\"margin-left: 2em\">あい</div></div>"
            ),
            "{twice}"
        );
        // ｜ の直後（ルビ親文字の中）でも行を包む。
        let in_ruby = body("｜［＃２字下げ］あいう《るび》");
        assert!(
            in_ruby.contains("<div class=\"jisage_2\" style=\"margin-left: 2em\"><ruby>"),
            "{in_ruby}"
        );
        assert!(in_ruby.contains("</ruby></div>"), "{in_ruby}");
    }

    /// 幅の数字が無い `［＃字下げ］` `［＃ここから字詰め］` も参照は受理し、
    /// 空幅のまま不正な CSS を出す（Quirk empty_indent_css）。注記化すると
    /// div ごと消えてしまう。期待値は参照実装で実測した。
    #[test]
    fn empty_width_indent_commands_are_accepted() {
        let body = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n{line}\r\n\r\n底本：「テスト」\r\n");
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        let jisage = body("［＃字下げ］あいう");
        assert!(
            jisage.contains("<div class=\"jisage_\" style=\"margin-left: em\">あいう</div>"),
            "{jisage}"
        );
        let jizume = body("［＃ここから字詰め］\r\nあいう\r\n［＃ここで字詰め終わり］");
        assert!(
            jizume.contains("<div class=\"jizume_\" style=\"width: em\">"),
            "{jizume}"
        );
    }

    /// `［＃N字下げ］` や行スコープ地付きは、同じ行に `［＃ここで…終わり］` が
    /// あっても行を包む。classify_line は「終わり」を含む行を先に Closes として
    /// 扱うので、行スコープ包みの判定まで来ないのが原因だった。
    /// 参照 apply_jisage は閉じの有無に関わらずバッファへ unshift するだけ（実測）。
    #[test]
    fn line_scope_wrap_survives_a_close_on_the_same_line() {
        let body = |lines: &[&str]| {
            let src = format!(
                "作品名\r\n著者\r\n\r\n{}\r\n\r\n底本：「テスト」\r\n",
                lines.join("\r\n")
            );
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        let jisage = body(&[
            "［＃ここから２字下げ、折り返して４字下げ］",
            "まえ。",
            "［＃２字下げ］あいう［＃ここで字下げ終わり］",
        ]);
        assert!(
            jisage.contains("<div class=\"jisage_2\" style=\"margin-left: 2em\">あいう</div>"),
            "{jisage}"
        );
        let chitsuki = body(&[
            "［＃ここから２字下げ］",
            "まえ。",
            "［＃地付き］あいう［＃ここで字下げ終わり］",
        ]);
        assert!(
            chitsuki.contains(
                "chitsuki_0\" style=\"text-align:right; margin-right: 0em\">あいう</div>"
            ),
            "{chitsuki}"
        );
    }
}
