//! 2 つの AST が仕様で謳う**不変条件**を検査する。
//!
//! `conformance.rs` は「実装の出力」と「フィクスチャ」を突き合わせるスナップショット
//! 比較なので、出力が変わったことは分かっても、それが仕様の不変条件を満たすかは見て
//! いない。ここはその穴を埋める。規定は docs/spec-rawast-json.md /
//! docs/spec-aozora-ast-json.md の「不変条件」（span の意味論は docs/spec-ast.md）。

use aozora_core::ast::{Block, Inline, InlineKind};
use aozora_core::html::Quirks;
use aozora_core::interchange::{AozoraDocument, RawDocument};
use aozora_core::lower::lower_to_blocks;
use aozora_core::node::NodeKind;
use aozora_core::parser::parse_document_raw;

/// 記法を一通り含む本文（不変条件の検査に使う共通の入力）。
const SOURCE: &str = "東京《とうきょう》へ\n\
     本文［＃「本文」に傍点］と※［＃「けものへん＋苗」、第3水準1-87-63］\n\
     ［＃ここから２字下げ］\n\
     中身［＃「中身」は中見出し］\n\
     ［＃ここで字下げ終わり］つづき\n\
     ［＃３字下げ］行スコープの包み\n\
     〔未閉じの行\n\
     次の行\n\
     ［＃ここから改行天付き、折り返して１字下げ］開始行\n\
     ぶら下げの中\n\
     ［＃ここで字下げ終わり］\n\
     解決できない［＃「存在しない対象」に傍点］\n\
     12［＃「12」は縦中横］［＃割り注］注［＃割り注終わり］";

fn lines() -> Vec<&'static str> {
    SOURCE.split('\n').collect()
}

/// RawAST の不変条件4（可逆）: 行を連結すれば元のテキストに戻る。
///
/// `source` は行の原文をそのまま持つ、というのが RawAST を「フォーマッタ・記法リンタ・
/// エディタ支援」に使える根拠なので、実際に戻ることを縛る。
#[test]
fn raw_ast_round_trips_to_the_original_text() {
    let lines = lines();
    let raw = parse_document_raw(&lines);

    assert_eq!(raw.lines.len(), lines.len(), "1 ソース行 = 1 RawLine");
    let restored: Vec<&str> = raw.lines.iter().map(|l| l.source.as_str()).collect();
    assert_eq!(restored, lines, "source を連結すると元の本文に戻る");

    // 行番号は本文 0 起点の通し番号。
    for (i, line) in raw.lines.iter().enumerate() {
        assert_eq!(line.line_no, i, "{i} 行目の line_no");
    }
}

/// RawAST の不変条件3（未解決）: 前方参照は `UnresolvedReference` のまま残る。
///
/// 解決してしまうと RawAST が Aozora AST に近づき、2 つに分けている意味が無くなる。
#[test]
fn raw_ast_keeps_forward_references_unresolved() {
    let raw = parse_document_raw(&lines());
    let has_unresolved = raw
        .lines
        .iter()
        .flat_map(|l| &l.nodes)
        .any(|n| matches!(n.kind, NodeKind::UnresolvedReference { .. }));
    assert!(
        has_unresolved,
        "前方参照を含む入力なのに UnresolvedReference が 1 つも無い"
    );
}

fn walk_inlines(inlines: &[Inline], f: &mut impl FnMut(&Inline)) {
    for inline in inlines {
        f(inline);
        for children in inline_children(&inline.kind) {
            walk_inlines(children, f);
        }
    }
}

/// 子インライン列をすべて返す。網羅一致にして、変種を足したとき検査から漏れないようにする。
fn inline_children(kind: &InlineKind) -> Vec<&[Inline]> {
    match kind {
        InlineKind::Ruby { base, ruby, .. } => vec![base, ruby],
        InlineKind::AnnotationEnd { content, .. } => vec![content],
        InlineKind::Note { content, .. } | InlineKind::Okurigana { content, .. } => vec![content],
        InlineKind::Style { children, .. }
        | InlineKind::Midashi { children, .. }
        | InlineKind::Tcy { children }
        | InlineKind::Keigakomi { children }
        | InlineKind::Yokogumi { children }
        | InlineKind::Caption { children }
        | InlineKind::Warigaki { children }
        | InlineKind::FontSize { children, .. }
        | InlineKind::ChitsukiInline { children, .. }
        | InlineKind::BlockInline { children, .. } => vec![children],
        InlineKind::Text(_)
        | InlineKind::Gaiji { .. }
        | InlineKind::Accent { .. }
        | InlineKind::Img { .. }
        | InlineKind::Warichu { .. }
        | InlineKind::Kaeriten(_)
        | InlineKind::DakutenKatakana { .. }
        | InlineKind::UnclosedAccentBreak => vec![],
    }
}

