//! エディタ向け静的解析レイヤ（LSP 的機能の基盤）。
//!
//! パーサの**位置情報付き**生ノード（[`crate::parser::parse_document_raw`] が返す
//! [`crate::parser::RawLine`] の `nodes`／`spans`）から、エディタが必要とする三つの
//! 派生情報を組み立てる純粋関数を提供する:
//!
//! - [`Analysis::tokens`]  … セマンティックトークン（正確なハイライト／ホバーの土台）
//! - [`Analysis::symbols`] … アウトライン（見出しの一覧＝ドキュメントシンボル）
//! - [`Analysis::diagnostics`] … 診断（現状は解決できなかった注記＝誤記の指摘）
//!
//! すべて `convert()`（本文レンダリング）とは独立した**追加レイヤ**であり、オラクル
//! 出力には一切影響しない。位置は LSP と同じく **0 起点**（行・char とも）で、`end` は
//! 含まない半開区間。フロント（CodeMirror）側で行番号を +1 して用いる。

use crate::node::{BlockType, MidashiLevel, Node};
use crate::parser::{parse_document_raw, RawLine};

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
                });
            }

            // パーサが対象を解決できなかった注記＝誤記の可能性が高い。
            // 「解決できなかった」という事実そのものが診断源なので誤検知しない。
            if let Node::UnresolvedReference { raw: original, .. } = node {
                analysis.diagnostics.push(Diagnostic {
                    range,
                    severity: Severity::Warning,
                    code: "unresolved-reference",
                    message: format!("注記の対象を前方に見つけられません: {original}"),
                });
            }
        }

        extract_symbols(raw, &mut analysis.symbols);
    }

    analysis
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
