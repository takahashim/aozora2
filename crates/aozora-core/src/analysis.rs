//! エディタ向け静的解析レイヤ（LSP 的機能の基盤）。
//!
//! パーサの**位置情報付き**生ノード（[`crate::parser::parse_document_raw`] が返す
//! [`crate::parser::RawLine`] の `nodes`／`spans`）から、エディタが必要とする三つの
//! 派生情報を組み立てる純粋関数を提供する:
//!
//! - [`Analysis::tokens`]  … セマンティックトークン（正確なハイライト／ホバーの土台）
//! - [`Analysis::symbols`] … アウトライン（見出しの一覧＝ドキュメントシンボル）
//! - [`Analysis::diagnostics`] … 診断（解決できない注記／解決できない外字／未閉じブロック）
//!
//! すべて `convert()`（本文レンダリング）とは独立した**追加レイヤ**であり、オラクル
//! 出力には一切影響しない。位置は LSP と同じく **0 起点**（行・char とも）で、`end` は
//! 含まない半開区間。フロント（CodeMirror）側で行番号を +1 して用いる。

use crate::ast::{Block, BlockKind};
use crate::lower::lower_to_blocks_with_diagnostics;
use crate::node::{BlockType, MidashiLevel, Node, RefSpec};
use crate::parser::reference_resolver::resolve_references_collecting_failures;
use crate::parser::{parse_document_raw, RawLine};
use std::collections::HashSet;

#[cfg(feature = "serde")]
use serde::Serialize;

/// 行内の char 範囲（0 起点・`end` は含まない）。`line` も 0 起点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Range {
    /// 0 起点の行番号。
    pub line: usize,
    /// 行内の開始 char オフセット（含む）。
    pub start: usize,
    /// 行内の終了 char オフセット（含まない）。
    pub end: usize,
}

/// セマンティックトークンの種別。ハイライトの CSS クラスやホバー種別に対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum SemTokenKind {
    /// ルビ（`《…》`／`｜…《…》`）。
    Ruby,
    /// 見出し（大／中／小）。
    Heading,
    /// 傍点・傍線・太字などの強調系スタイル。
    Emphasis,
    /// 外字（`※［＃…］`）。
    Gaiji,
    /// アクセント記法。
    Accent,
    /// 画像（挿絵）。
    Image,
    /// その他の注記・ブロック記法（字下げ・地付き・縦中横・警句など）。
    Annotation,
}

/// セマンティックトークン 1 個。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SemToken {
    /// 位置。
    pub range: Range,
    /// 種別。
    pub kind: SemTokenKind,
    /// ホバー用の説明（外字の実文字・ルビ読み・見出しレベルなど）。無ければ `None`。
    pub detail: Option<String>,
}

/// アウトライン項目（見出し）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Symbol {
    /// 見出し記法の位置。
    pub range: Range,
    /// 見出しレベル（1=大, 2=中, 3=小）。
    pub level: u8,
    /// 見出しの表示テキスト。
    pub text: String,
}

/// 診断の重大度（LSP の DiagnosticSeverity に対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// 診断 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Diagnostic {
    /// 対象位置。
    pub range: Range,
    /// 重大度。
    pub severity: Severity,
    /// 機械可読なコード（種別の識別子）。
    pub code: &'static str,
    /// 人間向けメッセージ。
    pub message: String,
}

/// 折りたたみ可能な範囲（複数行ブロック）。行はいずれも 0 起点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct FoldRange {
    /// ブロックを開いた行。
    pub start_line: usize,
    /// ブロックの最終行（この行までを畳める）。
    pub end_line: usize,
}

/// 解析結果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Analysis {
    /// セマンティックトークン。
    pub tokens: Vec<SemToken>,
    /// アウトライン（見出し一覧）。
    pub symbols: Vec<Symbol>,
    /// 診断。
    pub diagnostics: Vec<Diagnostic>,
    /// 折りたたみ可能な範囲（複数行ブロック）。
    pub folds: Vec<FoldRange>,
}

