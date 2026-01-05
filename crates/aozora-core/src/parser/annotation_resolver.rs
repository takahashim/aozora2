//! 注記範囲解決
//!
//! `［＃注記付き］内容［＃「注記」の注記付き終わり］` を
//! `<ruby><rb>内容</rb><rt>注記</rt></ruby>` に変換します。

use crate::gaiji::{parse_gaiji, GaijiResult};
use crate::node::{BlockType, Node, RubyDirection};
use crate::token::Token;
use crate::tokenizer::tokenize;

/// 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
pub fn resolve_annotation_ranges(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 注記付き範囲の開始を探す
        if let Node::BlockStart { block_type, .. } = &nodes[i] {
            if *block_type == BlockType::AnnotationRange
                || *block_type == BlockType::LeftAnnotationRange
            {
                let is_left = *block_type == BlockType::LeftAnnotationRange;

                // 対応する終了を探す
                let found = nodes[i + 1..]
                    .iter()
                    .enumerate()
                    .find_map(|(offset, node)| {
                        if let Node::BlockEnd {
                            block_type: bt,
                            params,
                        } = node
                        {
                            if (*bt == BlockType::AnnotationRange && !is_left)
                                || (*bt == BlockType::LeftAnnotationRange && is_left)
                            {
                                return Some((i + 1 + offset, params.annotation.clone()));
                            }
                        }
                        None
                    });

                if let Some((end_idx, Some(annotation))) = found {
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

/// 注記テキストをノード列にパース
///
/// 外字表記（`※［＃...］`）を含むテキストをパースして、
/// テキストノードと外字ノードの列に変換します。
fn parse_annotation_text(text: &str) -> Vec<Node> {
    let tokens = tokenize(text);
    let mut nodes = Vec::new();

    for token in tokens {
        match token {
            Token::Text(s) => nodes.push(Node::text(&s)),
            Token::Gaiji { description } => {
                let node = match parse_gaiji(&description) {
                    GaijiResult::Unicode(s) => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(s),
                        jis_code: None,
                    },
                    GaijiResult::JisConverted { jis_code, unicode } => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(unicode),
                        jis_code: Some(jis_code),
                    },
                    GaijiResult::JisImage { jis_code } => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: Some(jis_code),
                    },
                    GaijiResult::Unconvertible => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: None,
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
