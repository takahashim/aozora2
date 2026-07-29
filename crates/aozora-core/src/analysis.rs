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
use crate::document::{classify_lines, LineSection};
use crate::lower::lower_to_blocks_with_diagnostics;
use crate::node::{BlockType, MidashiLevel, Node, NodeKind, RefSpec};
use crate::parser::reference_resolver::resolve_references_collecting_failures;
use crate::parser::{parse_document_raw, RawLine};
use crate::token::Span;
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
    // `convert_editor` と同じく単独 `\r` も改行として扱う（エディタのバッファが
    // どの改行でも、行番号が変換側とずれないようにする）。`str::lines` は
    // 単独 `\r` を改行と見なさないので、先に均してから分割する。
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    // 末尾の改行で生じる空行は行として数えない（`str::lines` と同じ挙動）。
    if normalized.ends_with('\n') {
        lines.pop();
    }
    let doc = parse_document_raw(&lines);
    // 行番号はバッファのまま保ちつつ、その行で記法が効くかをセクションで判定する。
    // ヘッダは参照実装がルビ `《》` と `｜` を剥がして生文字列として出し、注記セクション
    // （罫線で囲まれた凡例）は出力に一切現れない。どちらもトークン・診断を出すと
    // 「出力に反映されない記法」をエディタが色付けし、誤った警告まで出てしまう
    // （凡例の `（例）小径《こみち》` がルビとして光る等）。
    let sections = classify_lines(&lines);

    let mut analysis = Analysis::default();

    for (raw, section) in doc.lines.iter().zip(&sections) {
        if !section.applies_notation() {
            // ヘッダに記法を書いても出力に出ないことは分かりにくい（書いた本人には
            // 効いているように見える）ので、1行1件だけ理由を知らせる。
            // 注記セクションは凡例に `《》：ルビ` 等が必ず出てくる定型文なので出さない
            // （出すと全ての青空文庫ファイルで恒常的なノイズになる）。
            if *section == LineSection::Header && raw.nodes.iter().any(|n| classify(n).is_some()) {
                analysis.diagnostics.push(Diagnostic {
                    range: Range {
                        line: raw.line_no,
                        start: 0,
                        end: raw.source.chars().count(),
                    },
                    severity: Severity::Info,
                    code: "notation-in-header",
                    message: "ヘッダ（作品名・著者など）の記法は出力に反映されません（ルビ《》と｜は取り除かれます）".to_string(),
                });
            }
            continue;
        }
        // 前方参照はこの行のなかで解決される（ルビ親文字も含む）。生ノードでは未解決
        // なので、実際の解決を **1 行 1 回**走らせて、解決に失敗した参照の**位置**を
        // 得ておく。個々の参照を再解決する二次コストを避けるため。
        //
        // 識別子が注記文字列ではなく位置なのは、同じ注記が1行に2回あるとき
        // （`［＃「あ」は太字］あ［＃「あ」は太字］` は前者だけ失敗する）に
        // 文字列では区別できず、成功した方にも診断が出てしまうため。
        //
        // 前方参照が1つも無い行が大半なので、その場合は複製も解決も走らせない。
        let failed: HashSet<Span> = if raw
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::UnresolvedReference { .. }))
        {
            let mut nodes = raw.nodes.clone();
            resolve_references_collecting_failures(&mut nodes)
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };

        // 各生ノードがchar位置範囲を自前で持つ。
        for node in &raw.nodes {
            let span = &node.span;
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

            match &node.kind {
                // 実際に解決できず注記化されたものだけ診断（ルビ併用などの偽陽性を除く）。
                NodeKind::UnresolvedReference { raw: original, .. }
                    if failed.contains(&node.span) =>
                {
                    analysis.diagnostics.push(Diagnostic {
                        range,
                        severity: Severity::Warning,
                        code: "unresolved-reference",
                        message: format!("注記の対象を前方に見つけられません: {original}"),
                    });
                }
                // 面区点にも U+ にも解決できない外字（画像にも文字にもならない）。
                NodeKind::Gaiji {
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

        // 未閉じアクセント（対応する 〕 が同一行に無く、行末まで延長した 〔…）。
        // 参照実装は複数行アクセントの1行目として受理する（変換は byte 一致のまま）。
        // 現状は互換優先で許容 → Warning。将来 複数行アクセントを禁止する厳格モードは、
        // この検出（安定コード `unclosed-accent`）を弾く根拠に再利用できる。
        for span in &raw.unclosed_accents {
            analysis.diagnostics.push(Diagnostic {
                range: Range {
                    line: raw.line_no,
                    start: span.start,
                    end: span.end,
                },
                severity: Severity::Warning,
                code: "unclosed-accent",
                message:
                    "アクセント〔…〕が同一行で閉じられていません（行末まで延長。複数行アクセントは将来非対応予定）"
                        .to_string(),
            });
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
        BlockKind::Burasage(_) => "ぶら下げ",
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
/// - インライン `NodeKind::Midashi { level, children }`（同行見出しなど）
/// - ブロック `BlockStart{Midashi}` … 本文Text … `BlockEnd{Midashi}`
fn extract_symbols(raw: &RawLine, out: &mut Vec<Symbol>) {
    let mut i = 0;
    while i < raw.nodes.len() {
        match &raw.nodes[i].kind {
            NodeKind::Midashi {
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
            NodeKind::UnresolvedReference {
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
            NodeKind::BlockStart {
                block_type: BlockType::Midashi,
                params,
            } => {
                let level = params.level.map(level_number).unwrap_or(0);
                // 対応する BlockEnd(Midashi) まで本文テキストを集める。
                let mut text = String::new();
                let mut j = i + 1;
                while j < raw.nodes.len() {
                    if matches!(
                        &raw.nodes[j].kind,
                        NodeKind::BlockEnd {
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
        start: raw.nodes[from].span.start,
        end: raw.nodes[to].span.end,
    }
}

/// ノードをセマンティックトークン種別に分類する。`Text` はハイライト不要なので `None`。
///
/// **`_` の catch-all を置かないこと。** 既定の `Annotation` は「注記色にする」という
/// 積極的な主張なので、variant を足すとコンパイラが黙って誤った色を選ぶ
/// （実際に濁点付き片仮名を実装したとき、外字なのに注記色のまま漏れていた）。
fn classify(node: &Node) -> Option<SemTokenKind> {
    match &node.kind {
        NodeKind::Text(_) => None,
        NodeKind::Ruby { .. } => Some(SemTokenKind::Ruby),
        NodeKind::Midashi { .. } => Some(SemTokenKind::Heading),
        NodeKind::Style { .. } => Some(SemTokenKind::Emphasis),
        NodeKind::FontSize { .. } => Some(SemTokenKind::Emphasis),
        NodeKind::Gaiji { .. } => Some(SemTokenKind::Gaiji),
        NodeKind::Accent { .. } => Some(SemTokenKind::Accent),
        NodeKind::DakutenKatakana { .. } => Some(SemTokenKind::Gaiji),
        NodeKind::Img { .. } => Some(SemTokenKind::Image),
        // 後置形の参照（未解決の生ノード）は、適用先の種別で色を決める。
        // 例 `序章［＃「序章」は大見出し］` は NodeKind::Midashi ではなく UnresolvedReference。
        NodeKind::UnresolvedReference { spec, .. } => Some(classify_ref_spec(spec)),
        // ブロック見出しの開始／終了マーカーも見出し色にする。
        NodeKind::BlockStart {
            block_type: BlockType::Midashi,
            ..
        }
        | NodeKind::BlockEnd {
            block_type: BlockType::Midashi,
            ..
        } => Some(SemTokenKind::Heading),
        // 構造・その他の注記系。
        NodeKind::BlockStart { .. }
        | NodeKind::BlockEnd { .. }
        | NodeKind::Note(_)
        | NodeKind::LineJisage { .. }
        | NodeKind::AnnotationEnd { .. }
        | NodeKind::Kaeriten(_)
        | NodeKind::Okurigana(_)
        | NodeKind::Tcy { .. }
        | NodeKind::Keigakomi { .. }
        | NodeKind::Yokogumi { .. }
        | NodeKind::Caption { .. } => Some(SemTokenKind::Annotation),
    }
}

/// 後置形の参照が適用する指定を、トークン種別に対応づける。
/// ここも `_` を置かないこと（[`RefSpec`] を足すと色が黙って注記色になる）。
fn classify_ref_spec(spec: &RefSpec) -> SemTokenKind {
    match spec {
        RefSpec::Midashi { .. } => SemTokenKind::Heading,
        RefSpec::Style(_) | RefSpec::FontSize { .. } => SemTokenKind::Emphasis,
        // ルビとして表示されるもの（注記ルビ・傍記）。
        RefSpec::AnnotationRuby { .. } | RefSpec::SideNote { .. } => SemTokenKind::Ruby,
        // 外字画像になるもの（句点コード指定・濁点付き片仮名）。
        RefSpec::EmbeddedGaiji { .. } | RefSpec::DakutenKatakana { .. } => SemTokenKind::Gaiji,
        // 縦中横・罫囲み・横組み・キャプション・返り点・訓点送り仮名。
        RefSpec::Inline(_) => SemTokenKind::Annotation,
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
    match &node.kind {
        NodeKind::Ruby { ruby, .. } => Some(format!("ルビ: {}", text_of(ruby))),
        NodeKind::Gaiji {
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
        NodeKind::Accent { unicode, name, .. } => Some(match unicode {
            Some(u) => format!("アクセント: {u}（{name}）"),
            None => format!("アクセント: {name}"),
        }),
        NodeKind::Midashi { level, .. } => Some(format!("{}見出し", level_label(*level))),
        NodeKind::Img { filename, .. } => Some(format!("画像: {filename}")),
        NodeKind::DakutenKatakana { num } => Some(format!(
            "濁点付き片仮名: {}（1-07-8{num}）",
            Node::dakuten_katakana_char(num)
        )),
        NodeKind::UnresolvedReference {
            spec: RefSpec::DakutenKatakana { num },
            ..
        } => Some(format!(
            "濁点付き片仮名: {}（1-07-8{num}）",
            Node::dakuten_katakana_char(num)
        )),
        NodeKind::BlockStart {
            block_type: BlockType::Midashi,
            params,
        } => Some(format!(
            "{}見出し（開始）",
            params.level.map(level_label).unwrap_or("")
        )),
        NodeKind::BlockEnd {
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

    /// 本文だけを与えたいテスト用に、最小のヘッダ（作品名・著者・空行）を前置する。
    /// ヘッダ行では記法が効かない（参照実装がルビを剥がして生出力する）ので、
    /// 記法の解析を試すには本文セクションに置く必要がある。本文は 3 行目から。
    const HEAD: &str = "作品名\n著者\n\n";
    /// 本文の 0 起点行番号（[`HEAD`] を前置したとき）。
    const BODY0: usize = 3;

    fn analyze_body(body: &str) -> Analysis {
        analyze(&format!("{HEAD}{body}"))
    }

    #[test]
    fn ruby_becomes_semantic_token_with_char_span() {
        let a = analyze_body("東京《とうきょう》");
        // base「東京」は Text、ルビノードは `《とうきょう》`=[2,9) を覆う。
        let ruby: Vec<_> = a
            .tokens
            .iter()
            .filter(|t| t.kind == SemTokenKind::Ruby)
            .collect();
        assert_eq!(ruby.len(), 1);
        assert_eq!(ruby[0].range.line, BODY0);
        assert_eq!(ruby[0].range.start, 2);
        assert_eq!(ruby[0].range.end, 9);
    }

    #[test]
    fn heading_becomes_symbol_and_token() {
        let a = analyze_body("［＃大見出し］序章［＃大見出し終わり］");
        assert_eq!(a.symbols.len(), 1, "大見出しが 1 件シンボル化される");
        assert_eq!(a.symbols[0].level, 1);
        assert_eq!(a.symbols[0].text, "序章");
        assert!(a.tokens.iter().any(|t| t.kind == SemTokenKind::Heading));
    }

    #[test]
    fn unresolvable_annotation_becomes_diagnostic() {
        // 前方に対象が無い「傍点」注記は解決できず診断になる。
        let a = analyze_body("［＃「存在しない語」に傍点］");
        assert_eq!(a.diagnostics.len(), 1);
        assert_eq!(a.diagnostics[0].severity, Severity::Warning);
        assert_eq!(a.diagnostics[0].code, "unresolved-reference");
        assert_eq!(a.diagnostics[0].range.line, BODY0);
    }

    #[test]
    fn ruby_token_carries_reading_detail() {
        let a = analyze_body("東京《とうきょう》");
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
        let a = analyze_body("※［＃「丸印」、U+25CB］");
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
        let a = analyze_body("序章［＃「序章」は大見出し］");
        assert!(
            a.diagnostics
                .iter()
                .all(|d| d.code != "unresolved-reference"),
            "正当な注記を未解決扱いしない"
        );
    }

    #[test]
    fn postfix_heading_becomes_symbol_and_heading_token() {
        // 最も一般的な後置形見出し（NodeKind::Midashi ではなく UnresolvedReference）。
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
        let a = analyze_body("※［＃「謎の字」］");
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
        let a = analyze_body("※［＃「丸印」、U+25CB］");
        assert!(a.diagnostics.iter().all(|d| d.code != "unresolved-gaiji"));
    }

    #[test]
    fn unclosed_block_becomes_diagnostic() {
        // ［＃ここから字下げ］に対応する「終わり」が無い（ブロック命令は行頭）。
        let a = analyze_body("［＃ここから２字下げ］\n本文だけ続く");
        let unclosed: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "unclosed-block")
            .collect();
        assert_eq!(unclosed.len(), 1);
        assert_eq!(unclosed[0].range.line, BODY0);
        assert!(
            unclosed[0].message.contains("字下げ"),
            "{}",
            unclosed[0].message
        );
    }

    #[test]
    fn properly_closed_block_has_no_unclosed_diagnostic() {
        let a = analyze_body("［＃ここから２字下げ］\n本文\n［＃ここで字下げ終わり］");
        assert!(a.diagnostics.iter().all(|d| d.code != "unclosed-block"));
    }

    #[test]
    fn unclosed_accent_becomes_diagnostic() {
        // 対応する 〕 が同一行に無く行末まで延長した 〔…（複数行アクセントの1行目・4363相当）。
        // 変換は互換のため受理するが、検証用に Warning を出す。
        let a = analyze_body("〔Pardonnez a` mon");
        let acc: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "unclosed-accent")
            .collect();
        assert_eq!(acc.len(), 1);
        assert_eq!(acc[0].range.line, BODY0);
        assert_eq!(acc[0].severity, Severity::Warning);
    }

    #[test]
    fn closed_accent_has_no_unclosed_diagnostic() {
        // 同一行で 〕 まで閉じているアクセントは診断しない。
        let a = analyze_body("〔Cafe'《カフエ》〕");
        assert!(a.diagnostics.iter().all(|d| d.code != "unclosed-accent"));
    }

    #[test]
    fn nested_unclosed_bracket_is_not_flagged() {
        // 入れ子の未閉じ 〔 はリテラル（アクセントにならない）ので診断も出ない。
        // 〔訳者注 〔Beethoven e`〕: 外側だけアクセント、内側 〔 は本文（54931相当）。
        let a = analyze_body("〔訳者注 〔Beethoven e`〕");
        assert!(a.diagnostics.iter().all(|d| d.code != "unclosed-accent"));
    }

    #[test]
    fn multiline_block_produces_fold_range() {
        // 3行の字下げブロック（0: 開始, 1: 本文, 2: 終わり）。
        let a = analyze_body("［＃ここから２字下げ］\n本文\n［＃ここで字下げ終わり］");
        assert_eq!(a.folds.len(), 1);
        assert_eq!(a.folds[0].start_line, BODY0);
        assert!(a.folds[0].end_line >= 1);
    }

    #[test]
    fn single_line_has_no_fold() {
        let a = analyze_body("ただの本文\nもう一行");
        assert!(a.folds.is_empty());
    }

    #[test]
    fn plain_text_yields_no_tokens_or_diagnostics() {
        let a = analyze_body("ただの本文です");
        assert!(a.tokens.is_empty());
        assert!(a.diagnostics.is_empty());
        assert!(a.symbols.is_empty());
    }

    #[test]
    fn line_numbers_are_zero_based_per_buffer_line() {
        let a = analyze_body("一行目\n東京《とうきょう》");
        let ruby = a
            .tokens
            .iter()
            .find(|t| t.kind == SemTokenKind::Ruby)
            .expect("2 行目のルビ");
        assert_eq!(ruby.range.line, BODY0 + 1, "0 起点でバッファ行と一致する");
    }

    /// 解決に失敗した参照の識別は**位置**で行う。同じ注記が1行に2回あると、
    /// 文字列をキーにしていた頃は成功した方にも診断が出ていた。
    ///
    /// `［＃「あ」は太字］あ［＃「あ」は太字］`
    /// → 1つ目は前方に対象が無く失敗、2つ目は「あ」を消費して成功。
    #[test]
    fn duplicate_reference_reports_only_the_failing_one() {
        let a = analyze_body("［＃「あ」は太字］あ［＃「あ」は太字］");
        let d: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "unresolved-reference")
            .collect();
        assert_eq!(d.len(), 1, "失敗した1件だけが診断される: {d:?}");
        // 失敗するのは行頭の方（[0, 9)）。
        assert_eq!(d[0].range.start, 0);
    }

    /// 濁点付き片仮名は外字画像になるので、注記色ではなく外字色にする。
    /// `RefSpec` を足したとき色が黙って注記色に落ちないよう classify は網羅マッチ。
    #[test]
    fn dakuten_katakana_is_a_gaiji_token_with_detail() {
        let a = analyze_body("ワ゛［＃1-7-82］");
        let t = a
            .tokens
            .iter()
            .find(|t| t.kind == SemTokenKind::Gaiji)
            .unwrap_or_else(|| panic!("外字トークンが無い: {:?}", a.tokens));
        assert_eq!(t.detail.as_deref(), Some("濁点付き片仮名: ワ゛（1-07-82）"));
    }

    /// 単独 `\r` 改行でも `convert_editor` と同じ行分割になる
    /// （`str::lines` は単独 `\r` を改行と見なさない）。
    #[test]
    fn lone_cr_is_treated_as_a_line_break() {
        let a = analyze("タイトル\r著者\r\r序章［＃「序章」は大見出し］");
        assert_eq!(a.symbols.len(), 1, "{:?}", a.symbols);
        assert_eq!(a.symbols[0].range.line, 3);
        // 末尾の改行が余分な行を作らないことも固定する。
        assert_eq!(analyze("あ\n").symbols.len(), 0);
    }

    /// ヘッダと注記セクションでは記法が効かない。
    ///
    /// - ヘッダ: 参照実装 `parse_header` はルビ `《》` と `｜` を**剥がして**から
    ///   項目に割り当てる（タイトルの `《》` はルビにならない）。
    /// - 注記セクション（罫線で囲まれた凡例）: 出力に一切現れない。
    ///
    /// どちらもトークンを出すと「出力に反映されない記法」が光り、診断まで出てしまう。
    #[test]
    fn header_and_chuuki_section_produce_no_tokens() {
        let src = concat!(
            "作品名\n",
            "著者《ちょしゃ》\n",
            "\n",
            "-------------------------------------------------------\n",
            "【テキスト中に現れる記号について】\n",
            "\n",
            "《》：ルビ\n",
            "（例）小径《こみち》\n",
            "（例）※［＃「魚＋師のつくり」、第4水準2-93-37］\n",
            "-------------------------------------------------------\n",
            "\n",
            "本文の東京《とうきょう》です。\n",
            "\n",
            "底本：「テスト」\n",
        );
        let a = analyze(src);
        // 光るのは本文行（11 行目）のルビ 1 件だけ。
        assert_eq!(a.tokens.len(), 1, "{:?}", a.tokens);
        assert_eq!(a.tokens[0].kind, SemTokenKind::Ruby);
        assert_eq!(a.tokens[0].range.line, 11);
        // 注記セクションの凡例には診断を出さない（全ファイル共通の定型文なので
        // 出すと恒常的なノイズになる）。
        assert!(
            a.diagnostics.iter().all(|d| d.range.line < 3),
            "注記セクションに診断が出ている: {:?}",
            a.diagnostics
        );
    }

    /// ヘッダの記法は黙って無視するのではなく、理由を Info で知らせる
    /// （書いた本人には効いているように見えるため）。
    #[test]
    fn notation_in_header_is_explained() {
        let a = analyze("作品名《さくひんめい》\n著者\n\n本文\n");
        let info: Vec<_> = a
            .diagnostics
            .iter()
            .filter(|d| d.code == "notation-in-header")
            .collect();
        assert_eq!(info.len(), 1, "{:?}", a.diagnostics);
        assert_eq!(info[0].range.line, 0);
        assert_eq!(info[0].severity, Severity::Info);
        // 記法の無いヘッダ行には出さない。
        assert!(analyze("作品名\n著者\n\n本文\n").diagnostics.is_empty());
    }
}