fn walk_blocks(blocks: &[Block], f: &mut impl FnMut(&Inline)) {
    for block in blocks {
        match block {
            Block::Line { inline, .. } | Block::LineWrap { inline, .. } => walk_inlines(inline, f),
            Block::Nested { children, .. } => walk_blocks(children, f),
        }
    }
}

/// Aozora AST の不変条件1（解決済み）: 未解決の参照は残らず、解決できなかったものは
/// 編集者注になる。
///
/// `Inline` には未解決参照を表す変種が無いので「残らない」は型で保証されるが、
/// **黙って消えていない**ことは型では言えない。解決に失敗した参照が注記として
/// 出ていることを確かめる（`to_inlines` は未解決参照を落とすので、そこへ来ていたら
/// 内容が失われる）。
#[test]
fn aozora_ast_turns_unresolvable_references_into_notes() {
    let raw = parse_document_raw(&lines());
    let ast = lower_to_blocks(&raw);

    let mut notes = Vec::new();
    walk_blocks(&ast, &mut |inline| {
        if let InlineKind::Note { raw, .. } = &inline.kind {
            notes.push(raw.clone());
        }
    });
    assert!(
        notes.iter().any(|r| r.contains("存在しない対象")),
        "解決できなかった前方参照が注記として残っていない: {notes:?}"
    );
}

/// Aozora AST の不変条件3（型付き・マーカーレス）: 生の `［＃…］` 文字列は残らない。
///
/// 例外は編集者注の `raw`（原文の併置。描画には使わない）だけ。テキストとして
/// `［＃` が漏れていたら、記法を畳みそこねて素通ししている。
#[test]
fn aozora_ast_leaves_no_raw_notation_in_text() {
    let raw = parse_document_raw(&lines());
    let ast = lower_to_blocks(&raw);

    let mut leaked = Vec::new();
    walk_blocks(&ast, &mut |inline| {
        if let InlineKind::Text(s) = &inline.kind {
            if s.contains("［＃") {
                leaked.push(s.clone());
            }
        }
    });
    assert!(
        leaked.is_empty(),
        "素の記法がテキストに残っている: {leaked:?}"
    );
}

/// Aozora AST の不変条件5（行番号を保持）: 各ブロックの `line` は本文の行数に収まる。
#[test]
fn aozora_ast_block_lines_stay_within_the_document() {
    let lines = lines();
    let ast = lower_to_blocks(&parse_document_raw(&lines));

    fn check(blocks: &[Block], max: usize) {
        for block in blocks {
            let line = match block {
                Block::Line { line, .. }
                | Block::LineWrap { line, .. }
                | Block::Nested { line, .. } => *line,
            };
            assert!(line < max, "行番号が本文の外を指している: {line} >= {max}");
            if let Block::Nested { children, .. } = block {
                check(children, max);
            }
        }
    }
    check(&ast, lines.len());
}

/// 互換フラグが木に触れるのは `unclosed_accent_break` だけである。
///
/// 他の 7 つは描画時（`html::block_renderer` / `html::document_renderer`）にしか
/// 読まれない。交換形式の消費者にとってこれは重要な性質で——JSON を受け取った側は
/// 「どの Quirks で作られたか」をほぼ気にしなくてよい——黙って崩れると気づけない。
/// レンダ用フラグを畳み込みで読み始めたら、この検査が落ちる。
#[test]
fn only_the_unclosed_accent_quirk_reaches_the_aozora_ast() {
    // Quirk が絡む記法を一通り含む文書（外字の入れ子・幅なし画像・アクセント・
    // ヘッダのエスケープ・空幅の字下げ・濁点付き片仮名・未閉じ `〔`）。
    let src = "題\"と\"引用\r\n著\r\n\r\n\
        ※［＃「「莎」の草かんむり」、1-84-25］\r\n\
        ［＃図（fig01.png）入る］\r\n\
        〔e^〕とワ゛［＃1-7-82］\r\n\
        ［＃ここから　字下げ］\r\n\
        中身\r\n\
        ［＃ここで字下げ終わり］\r\n\
        〔未閉じの行\r\n\
        次の行\r\n";

    // 既定（全オン）と「`unclosed_accent_break` だけオン」で木が一致する
    // ＝残る 7 つは畳み込みに影響しない。
    let only_unclosed = Quirks {
        unclosed_accent_break: true,
        ..Quirks::none()
    };
    assert_eq!(
        AozoraDocument::from_text(src, &Quirks::default()),
        AozoraDocument::from_text(src, &only_unclosed),
        "描画用の互換フラグが Aozora AST に漏れている"
    );

    // 逆に `unclosed_accent_break` は木を変える（この検査自体が空回りしていない証拠）。
    assert_ne!(
        AozoraDocument::from_text(src, &Quirks::default()),
        AozoraDocument::from_text(src, &Quirks::none()),
        "unclosed_accent_break が木に効いていない"
    );
}