/// バッファ全体を解析する。
///
/// 行は `\n` で分割し（末尾 `\r` は除去）、各行を位置情報付きでパースして
/// トークン／シンボル／診断を組み立てる。純粋関数で副作用は無い。
pub fn analyze(input: &str) -> Analysis {
    let lines: Vec<&str> = input.lines().collect();
    let doc = parse_document_raw(&lines);

    let mut analysis = Analysis::default();

    for raw in &doc.lines {
        // 前方参照はこの行のなかで解決される（ルビ親文字も含む）。生ノードでは未解決
        // なので、実際の解決を **1 行 1 回**走らせて、注記化された（＝解決に失敗した）
        // raw の集合を得ておく。個々の参照を再解決する二次コストを避けるため。
        let failed: HashSet<String> = {
            let mut nodes = raw.nodes.clone();
            resolve_references_collecting_failures(&mut nodes)
                .into_iter()
                .collect()
        };

        // `nodes[i]` と `spans[i]` は 1:1 対応（parse_raw_nodes_spanned の契約）。
        for (node, span) in raw.nodes.iter().zip(raw.spans.iter()) {
            let range = Range {
                line: raw.line_no,
                start: span.start,
                end: span.end,
            };

            if let Some(kind) = classify(node) {
                analysis.tokens.push(SemToken {
                    range: range.clone(),
                    kind,
                    detail: describe(node),
                });
            }

            match node {
                // 実際に解決できず注記化されたものだけ診断（ルビ併用などの偽陽性を除く）。
                Node::UnresolvedReference { raw: original, .. } if failed.contains(original) => {
                    analysis.diagnostics.push(Diagnostic {
                        range,
                        severity: Severity::Warning,
                        code: "unresolved-reference",
                        message: format!("注記の対象を前方に見つけられません: {original}"),
                    });
                }
                // 面区点にも U+ にも解決できない外字（画像にも文字にもならない）。
                Node::Gaiji {
                    description,
                    unicode: None,
                    jis_code: None,
                    ..
                } => {
                    analysis.diagnostics.push(Diagnostic {
                        range,
                        severity: Severity::Warning,
                        code: "unresolved-gaiji",
                        message: format!(
                            "外字を文字・画像に解決できません（面区点/U+ 指定なし）: {description}"
                        ),
                    });
                }
                _ => {}
            }
        }

        extract_symbols(raw, &mut analysis.symbols);
    }

    // 構造診断＋折りたたみ範囲: lower を通して Block 木を得る（convert には無影響）。
    let (blocks, lower_diags) = lower_to_blocks_with_diagnostics(&doc);
    collect_folds(&blocks, &mut analysis.folds);
    for d in lower_diags {
        let end = doc
            .lines
            .get(d.line)
            .map(|l| l.source.chars().count())
            .unwrap_or(0);
        analysis.diagnostics.push(Diagnostic {
            range: Range {
                line: d.line,
                start: 0,
                end,
            },
            severity: Severity::Warning,
            code: "unclosed-block",
            message: format!(
                "{}が閉じられていません（対応する「終わり」がありません）",
                block_kind_label(&d.kind)
            ),
        });
    }

    analysis
}

/// Block 木から折りたたみ範囲を集める（複数行の Nested ブロックのみ）。
fn collect_folds(blocks: &[Block], out: &mut Vec<FoldRange>) {
    for b in blocks {
        if let Block::Nested { line, children, .. } = b {
            let end = block_max_line(b);
            if end > *line {
                out.push(FoldRange {
                    start_line: *line,
                    end_line: end,
                });
            }
            collect_folds(children, out);
        }
    }
}

/// ブロックの部分木に現れる最大の行番号。
fn block_max_line(b: &Block) -> usize {
    match b {
        Block::Line { line, .. } | Block::LineWrap { line, .. } => *line,
        Block::Nested { line, children, .. } => children
            .iter()
            .map(block_max_line)
            .max()
            .unwrap_or(*line)
            .max(*line),
    }
}

/// ブロック種別の表示名（診断メッセージ用）。
fn block_kind_label(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Jisage { .. } => "字下げ",
        BlockKind::Chitsuki { .. } => "地付き",
        BlockKind::Jizume { .. } => "字詰め",
        BlockKind::Burasage { .. } => "ぶら下げ",
        BlockKind::Midashi { .. } => "見出し",
        BlockKind::Keigakomi => "罫囲み",
        BlockKind::Yokogumi => "横組み",
        BlockKind::Caption => "キャプション",
        BlockKind::FontSize { .. } => "文字サイズ変更",
        BlockKind::Futoji => "太字",
        BlockKind::Shatai => "斜体",
    }
}

