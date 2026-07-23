use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // jis2ucs テーブル生成
    generate_jis2ucs_table(&out_dir);

    // accent テーブル生成
    generate_accent_table(&out_dir);

    // accent の説明文テーブル生成
    generate_accent_name_table(&out_dir);
}

fn generate_jis2ucs_table(out_dir: &str) {
    let dest_path = Path::new(out_dir).join("jis2ucs_table.rs");

    let json = fs::read_to_string("data/jis2ucs.json").expect("data/jis2ucs.json not found");
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");

    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(s) = value {
                if let Some(decoded) = parse_html_entities(&s) {
                    let escaped: String = decoded
                        .chars()
                        .map(|c| format!("\\u{{{:04X}}}", c as u32))
                        .collect();
                    code.push_str(&format!("    m.insert(\"{key}\", \"{escaped}\");\n"));
                }
            }
        }
    }

    code.push_str("    m\n}");
    fs::write(&dest_path, code).unwrap();
    println!("cargo:rerun-if-changed=data/jis2ucs.json");
}

fn generate_accent_table(out_dir: &str) {
    let dest_path = Path::new(out_dir).join("accent_table.rs");

    let json =
        fs::read_to_string("data/accent_table.json").expect("data/accent_table.json not found");
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");

    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(jis_code) = value {
                code.push_str(&format!("    m.insert(\"{key}\", \"{jis_code}\");\n"));
            }
        }
    }

    code.push_str("    m\n}");
    fs::write(&dest_path, code).unwrap();
    println!("cargo:rerun-if-changed=data/accent_table.json");
}

/// アクセント文字の説明文テーブルを生成する。
/// 参照実装 aozora2html の yml/accent_table.yml から取り込んだもので、
/// 規則から組み立てられない表記（ドイツ語エスツェット など）を含む。
fn generate_accent_name_table(out_dir: &str) {
    let dest_path = Path::new(out_dir).join("accent_name_table.rs");

    let json =
        fs::read_to_string("data/accent_names.json").expect("data/accent_names.json not found");
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");
    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(name) = value {
                code.push_str(&format!("    m.insert(\"{key}\", \"{name}\");\n"));
            }
        }
    }
    code.push_str("    m\n}");
    fs::write(&dest_path, code).unwrap();
    println!("cargo:rerun-if-changed=data/accent_names.json");
}

fn parse_html_entities(s: &str) -> Option<String> {
    let mut result = String::new();
    let mut remaining = s;

    while !remaining.is_empty() {
        if remaining.starts_with("&#x") {
            // &#xXXXX; 形式
            if let Some(end) = remaining.find(';') {
                let hex = &remaining[3..end];
                if let Ok(code) = u32::from_str_radix(hex, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        result.push(ch);
                        remaining = &remaining[end + 1..];
                        continue;
                    }
                }
            }
            return None; // パース失敗
        } else {
            // 直接Unicode文字
            if let Some(ch) = remaining.chars().next() {
                result.push(ch);
                remaining = &remaining[ch.len_utf8()..];
            } else {
                break;
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