/// RawAST はどの互換フラグからも独立している。
///
/// `RawDocument::from_text` が `Quirks` を受け取らないので型の上で保証されるが、
/// 将来フラグを足したくなったときに「ここは原文どおり」という意図が読めるよう残す。
/// RawAST の不変条件は可逆性（原文へ戻せること）なので、互換の都合で内容を落としたり
/// 足したりしてはならない。
#[test]
fn the_raw_ast_is_independent_of_quirks() {
    let src = "題\r\n著\r\n\r\n〔未閉じの行\r\n次の行\r\n";
    let raw = RawDocument::from_text(src);
    assert_eq!(raw.to_text(), src);
    // 同じ RawAST から、フラグ違いの 2 つの Aozora AST が導ける
    // （フラグは畳み込みの引数であって、RawAST に焼き付いていない）。
    assert_ne!(
        raw.to_aozora(&Quirks::default()),
        raw.to_aozora(&Quirks::none())
    );
}

/// 行のノード列は、その行を**隙間なく覆う**（docs/spec-rawast-json.md）。
///
/// これが成り立つので、RawAST は各ノードに原文の写しを持たなくてよい——必要なら
/// `RawLine.source` を `span` で切れば取り出せる。破れやすいのはアクセントで、
/// `〔…〕` は木に残らず中身だけが並ぶため、区切りの 2 文字が宙に浮きやすい
/// （`parser::widen_to_delimiters` が両端のノードに引き受けさせている）。
#[test]
fn nodes_tile_their_line_without_gaps() {
    let sources = [
        SOURCE,
        // アクセント: 行頭・行中・行末、閉じない形、入れ子。
        "〔e'〕cole\n\
         前〔a e'〕後\n\
         行末が〔e'〕\n\
         〔未閉じの e' 行\n\
         東京《〔e'〕》\n\
         ［＃「〔e'〕」に傍点］",
    ];

    for src in sources {
        let lines: Vec<&str> = src.split('\n').collect();
        for line in parse_document_raw(&lines).lines {
            let width = line.source.chars().count();
            let mut pos = 0;
            for node in &line.nodes {
                assert_eq!(
                    node.span.start, pos,
                    "隙間か重なり: {:?} の {:?}",
                    line.source, node
                );
                pos = pos.max(node.span.end);
            }
            assert_eq!(pos, width, "行末まで覆えていない: {:?}", line.source);
        }
    }
}

/// 左ルビは木では `Ruby { direction: Left }` として保たれ、既定の HTML では
/// 元の変換系と同じく注記へ退避する（docs/spec-aozora-ast-json.md）。
#[test]
fn left_ruby_is_kept_in_the_tree_and_escapes_to_a_note_in_html() {
    use aozora_core::ast::{Inline, InlineKind};
    use aozora_core::html::{Quirks, RenderOptions};
    use aozora_core::interchange::AozoraDocument;

    let src = "題\r\n著\r\n\r\n対象［＃「対象」の左に「ヨミ」のルビ］\r\n";

    for quirks in [Quirks::default(), Quirks::none()] {
        let doc = AozoraDocument::from_text(src, &quirks);
        let mut found = None;
        walk_blocks(&doc.main_text, &mut |inline: &Inline| {
            if let InlineKind::Ruby {
                direction,
                note_fallback,
                ..
            } = &inline.kind
            {
                found = Some((*direction, note_fallback.is_some()));
            }
        });
        let (direction, has_fallback) = found.expect("左ルビが木に無い");
        assert_eq!(
            direction,
            aozora_core::node::RubyDirection::Left,
            "左ルビは direction=Left で持つ"
        );
        assert!(has_fallback, "退避先の内容を併せ持つ");
    }

    // 既定は注記へ退避（元の変換系と同じ出力）。
    let on = AozoraDocument::from_text(src, &Quirks::default()).to_html(&RenderOptions::default());
    assert!(
        on.contains("対象<span class=\"notes\">［＃「対象」の左に「ヨミ」のルビ］</span>"),
        "既定は注記へ退避する: {on}"
    );
    assert!(!on.contains("leftrb"), "既定でルビを出さない");

    // オフにすると左ルビとして出す。
    let quirks = Quirks::none();
    let off = AozoraDocument::from_text(src, &quirks)
        .to_html(&RenderOptions::default().with_quirks(quirks));
    assert!(
        off.contains("<ruby class=\"leftrb\"><rb>対象</rb>"),
        "オフでは左ルビを出す: {off}"
    );
}
