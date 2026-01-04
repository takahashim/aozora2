//! 前方参照解決
//!
//! 青空文庫形式の「〇〇」に傍点 のようなパターンを解決します。
//! これらのコマンドは前方のテキストを参照し、装飾を適用します。

use crate::node::Node;
use crate::parser::annotation_resolver::resolve_annotation_ranges;
use crate::parser::ruby_resolver::resolve_ruby_bases;
use crate::parser::style_resolver::resolve_style_references;

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