/// 行から見出しを抽出する。見出しには二形式ある:
/// - インライン `Node::Midashi { level, children }`（同行見出しなど）
/// - ブロック `BlockStart{Midashi}` … 本文Text … `BlockEnd{Midashi}`
fn extract_symbols(raw: &RawLine, out: &mut Vec<Symbol>) {
    let mut i = 0;
    while i < raw.nodes.len() {
        match &raw.nodes[i] {
            Node::Midashi {
                level, children, ..
            } => {
                out.push(Symbol {
                    range: span_range(raw, i, i),
                    level: level_number(*level),
                    text: text_of(children),
                });
            }
            // 後置形の見出し `対象［＃「対象」は大見出し］`（未解決の生ノード）。
            // 対象テキストを見出し名にする。
            Node::UnresolvedReference {
                spec: RefSpec::Midashi { level, .. },
                target,
                ..
            } => {
                out.push(Symbol {
                    range: span_range(raw, i, i),
                    level: level_number(*level),
                    text: target.clone(),
                });
            }
            Node::BlockStart {
                block_type: BlockType::Midashi,
                params,
            } => {
                let level = params.level.map(level_number).unwrap_or(0);
                // 対応する BlockEnd(Midashi) まで本文テキストを集める。
                let mut text = String::new();
                let mut j = i + 1;
                while j < raw.nodes.len() {
                    if matches!(
                        &raw.nodes[j],
                        Node::BlockEnd {
                            block_type: BlockType::Midashi,
                            ..
                        }
                    ) {
                        break;
                    }
                    text.push_str(&raw.nodes[j].to_text());
                    j += 1;
                }
                let end = j.min(raw.nodes.len().saturating_sub(1));
                out.push(Symbol {
                    range: span_range(raw, i, end),
                    level,
                    text,
                });
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
}

/// `raw` の `from..=to` 番目のノードを覆う範囲を作る。
fn span_range(raw: &RawLine, from: usize, to: usize) -> Range {
    Range {
        line: raw.line_no,
        start: raw.spans[from].start,
        end: raw.spans[to].end,
    }
}

/// ノードをセマンティックトークン種別に分類する。`Text` はハイライト不要なので `None`。
fn classify(node: &Node) -> Option<SemTokenKind> {
    match node {
        Node::Text(_) => None,
        Node::Ruby { .. } => Some(SemTokenKind::Ruby),
        Node::Midashi { .. } => Some(SemTokenKind::Heading),
        Node::Style { .. } => Some(SemTokenKind::Emphasis),
        Node::Gaiji { .. } => Some(SemTokenKind::Gaiji),
        Node::Accent { .. } => Some(SemTokenKind::Accent),
        Node::Img { .. } => Some(SemTokenKind::Image),
        // 後置形の参照（未解決の生ノード）: 見出し・強調は種別色にする。
        // 例 `序章［＃「序章」は大見出し］` は Node::Midashi ではなく UnresolvedReference。
        Node::UnresolvedReference {
            spec: RefSpec::Midashi { .. },
            ..
        } => Some(SemTokenKind::Heading),
        Node::UnresolvedReference {
            spec: RefSpec::Style(_),
            ..
        } => Some(SemTokenKind::Emphasis),
        // ブロック見出しの開始／終了マーカーも見出し色にする。
        Node::BlockStart {
            block_type: BlockType::Midashi,
            ..
        }
        | Node::BlockEnd {
            block_type: BlockType::Midashi,
            ..
        } => Some(SemTokenKind::Heading),
        // 構造・その他の注記系はまとめて Annotation。
        _ => Some(SemTokenKind::Annotation),
    }
}

/// 見出しレベルを数値化（1=大, 2=中, 3=小）。
fn level_number(level: MidashiLevel) -> u8 {
    match level {
        MidashiLevel::O => 1,
        MidashiLevel::Naka => 2,
        MidashiLevel::Ko => 3,
    }
}

/// 見出しレベルの表示名（大／中／小）。
fn level_label(level: MidashiLevel) -> &'static str {
    match level {
        MidashiLevel::O => "大",
        MidashiLevel::Naka => "中",
        MidashiLevel::Ko => "小",
    }
}

/// ホバー用の説明文を作る。値の分かるもの（外字の実文字・ルビ読み等）だけ返す。
fn describe(node: &Node) -> Option<String> {
    match node {
        Node::Ruby { ruby, .. } => Some(format!("ルビ: {}", text_of(ruby))),
        Node::Gaiji {
            description,
            unicode,
            jis_code,
            ..
        } => {
            let mut s = format!("外字: {description}");
            if let Some(u) = unicode {
                s.push_str(&format!(" → {u}"));
            } else if let Some(j) = jis_code {
                s.push_str(&format!("（{j}）"));
            }
            Some(s)
        }
        Node::Accent { unicode, name, .. } => Some(match unicode {
            Some(u) => format!("アクセント: {u}（{name}）"),
            None => format!("アクセント: {name}"),
        }),
        Node::Midashi { level, .. } => Some(format!("{}見出し", level_label(*level))),
        Node::Img { filename, .. } => Some(format!("画像: {filename}")),
        Node::BlockStart {
            block_type: BlockType::Midashi,
            params,
        } => Some(format!(
            "{}見出し（開始）",
            params.level.map(level_label).unwrap_or("")
        )),
        Node::BlockEnd {
            block_type: BlockType::Midashi,
            ..
        } => Some("見出し（終わり）".to_string()),
        _ => None,
    }
}

/// 子ノード列の表示テキストを連結する。
fn text_of(children: &[Node]) -> String {
    children.iter().map(|n| n.to_text()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_becomes_semantic_token_with_char_span() {
        let a = analyze("東京《とうきょう》");
        // base「東京」は Text、ルビノードは `《とうきょう》`=[2,9) を覆う。
        let ruby: Vec<_> = a
            .tokens
            .iter()
            .filter(|t| t.kind == SemTokenKind::Ruby)
            .collect();
        assert_eq!(ruby.len(), 1);
        assert_eq!(ruby[0].range.line, 0);
        assert_eq!(ruby[0].range.start, 2);
        assert_eq!(ruby[0].range.end, 9);
    }

    #[test]
    fn heading_becomes_symbol_and_token() {
        let a = analyze("［＃大見出し］序章［＃大見出し終わり］");
        assert_eq!(a.symbols.len(), 1, "大見出しが 1 件シンボル化される");
        assert_eq!(a.symbols[0].level, 1);
        assert_eq!(a.symbols[0].text, "序章");
        assert!(a.tokens.iter().any(|t| t.kind == SemTokenKind::Heading));
    }

    #[test]
    fn unresolvable_annotation_becomes_diagnostic() {
        // 前方に対象が無い「傍点」注記は解決できず診断になる。
        let a = analyze("［＃「存在しない語」に傍点］");
        assert_eq!(a.diagnostics.len(), 1);
        assert_eq!(a.diagnostics[0].severity, Severity::Warning);
        assert_eq!(a.diagnostics[0].code, "unresolved-reference");
        assert_eq!(a.diagnostics[0].range.line, 0);
    }

    #[test]
    fn ruby_token_carries_reading_detail() {
        let a = analyze("東京《とうきょう》");
        let ruby = a
            .tokens
            .iter()
            .find(|t| t.kind == SemTokenKind::Ruby)
            .expect("ルビトークン");
        assert_eq!(ruby.detail.as_deref(), Some("ルビ: とうきょう"));
    }

    #[test]
    fn gaiji_token_carries_char_detail() {
        // U+25CB は ○ に解決される。
        let a = analyze("※［＃「丸印」、U+25CB］");
        let gaiji = a
            .tokens
            .iter()
            .find(|t| t.kind == SemTokenKind::Gaiji)
            .expect("外字トークン");
        let detail = gaiji.detail.as_deref().unwrap_or("");
        assert!(detail.starts_with("外字:"), "detail={detail}");
        assert!(detail.contains('○'), "実文字を含む: {detail}");
    }

    #[test]
    fn valid_annotation_is_not_flagged() {
        // 対象「序章」が直前にある正当な見出し注記は誤検知しない。
        let a = analyze("序章［＃「序章」は大見出し］");
        assert!(
            a.diagnostics
                .iter()
                .all(|d| d.code != "unresolved-reference"),
            "正当な注記を未解決扱いしない"
        );
    }

    #[test]
    fn postfix_heading_becomes_symbol_and_heading_token() {
        // 最も一般的な後置形見出し（Node::Midashi ではなく UnresolvedReference）。
        let a = analyze("タイトル\n著者\n\n序章［＃「序章」は大見出し］");
        let heading: Vec<_> = a.symbols.iter().filter(|s| s.text == "序章").collect();
        assert_eq!(heading.len(), 1, "後置形見出しがアウトラインに出る");
        assert_eq!(heading[0].level, 1);
        assert!(
            a.tokens.iter().any(|t| t.kind == SemTokenKind::Heading),
            "後置形見出しが見出し色になる"
        );
    }

    #[test]
    fn ruby_base_annotation_is_not_flagged() {
        // ルビ親文字を対象にした見出し（実際に解決する）は誤検知しない。
        let a = analyze("タイトル\n著者\n\n序章《じょしょう》［＃「序章」は大見出し］");
        assert!(
            a.diagnostics
                .iter()
                .all(|d| d.code != "unresolved-reference"),
            "ルビ親文字の見出しを未解決扱いしない: {:?}",
            a.diagnostics
        );
    }

    #[test]
    fn unresolvable_gaiji_becomes_diagnostic() {
        // 面区点も U+ も無い外字は文字・画像に解決できない。
        let a = analyze("※［＃「謎の字」］");
        let g: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "unresolved-gaiji")
            .collect();
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn resolvable_gaiji_is_not_flagged() {
        // U+ 指定つき外字は解決できるので診断しない。
        let a = analyze("※［＃「丸印」、U+25CB］");
        assert!(a.diagnostics.iter().all(|d| d.code != "unresolved-gaiji"));
    }

    #[test]
    fn unclosed_block_becomes_diagnostic() {
        // ［＃ここから字下げ］に対応する「終わり」が無い（ブロック命令は行頭）。
        let a = analyze("［＃ここから２字下げ］\n本文だけ続く");
        let unclosed: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "unclosed-block")
            .collect();
        assert_eq!(unclosed.len(), 1);
        assert_eq!(unclosed[0].range.line, 0);
        assert!(
            unclosed[0].message.contains("字下げ"),
            "{}",
            unclosed[0].message
        );
    }

    #[test]
    fn properly_closed_block_has_no_unclosed_diagnostic() {
        let a = analyze("［＃ここから２字下げ］\n本文\n［＃ここで字下げ終わり］");
        assert!(a.diagnostics.iter().all(|d| d.code != "unclosed-block"));
    }

    #[test]
    fn multiline_block_produces_fold_range() {
        // 3行の字下げブロック（0: 開始, 1: 本文, 2: 終わり）。
        let a = analyze("［＃ここから２字下げ］\n本文\n［＃ここで字下げ終わり］");
        assert_eq!(a.folds.len(), 1);
        assert_eq!(a.folds[0].start_line, 0);
        assert!(a.folds[0].end_line >= 1);
    }

    #[test]
    fn single_line_has_no_fold() {
        let a = analyze("ただの本文\nもう一行");
        assert!(a.folds.is_empty());
    }

    #[test]
    fn plain_text_yields_no_tokens_or_diagnostics() {
        let a = analyze("ただの本文です");
        assert!(a.tokens.is_empty());
        assert!(a.diagnostics.is_empty());
        assert!(a.symbols.is_empty());
    }

    #[test]
    fn line_numbers_are_zero_based_per_buffer_line() {
        let a = analyze("一行目\n東京《とうきょう》");
        let ruby = a
            .tokens
            .iter()
            .find(|t| t.kind == SemTokenKind::Ruby)
            .expect("2 行目のルビ");
        assert_eq!(ruby.range.line, 1, "0 起点で 2 行目 = 1");
    }
}
