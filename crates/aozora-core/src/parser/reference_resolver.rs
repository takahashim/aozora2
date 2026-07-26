//! 前方参照解決
//!
//! 青空文庫形式の「〇〇」に傍点 のようなパターンを解決します。
//! これらのコマンドは前方のテキストを参照し、装飾を適用します。

use crate::node::{BlockType, Node, RefSpec, RubyDirection};
use crate::parser::ruby_parser::extract_ruby_base_from_nodes;
use crate::tokenizer::tokenize;

/// ノード列の前方参照を解決
///
/// ルビの親文字抽出と、「〇〇」に傍点 形式の装飾コマンドを解決します。
pub fn resolve_references(nodes: &mut Vec<Node>) {
    // 1. ルビの親文字を解決
    resolve_ruby_bases(nodes);

    // 2. 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
    resolve_annotation_ranges(nodes);

    // 3. 装飾の前方参照を解決
    resolve_style_references(nodes);
}

/// 行内でのルビ親文字解決
///
/// 「漢字《かんじ》」形式のルビの親文字を解決します。
/// 外字ノードも漢字として親文字に含めます。
pub fn resolve_inline_ruby(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Ruby {
            children,
            ruby,
            direction,
            ..
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() && i > 0 {
                let ruby_clone = ruby.clone();
                let direction_clone = *direction;

                // 直前のノード列から親文字を抽出（外字も含む）
                let preceding_nodes: Vec<Node> = nodes[..i].to_vec();
                if let Some((remaining, base)) = extract_ruby_base_from_nodes(&preceding_nodes) {
                    // 残りのノード数を計算
                    let nodes_to_remove = preceding_nodes.len() - remaining.len();

                    // 前半を残りのノードで置き換え
                    let start_idx = i - nodes_to_remove;
                    nodes.splice(start_idx..i, std::iter::empty());

                    // 新しいインデックスを計算
                    let new_i = start_idx;

                    // 前半部分を挿入
                    nodes.splice(..new_i, remaining.into_iter());

                    // Rubyノードを更新（インデックスが変わっているので再計算）
                    let ruby_idx = nodes
                        .iter()
                        .position(|n| matches!(n, Node::Ruby { children: c, .. } if c.is_empty()));

                    if let Some(idx) = ruby_idx {
                        nodes[idx] = Node::Ruby {
                            children: base,
                            ruby: ruby_clone,
                            direction: direction_clone,
                            keep_gaiji_notes_in_base: false,
                        };
                    }
                    continue; // iを増やさない（ノードを操作したので）
                }
            }
        }
        i += 1;
    }
}

