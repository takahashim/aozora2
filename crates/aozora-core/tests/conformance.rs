//! 交換形式（JSON）の適合フィクスチャ。
//!
//! `data/conformance/*.json` は「入力 → RawAST / Aozora AST」の対で、
//! docs/spec-rawast-json.md と docs/spec-aozora-ast-json.md が定める形式の**正規の例**。
//! 他言語の実装はこのファイルを読んで自分の出力と突き合わせられる。本実装はここで
//! 照合して drift を検出する。
//!
//! 更新するときは再生成し、差分を目視してからコミットする:
//!
//! ```text
//! UPDATE_CONFORMANCE=1 cargo test -p aozora-core --features serde --test conformance
//! ```

#![cfg(feature = "serde")]

use std::path::PathBuf;

use aozora_core::html::Quirks;
use aozora_core::interchange::RawDocument;
use aozora_core::lower::lower_to_blocks;
use aozora_core::parser::parse_document_raw;

fn conformance_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/conformance")
}

/// フィクスチャ1件。
#[derive(serde::Serialize, serde::Deserialize)]
struct Fixture {
    /// 何を示す例か
    note: String,
    /// 入力（本文の行。LF 区切り。ヘッダ・底本は含めない）
    source: String,
    /// RawAST（docs/spec-rawast-json.md）
    raw_ast: serde_json::Value,
    /// Aozora AST（docs/spec-aozora-ast-json.md）
    aozora_ast: serde_json::Value,
}

/// 入力から両 AST を作り、交換形式の JSON にする。
///
/// 交換形式はどちらも文書 1 本を表す器なので、`source` も文書の形にしてある
/// （題名・著者・空行のあとに本文）。LF 区切りで書き、ここで CRLF に直す。
fn build(source: &str) -> (serde_json::Value, serde_json::Value) {
    let text = source.replace('\n', "\r\n");
    let raw = RawDocument::from_text(&text);
    let aozora = raw.to_aozora(&Quirks::default());
    (
        serde_json::to_value(&raw).expect("直列化できる"),
        serde_json::to_value(&aozora).expect("直列化できる"),
    )
}

#[test]
fn fixtures_match_the_implementation() {
    let update = std::env::var("UPDATE_CONFORMANCE").is_ok();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(conformance_dir())
        .expect("data/conformance が読める")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "フィクスチャが 1 件も無い");

    for path in paths {
        let text = std::fs::read_to_string(&path).expect("読める");
        let mut fixture: Fixture = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: 形式が壊れている: {e}", path.display()));
        let (raw_ast, aozora_ast) = build(&fixture.source);

        if update {
            fixture.raw_ast = raw_ast;
            fixture.aozora_ast = aozora_ast;
            let mut json = serde_json::to_string_pretty(&fixture).expect("直列化できる");
            json.push('\n');
            std::fs::write(&path, json).expect("書ける");
        } else {
            assert_eq!(
                fixture.raw_ast,
                raw_ast,
                "{}: RawAST が仕様例と違う",
                path.display()
            );
            assert_eq!(
                fixture.aozora_ast,
                aozora_ast,
                "{}: Aozora AST が仕様例と違う",
                path.display()
            );
        }
    }
}

/// 交換形式は往復する（直列化したものを読み戻すと同じ木になる）。
/// 他言語の実装が JSON から木を組み立てられることの最低条件。
#[test]
fn json_round_trips() {
    let source =
        "東京《とうきょう》へ\n［＃ここから２字下げ］\n本文［＃「本文」に傍点］\n［＃ここで字下げ終わり］";
    let lines: Vec<&str> = source.split('\n').collect();
    let raw = parse_document_raw(&lines);
    let ast = lower_to_blocks(&raw);

    let raw_json = serde_json::to_string(&raw).expect("直列化できる");
    let raw_back: aozora_core::parser::RawDoc =
        serde_json::from_str(&raw_json).expect("読み戻せる");
    assert_eq!(serde_json::to_string(&raw_back).unwrap(), raw_json);

    let ast_json = serde_json::to_string(&ast).expect("直列化できる");
    let ast_back: aozora_core::AozoraAst = serde_json::from_str(&ast_json).expect("読み戻せる");
    assert_eq!(ast_back, ast);
}
