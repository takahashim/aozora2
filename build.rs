use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("jis2ucs_table.rs");

    // data/jis2ucs.json を読み込み
    let json = fs::read_to_string("data/jis2ucs.json").expect("data/jis2ucs.json not found");
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Rustのハッシュマップ初期化コードを生成
    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");

    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(s) = value {
                // HTML実体参照 "&#xXXXX;" をcharに変換
                if let Some(ch) = parse_html_entity(&s) {
                    code.push_str(&format!(
                        "    m.insert(\"{}\", '\\u{{{:04X}}}');\n",
                        key,
                        ch as u32
                    ));
                }
            }
        }
    }

    code.push_str("    m\n}");

    fs::write(&dest_path, code).unwrap();

    // ファイル変更時に再ビルド
    println!("cargo:rerun-if-changed=data/jis2ucs.json");
}

fn parse_html_entity(s: &str) -> Option<char> {
    // "&#xXXXX;" 形式
    if s.starts_with("&#x") && s.ends_with(';') {
        let hex = &s[3..s.len() - 1];
        u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
    } else {
        // 直接Unicode文字
        s.chars().next()
    }
}