/// ルビの親文字を解決
fn resolve_ruby_bases(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 親文字が空のRubyノードを探す
        if let Node::Ruby {
            children,
            ruby,
            direction: _,
            ..
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() {
                // 直前のノードから親文字を抽出
                if i > 0 {
                    let preceding_nodes: Vec<Node> = nodes[..i].to_vec();
                    if let Some((remaining, base)) = extract_ruby_base_from_nodes(&preceding_nodes)
                    {
                        // 直前のノードを更新
                        let to_remove = i - (preceding_nodes.len() - remaining.len());

                        // 残りのノードで前半を置き換え
                        nodes.splice(..i, remaining.into_iter());

                        // 新しいインデックスを計算
                        let new_i = nodes.len() - (nodes.len() - to_remove);

                        // Rubyノードを更新
                        if let Some(Node::Ruby { children: c, .. }) = nodes.get_mut(new_i) {
                            *c = base;
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
///
/// `［＃注記付き］内容［＃「注記」の注記付き終わり］` を `<ruby><rb>内容</rb><rt>注記</rt></ruby>` に変換
fn resolve_annotation_ranges(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 注記付き範囲の開始を探す
        if let Node::BlockStart { block_type, .. } = &nodes[i] {
            if *block_type == BlockType::AnnotationRange
                || *block_type == BlockType::LeftAnnotationRange
            {
                let is_left = *block_type == BlockType::LeftAnnotationRange;

                // 対応する終了を探す
                let mut end_idx = None;
                let mut annotation = None;
                for j in (i + 1)..nodes.len() {
                    if let Node::BlockEnd {
                        block_type: bt,
                        params,
                        ..
                    } = &nodes[j]
                    {
                        if (*bt == BlockType::AnnotationRange && !is_left)
                            || (*bt == BlockType::LeftAnnotationRange && is_left)
                        {
                            end_idx = Some(j);
                            annotation = params.annotation.clone();
                            break;
                        }
                    }
                }

                if let (Some(end_idx), Some(annotation)) = (end_idx, annotation) {
                    // 開始から終了までの間のノードを収集
                    let children: Vec<Node> = nodes[(i + 1)..end_idx].to_vec();
                    // 注記テキストをパース（外字を含む場合があるため）
                    let annotation_nodes = parse_annotation_text(&annotation);

                    if is_left {
                        // 左注記の場合は注記として出力（Ruby版と同様）
                        // 開始マーカー + 内容ノード + 終了マーカー（外字を含む）
                        let mut new_nodes = Vec::new();
                        new_nodes.push(Node::Note("左に注記付き".to_string()));
                        new_nodes.extend(children);
                        // 終了マーカーは外字を含む可能性があるのでAnnotationEndノードを使用
                        new_nodes.push(Node::AnnotationEnd {
                            prefix: "左に「".to_string(),
                            content: annotation_nodes,
                            suffix: "」の注記付き終わり".to_string(),
                        });

                        // 範囲を新しいノード列で置き換え
                        nodes.splice(i..=end_idx, new_nodes.into_iter());
                    } else {
                        // 通常の注記付きはRubyとして出力
                        let new_node = Node::Ruby {
                            children,
                            ruby: annotation_nodes,
                            direction: RubyDirection::Right,
                            keep_gaiji_notes_in_base: true,
                        };
                        // 範囲を新しいノードで置き換え
                        nodes.splice(i..=end_idx, std::iter::once(new_node));
                    }
                    // iを増やさない（置き換えたので次のノードは同じインデックス）
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// 装飾の前方参照を解決
/// 前方参照を解決しつつ、解決できず注記化した参照の `raw` 文字列を返す。
/// ノード変換の結果は [`resolve_references`] と完全に同一で、失敗リストを追加返却するだけ。
/// エディタ支援 `analysis` が「実際に解決できなかった参照」を **1 行 1 回**の解決で
/// 知るために使う（参照ごとに resolve をやり直す二次的コストを避ける）。
pub fn resolve_references_collecting_failures(nodes: &mut Vec<Node>) -> Vec<String> {
    resolve_ruby_bases(nodes);
    resolve_annotation_ranges(nodes);
    let mut failed = Vec::new();
    resolve_style_references_collecting(nodes, &mut failed);
    failed
}

fn resolve_style_references(nodes: &mut Vec<Node>) {
    resolve_style_references_collecting(nodes, &mut Vec::new());
}

/// [`resolve_style_references`] と同じ解決を行い、解決できず注記化したトップレベル参照の
/// `raw` を `failed` に集める（子ノード内の失敗は集めない）。
fn resolve_style_references_collecting(nodes: &mut Vec<Node>, failed: &mut Vec<String>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::UnresolvedReference { target, spec, raw } = &nodes[i] {
            let target_clone = target.clone();
            let spec_clone = spec.clone();
            let raw_clone = raw.clone();

            // 参照実装 search_front_reference を移植: 直前ノード（バッファ末尾）
            // から連続した接尾辞スパンとして対象を探す。i == 0 なら前が無い。
            if i > 0 {
                if let Some(m) = search_front_reference(&nodes[..i], i - 1, &target_clone) {
                    apply_front_reference(nodes, &mut i, m, &spec_clone);
                    continue;
                }
            }

            // 解決できなかった場合は、もとの文字列のまま注記にする
            failed.push(raw_clone.clone());
            nodes[i] = Node::Note(raw_clone);
        }
        i += 1;
    }

    // 子ノードの中の前方参照も解決する。参照実装は ｜瀕［＃「瀕」は太字］《ひん》
    // のようにルビ親文字の内側で前方参照を解決してから（@ruby_buf を対象に）
    // ルビを閉じる。こちらはルビ親文字解決後、親文字ノード列の中に前方参照が
    // 残るので、各コンテナの子へ再帰して同じスコープ内で解決する。
    for node in nodes.iter_mut() {
        resolve_style_references_in_children(node);
    }
}

/// コンテナノードの子ノード列に対して前方参照解決を再帰的に適用する。
fn resolve_style_references_in_children(node: &mut Node) {
    match node {
        Node::Ruby { children, ruby, .. } => {
            resolve_style_references(children);
            resolve_style_references(ruby);
        }
        Node::Style { children, .. }
        | Node::FontSize { children, .. }
        | Node::Tcy { children }
        | Node::Keigakomi { children }
        | Node::Yokogumi { children }
        | Node::Caption { children }
        | Node::Midashi { children, .. } => {
            resolve_style_references(children);
        }
        Node::Warichu { upper, lower } => {
            resolve_style_references(upper);
            resolve_style_references(lower);
        }
        _ => {}
    }
}

/// 前方参照の照合結果。start_idx..=（注記の直前）を子ノードに置き換える。
struct FrontRefMatch {
    /// 消費する最古ノードの位置
    start_idx: usize,
    /// 最古ノードが Text 分割だったときにバッファへ残す前半（それ以外は空）
    prefix: String,
    /// ラップ対象のノード列（古い順）
    children: Vec<Node>,
}

/// 対象ノードが参照実装 ReferenceMentioned 相当か（スパンの一要素になれるか）。
/// 対応: ルビ・装飾（Decorate: 傍点/傍線/太字/斜体/上下付き 等）・文字サイズ・
/// 縦中横（Dir）・罫囲み・横組み・キャプション・見出し。画像/外字/アクセント/
/// 訓点（送り仮名・返り点）はスパン不可（参照実装で false になる）。
fn is_reference_mentioned(node: &Node) -> bool {
    matches!(
        node,
        Node::Ruby { .. }
            | Node::Style { .. }
            | Node::FontSize { .. }
            | Node::Tcy { .. }
            | Node::Keigakomi { .. }
            | Node::Yokogumi { .. }
            | Node::Caption { .. }
            | Node::Midashi { .. }
    )
}

/// 参照実装 search_front_reference の移植。
///
/// バッファ末尾（end_idx）から連続した要素を消費して、対象 `target` を接尾辞
/// スパンとして照合する。String 要素は末尾一致なら分割（前半を残す）、対象が
/// その要素で終わるなら残りを前方へ再帰。ReferenceMentioned 要素は内部テキストが
/// 対象末尾に一致するなら要素まるごとを子に取り込む。空文字列要素は読み飛ばす。
/// それ以外（画像・外字など）に当たった時点で照合失敗。
fn search_front_reference(nodes: &[Node], end_idx: usize, target: &str) -> Option<FrontRefMatch> {
    if target.is_empty() {
        return None;
    }
    match &nodes[end_idx] {
        Node::Text(s) => {
            if s.is_empty() {
                // 空文字列は捨てて同じ対象で1つ前へ。
                return end_idx
                    .checked_sub(1)
                    .and_then(|e| search_front_reference(nodes, e, target));
            }
            // 完全一致: s が対象で終わる → s を分割し前半を残す。
            if let Some(prefix) = s.strip_suffix(target) {
                return Some(FrontRefMatch {
                    start_idx: end_idx,
                    prefix: prefix.to_string(),
                    children: vec![Node::text(target)],
                });
            }
            // 部分一致: 対象が s で終わる → s は対象の末尾セグメント。残りを再帰。
            if let Some(remaining) = target.strip_suffix(s.as_str()) {
                let e = end_idx.checked_sub(1)?;
                let mut sub = search_front_reference(nodes, e, remaining)?;
                sub.children.push(Node::text(s));
                return Some(sub);
            }
            None
        }
        node if is_reference_mentioned(node) => {
            let inner = extract_plain_text(node);
            if inner.is_empty() {
                return None;
            }
            if inner == target {
                return Some(FrontRefMatch {
                    start_idx: end_idx,
                    prefix: String::new(),
                    children: vec![node.clone()],
                });
            }
            if let Some(remaining) = target.strip_suffix(inner.as_str()) {
                let e = end_idx.checked_sub(1)?;
                let mut sub = search_front_reference(nodes, e, remaining)?;
                sub.children.push(node.clone());
                return Some(sub);
            }
            None
        }
        _ => None,
    }
}

/// 照合結果をノード列に適用する。start_idx..=（注記の直前）を
/// [分割前半?][解決済みノード] に置き換え、注記自身を除去する。
fn apply_front_reference(nodes: &mut Vec<Node>, i: &mut usize, m: FrontRefMatch, spec: &RefSpec) {
    let new_node = spec.resolve(m.children);
    let mut replacement = Vec::new();
    if !m.prefix.is_empty() {
        replacement.push(Node::text(&m.prefix));
    }
    replacement.push(new_node);
    let r = replacement.len();

    // 注記の直前（*i - 1）までがスパン。start_idx..=(*i-1) を置き換える。
    let end = *i - 1;
    nodes.splice(m.start_idx..=end, replacement);

    // 置換後、注記は start_idx + r の位置へ移動している。除去して、続きは
    // 注記の次のノードから（continue で再検査）。
    let annotation_idx = m.start_idx + r;
    if annotation_idx < nodes.len() {
        nodes.remove(annotation_idx);
    }
    *i = annotation_idx;
}

/// ノードからプレーンテキストを抽出
fn extract_plain_text(node: &Node) -> String {
    match node {
        Node::Text(text) => text.clone(),
        Node::Ruby { children, .. } => {
            // Rubyノードからは親文字のみ抽出
            children.iter().map(extract_plain_text).collect()
        }
        Node::Style { children, .. } => children.iter().map(extract_plain_text).collect(),
        Node::FontSize { children, .. } => children.iter().map(extract_plain_text).collect(),
        Node::Tcy { children } => children.iter().map(extract_plain_text).collect(),
        Node::Keigakomi { children } => children.iter().map(extract_plain_text).collect(),
        Node::Yokogumi { children } => children.iter().map(extract_plain_text).collect(),
        Node::Caption { children } => children.iter().map(extract_plain_text).collect(),
        Node::Midashi { children, .. } => children.iter().map(extract_plain_text).collect(),
        _ => String::new(),
    }
}

/// 注記テキストをノード列にパース
///
/// 外字表記（`※［＃...］`）を含むテキストをパースして、
/// テキストノードと外字ノードの列に変換します。
fn parse_annotation_text(text: &str) -> Vec<Node> {
    use crate::gaiji::{parse_gaiji, GaijiResult};
    use crate::token::TokenKind;

    let tokens = tokenize(text);
    let mut nodes = Vec::new();

    for token in tokens {
        match token.kind {
            TokenKind::Text(s) => nodes.push(Node::text(&s)),
            TokenKind::Gaiji {
                description,
                had_igeta,
            } => {
                let node = match parse_gaiji(&description) {
                    GaijiResult::Unicode(s) => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(s),
                        jis_code: None,
                        had_igeta,
                    },
                    GaijiResult::JisConverted { jis_code, unicode } => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(unicode),
                        jis_code: Some(jis_code),
                        had_igeta,
                    },
                    GaijiResult::JisImage { jis_code } => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: Some(jis_code),
                        had_igeta,
                    },
                    GaijiResult::Unconvertible => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: None,
                        had_igeta,
                    },
                };
                nodes.push(node);
            }
            // その他のトークンは無視（注記内にはルビやコマンドは含まれない想定）
            _ => {}
        }
    }

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{InlineKind, RubyDirection, StyleType};

    #[test]
    fn test_resolve_inline_ruby() {
        let mut nodes = vec![
            Node::text("私の東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0], Node::Text(s) if s == "私の"));
        if let Node::Ruby { children, ruby, .. } = &nodes[1] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_inline_ruby_full_match() {
        let mut nodes = vec![
            Node::text("東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 1);
        if let Node::Ruby { children, ruby, .. } = &nodes[0] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_style_reference() {
        // 参照実装は対象を直前バッファの接尾辞としてしか照合しないので、
        // 対象「重要」はテキストの末尾になければならない。
        let mut nodes = vec![
            Node::text("とても重要"),
            Node::UnresolvedReference {
                target: "重要".to_string(),
                spec: RefSpec::Style(StyleType::SesameDot),
                raw: "「重要」に傍点".to_string(),
            },
        ];

        resolve_style_references(&mut nodes);

        // 「重要」が装飾ノードになっているはず
        assert!(nodes.iter().any(|n| matches!(n, Node::Style { .. })));
    }

    #[test]
    fn test_multinode_resolution_does_not_skip_following_reference() {
        // 複数ノードにまたがる前方参照（見出し等）の解決で対象ノードが減った
        // あと、直後の未解決参照（縦中横）を読み飛ばさず解決すること。
        // 従来は MultiNodeExact 適用時に *i を更新せず、直後の参照を skip して
        // いた（同行中見出し＋日付の縦中横が注記化するバグ）。
        let mut nodes = vec![
            Node::text("前"),
            Node::text("半"),
            Node::UnresolvedReference {
                target: "前半".to_string(),
                spec: RefSpec::Style(StyleType::Bold),
                raw: "「前半」は太字".to_string(),
            },
            Node::text("４・19"),
            Node::UnresolvedReference {
                target: "19".to_string(),
                spec: RefSpec::Inline(InlineKind::Tcy),
                raw: "「19」は縦中横".to_string(),
            },
        ];
        resolve_style_references(&mut nodes);
        assert!(
            nodes.iter().any(|n| matches!(n, Node::Tcy { .. })),
            "見出し解決の直後の縦中横参照が解決されていない: {nodes:?}"
        );
        assert!(
            !nodes
                .iter()
                .any(|n| matches!(n, Node::UnresolvedReference { .. })),
            "未解決参照が残っている: {nodes:?}"
        );
    }

    #[test]
    fn test_resolve_frontref_inside_ruby_base() {
        // ｜瀕［＃「瀕」は太字］《ひん》: ルビ親文字の内側に前方参照がある場合、
        // 親文字ノード列の中で解決してから <rb> に入れる（子への再帰）。
        let mut nodes = vec![Node::Ruby {
            keep_gaiji_notes_in_base: false,
            children: vec![
                Node::text("瀕"),
                Node::UnresolvedReference {
                    target: "瀕".to_string(),
                    spec: RefSpec::Style(StyleType::Bold),
                    raw: "「瀕」は太字".to_string(),
                },
            ],
            ruby: vec![Node::text("ひん")],
            direction: RubyDirection::Right,
        }];
        resolve_style_references(&mut nodes);
        if let Node::Ruby { children, .. } = &nodes[0] {
            assert_eq!(
                children.len(),
                1,
                "親文字が装飾1ノードに畳まれていない: {children:?}"
            );
            assert!(
                matches!(
                    &children[0],
                    Node::Style {
                        style_type: StyleType::Bold,
                        ..
                    }
                ),
                "親文字内の前方参照が太字に解決されていない: {children:?}"
            );
        } else {
            panic!("Ruby node が壊れた");
        }
    }

    #[test]
    fn test_search_front_reference_tail_only() {
        // 末尾（接尾辞）としてのみ照合する。末尾でないノードは対象でも解決しない。
        let nodes = vec![
            Node::text("前の文"),
            Node::text("重要"),
            Node::text("後の文"),
        ];
        // 末尾 "後の文" は "重要" で終わらないので照合失敗。
        assert!(search_front_reference(&nodes, nodes.len() - 1, "重要").is_none());
        // 末尾が対象そのものなら分割（prefix 空）で解決。
        let m = search_front_reference(&nodes[..2], 1, "重要").unwrap();
        assert_eq!(m.start_idx, 1);
        assert_eq!(m.prefix, "");
        assert!(matches!(&m.children[..], [Node::Text(s)] if s == "重要"));
    }

    #[test]
    fn test_search_front_reference_suffix_split() {
        // 途中一致（これは[重要]なことだ）は解決しない＝注記のまま。
        assert!(search_front_reference(&[Node::text("これは重要なことだ")], 0, "重要").is_none());

        // 末尾一致は分割して前半を prefix に残す。
        let nodes = [Node::text("これは重要")];
        let m = search_front_reference(&nodes, 0, "重要").unwrap();
        assert_eq!(m.start_idx, 0);
        assert_eq!(m.prefix, "これは");
        assert!(matches!(&m.children[..], [Node::Text(s)] if s == "重要"));
    }

    #[test]
    fn test_search_front_reference_spans_reference_mentioned() {
        // 対象が「Text＋既解決の装飾ノード」にまたがるとき、装飾ノードを丸ごと
        // 子に取り込んでスパン解決する（例:「Ｘ１」で Ｘ＝Text、１＝下付き）。
        let subscript = Node::Style {
            children: vec![Node::text("１")],
            style_type: StyleType::Subscript,
        };
        let nodes = vec![Node::text("前Ｘ"), subscript];
        let m = search_front_reference(&nodes, 1, "Ｘ１").unwrap();
        assert_eq!(m.start_idx, 0);
        assert_eq!(m.prefix, "前");
        // 子は古い順: [Text("Ｘ"), Subscript(１)]
        assert_eq!(m.children.len(), 2);
        assert!(matches!(&m.children[0], Node::Text(s) if s == "Ｘ"));
        assert!(matches!(&m.children[1], Node::Style { .. }));
    }
}
