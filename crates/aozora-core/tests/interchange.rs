//! 文書まるごとの交換形式（`aozora-document`）の往復。
//!
//! 「テキスト → JSON → HTML」が「テキスト → HTML」と**バイト一致**することを
//! 縛る。一致しなければ、交換形式が木の外の情報を取りこぼしている。

#![cfg(feature = "serde")]

use aozora_core::html::{convert, RenderOptions};
use aozora_core::interchange::{aozora_document, raw_document, Document};
use aozora_core::parser::RawDoc;

/// 記法・フッタの材料を一通り含む文書（本文＋底本情報）。
const SOURCE: &str = concat!(
    "作品名\r\n著者名\r\n\r\n",
    "東京《とうきょう》へ\r\n",
    "本文［＃「本文」に傍点］とわざ／＼\r\n",
    "刺※［＃「卓＋戈」、U+39B8］と〔e'〕cole\r\n",
    "［＃ここから２字下げ］\r\n中身\r\n［＃ここで字下げ終わり］\r\n",
    "\r\n底本：「甲」乙\r\n入力：丙\r\n"
);

/// `［＃本文終わり］` がある文書。以降は底本行も含めて after_text に入る（参照と同じ）。
const SOURCE_AFTER_TEXT: &str = concat!(
    "作品名\r\n著者名\r\n\r\n本文\r\n",
    "［＃本文終わり］\r\nあとがき\r\n底本：「甲」乙\r\n"
);

fn expected(source: &str) -> String {
    convert(source, &RenderOptions::default())
}

#[test]
fn aozora_document_round_trips_through_json() {
    for source in [SOURCE, SOURCE_AFTER_TEXT] {
        let doc = aozora_document(source);
        let json = serde_json::to_string(&doc).expect("直列化できる");
        let back: Document<Vec<aozora_core::ast::Block>> =
            serde_json::from_str(&json).expect("読み戻せる");
        assert_eq!(back, doc, "JSON を往復しても同じ文書");
        assert_eq!(
            back.to_html(&RenderOptions::default()),
            expected(source),
            "Aozora AST の JSON から組み立てた HTML がテキストからのものと違う"
        );
    }
}

#[test]
fn raw_document_round_trips_through_json() {
    for source in [SOURCE, SOURCE_AFTER_TEXT] {
        let doc = raw_document(source);
        let json = serde_json::to_string(&doc).expect("直列化できる");
        let back: Document<RawDoc> = serde_json::from_str(&json).expect("読み戻せる");
        assert_eq!(back, doc, "JSON を往復しても同じ文書");
        assert_eq!(
            back.to_html(&RenderOptions::default()),
            expected(source),
            "RawAST の JSON から組み立てた HTML がテキストからのものと違う"
        );
    }
}

/// 木の外から持ち回る情報が本当に効いているか（取りこぼしたら落ちる）。
#[test]
fn document_carries_what_the_trees_do_not_model() {
    let doc = aozora_document(SOURCE);
    assert_eq!(doc.header.title.as_deref(), Some("作品名"), "題名");
    assert_eq!(doc.header.author.as_deref(), Some("著者名"), "著者");
    assert!(doc.kunoji.plain, "くの字点は生の行からしか分からない");
    assert!(doc.ends_with_newline, "末尾の改行");
    assert!(
        doc.after_text.is_empty(),
        "本文終わりが無ければ after_text は空"
    );
    assert!(!doc.bibliographical.is_empty(), "底本情報");

    // `［＃本文終わり］` があると、以降は底本行も含めて after_text に入る。
    let with_after = aozora_document(SOURCE_AFTER_TEXT);
    assert!(!with_after.after_text.is_empty(), "本文終わり後");
    assert!(
        with_after.bibliographical.is_empty(),
        "底本情報の節は作られない"
    );
}
