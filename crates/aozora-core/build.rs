//! ビルド時のデータ生成。
//!
//! `data/*.json` を読んで `OUT_DIR` に Rust ソース片を出力し、各モジュールが
//! `include!` で取り込む。入出力の定型処理（[`read_json_string_map`] /
//! [`write_generated`]）と、データ固有の変換とを分けてある。

use std::env;
use std::fs;
use std::path::Path;

/// ビルド時に読む入力データ。変更されたら再生成する。
const INPUTS: [&str; 4] = [
    "data/jis2ucs.json",
    "data/accent_table.json",
    "data/accent_names.json",
    "data/original_title_chars.json",
];

const JIS2UCS: &str = "data/jis2ucs.json";
const ORIGINAL_TITLE_CHARS: &str = "data/original_title_chars.json";

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // 入力は 1 ファイルから複数の生成物を作ることがあるので、依存はここで一度だけ宣言する。
    for input in INPUTS {
        println!("cargo:rerun-if-changed={input}");
    }

    // jis2ucs: 値は HTML 実体参照（&#xXXXX;）または直接文字。デコードして \u{XXXX}
    // へエスケープする。デコードできない値があればビルドを失敗させる（黙って落とすと
    // 外字が解決できなくなる原因を実行時まで隠してしまうため）。
    generate_string_map(&out_dir, "jis2ucs_table.rs", JIS2UCS, |s| {
        parse_html_entities(s)
            .map(|decoded| {
                decoded
                    .chars()
                    .map(|c| format!("\\u{{{:04X}}}", c as u32))
                    .collect()
            })
            .ok_or_else(|| "HTML 実体参照（&#xXXXX;）をデコードできません".to_string())
    });

    // accent: 「基底文字＋記号」→ JISコード。値はそのまま。
    generate_string_map(&out_dir, "accent_table.rs", "data/accent_table.json", |s| {
        Ok(s.to_string())
    });

    // accent の説明文: 参照実装 aozora2html の yml/accent_table.yml 由来（規則から
    // 組み立てられない表記＝ドイツ語エスツェット等を含む）。値はそのまま。
    generate_string_map(
        &out_dir,
        "accent_name_table.rs",
        "data/accent_names.json",
        |s| Ok(s.to_string()),
    );

    // JIS X 0208 の漢字ビットマップ（同じ jis2ucs.json から導出する 2 つ目の生成物）。
    generate_x0208_kanji_bitmap(&out_dir);

    // 原題（欧文表題）として扱う文字の一覧。
    generate_original_title_chars(&out_dir);
}

// ---------------------------------------------------------------------------
// 原題（欧文表題）文字の生成
// ---------------------------------------------------------------------------

/// 原題として扱う文字の一覧を `OUT_DIR/original_title_chars.rs` へ生成する。
///
/// 出典は `data/original_title_chars.json`。判定基準はそのデータ自身が持つ——
/// 参照実装は Shift_JIS のバイト範囲で判定するが、それを Unicode の一覧に書き出して
/// あるので、**Shift_JIS の対応表を持たない移植先でもこの表だけで再現できる**。
/// ここは読み出して昇順配列にするだけ。
///
/// `char` 列は人間が読むための添え物だが、`unicode` 列と食い違っていればビルドを
/// 止める（表が壊れたまま通らないように）。
fn generate_original_title_chars(out_dir: &str) {
    let json = fs::read_to_string(ORIGINAL_TITLE_CHARS)
        .unwrap_or_else(|_| panic!("{ORIGINAL_TITLE_CHARS} not found"));
    let table: serde_json::Value = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("{ORIGINAL_TITLE_CHARS}: JSON として読めません: {e}"));

    let mut codepoints = Vec::new();

    // ASCII は範囲で書かれている。
    let ascii = &table["ascii"];
    let from = parse_codepoint(ascii["from"].as_str().unwrap_or_else(|| {
        panic!("{ORIGINAL_TITLE_CHARS}: ascii.from がありません");
    }));
    let to = parse_codepoint(ascii["to"].as_str().unwrap_or_else(|| {
        panic!("{ORIGINAL_TITLE_CHARS}: ascii.to がありません");
    }));
    codepoints.extend(from..=to);

    // 区点範囲の宣言。各行の kuten がこの範囲に収まっているかを検証するために使う
    // （符号化器を持たずに表の自己整合性を確かめられる）。
    let ranges: Vec<((u32, u32, u32), (u32, u32, u32))> = table["kuten_ranges"]
        .as_array()
        .unwrap_or_else(|| panic!("{ORIGINAL_TITLE_CHARS}: kuten_ranges が配列ではありません"))
        .iter()
        .map(|range| {
            let bound = |key: &str| {
                let text = range[key].as_str().unwrap_or_else(|| {
                    panic!("{ORIGINAL_TITLE_CHARS}: kuten_ranges[].{key} がありません")
                });
                parse_menkuten(text).unwrap_or_else(|| {
                    panic!("{ORIGINAL_TITLE_CHARS}: 面区点の書式ではありません: {text}")
                })
            };
            (bound("from"), bound("to"))
        })
        .collect();

    // 残りは 1 文字ずつ列挙されている。
    let entries = table["chars"].as_array().unwrap_or_else(|| {
        panic!("{ORIGINAL_TITLE_CHARS}: chars が配列ではありません");
    });
    for entry in entries {
        let text = entry["unicode"].as_str().unwrap_or_else(|| {
            panic!("{ORIGINAL_TITLE_CHARS}: chars[].unicode がありません");
        });
        let cp = parse_codepoint(text);
        let c = char::from_u32(cp)
            .unwrap_or_else(|| panic!("{ORIGINAL_TITLE_CHARS}: {text} は有効な文字ではありません"));
        // char 列は人間向けだが、符号位置と食い違っていたらビルドを止める。
        if let Some(shown) = entry["char"].as_str() {
            assert_eq!(
                c.to_string(),
                shown,
                "{ORIGINAL_TITLE_CHARS}: {text} の char 列が符号位置と一致しません"
            );
        }
        // kuten 列も同様。null は Shift_JIS が 1 バイトで表す字（区点を持たない）。
        if let Some(kuten) = entry["kuten"].as_str() {
            let cell = parse_menkuten(kuten).unwrap_or_else(|| {
                panic!("{ORIGINAL_TITLE_CHARS}: 面区点の書式ではありません: {kuten}")
            });
            assert!(
                ranges.iter().any(|&(from, to)| (from..=to).contains(&cell)),
                "{ORIGINAL_TITLE_CHARS}: {text} の区点 {kuten} が kuten_ranges の外です"
            );
        }
        codepoints.push(cp);
    }

    codepoints.sort_unstable();
    codepoints.dedup();

    let mut code = String::new();
    code.push_str("// build.rs が data/original_title_chars.json から生成しています。\n");
    code.push_str("// 手で編集しないでください。昇順なので二分探索で引ける。\n");
    code.push_str(&format!(
        "static ORIGINAL_TITLE_CHARS: [u32; {}] = [\n",
        codepoints.len()
    ));
    for chunk in codepoints.chunks(8) {
        code.push_str("    ");
        for cp in chunk {
            code.push_str(&format!("{cp:#06x}, "));
        }
        code.push('\n');
    }
    code.push_str("];\n");
    write_generated(out_dir, "original_title_chars.rs", &code);
}

/// `"U+XXXX"` 形式を符号位置へ。
fn parse_codepoint(s: &str) -> u32 {
    let hex = s
        .strip_prefix("U+")
        .unwrap_or_else(|| panic!("符号位置の書式が U+XXXX ではありません: {s}"));
    u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("符号位置を読み取れません: {s}"))
}

// ---------------------------------------------------------------------------
// 入出力の共通処理
// ---------------------------------------------------------------------------

/// JSON（文字列 → 文字列のオブジェクト）を `(キー, 値)` の列として読む。
///
/// 並び順は `serde_json` の Map（`BTreeMap`）に従うので**決定的**＝生成物も安定する。
/// トップレベルがオブジェクトでない・値が文字列でない場合は panic してビルドを止める
/// （データ不正を黙って読み飛ばさない）。
fn read_json_string_map(in_file: &str) -> Vec<(String, String)> {
    let json = fs::read_to_string(in_file).unwrap_or_else(|_| panic!("{in_file} not found"));
    let table: serde_json::Value = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("{in_file}: JSON として読めません: {e}"));
    let serde_json::Value::Object(map) = table else {
        panic!("{in_file}: トップレベルがオブジェクトではありません");
    };
    map.into_iter()
        .map(|(key, value)| match value {
            serde_json::Value::String(s) => (key, s),
            other => panic!("{in_file}: キー {key:?} の値が文字列ではありません: {other}"),
        })
        .collect()
}

/// 生成した Rust ソース片を `OUT_DIR/out_file` へ書き出す。
fn write_generated(out_dir: &str, out_file: &str, code: &str) {
    fs::write(Path::new(out_dir).join(out_file), code)
        .unwrap_or_else(|e| panic!("{out_file} を書き出せません: {e}"));
}

// ---------------------------------------------------------------------------
// 文字列マップの生成
// ---------------------------------------------------------------------------

/// JSON から `{ let mut m = HashMap::new(); m.insert(k, v); … m }` 形式の Rust 片を
/// 生成する。各値は `transform` を通し、`Err` ならキー付きで **panic してビルドを失敗**
/// させる。生成片は `accent.rs` / `jis_table.rs` が `include!` で取り込む。
fn generate_string_map<F>(out_dir: &str, out_file: &str, in_file: &str, transform: F)
where
    F: Fn(&str) -> Result<String, String>,
{
    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");
    for (key, value) in read_json_string_map(in_file) {
        match transform(&value) {
            Ok(v) => code.push_str(&format!("    m.insert(\"{key}\", \"{v}\");\n")),
            Err(e) => panic!("{in_file}: キー {key:?} の値 {value:?} を変換できません: {e}"),
        }
    }
    code.push_str("    m\n}");
    write_generated(out_dir, out_file, &code);
}

// ---------------------------------------------------------------------------
// JIS X 0208 漢字ビットマップの生成
// ---------------------------------------------------------------------------

/// URO（CJK統合漢字）の範囲。JIS X 0208 の漢字はすべてこの中に収まる。
const URO_START: u32 = 0x4E00;
const URO_END: u32 = 0x9FFF;

/// JIS X 0208-1990/1997/2012 の漢字数（第1水準 2,965 ＋ 第2水準 3,390）。
const X0208_KANJI_COUNT: usize = 6355;

/// JIS X 0208 の漢字ビットマップを `OUT_DIR/x0208_kanji_bitmap.rs` へ生成する。
///
/// 漢字判定を Unicode レンジで書くと 4,031 本に散らばるが、区点空間なら 2 レンジで
/// 済む（[`is_x0208_kanji_cell`]）。それをビットマップへ畳んでおけば実行時は O(1) で
/// 引け、エンコーディング実装にも依存しない。
fn generate_x0208_kanji_bitmap(out_dir: &str) {
    let entries = read_json_string_map(JIS2UCS);
    let codepoints = x0208_kanji_codepoints(&entries);
    assert_eq!(
        codepoints.len(),
        X0208_KANJI_COUNT,
        "JIS X 0208 の漢字数が合いません（区点レンジの抽出条件を確認）"
    );
    let bits = to_bitmap(&codepoints, URO_START, URO_END);
    write_generated(
        out_dir,
        "x0208_kanji_bitmap.rs",
        &format_bitmap_items(&bits),
    );
}

/// 面区点エントリから **JIS X 0208 の漢字**の符号位置を集める（規格知識はここに閉じる）。
///
/// X 0213 にしか無いセルは選別で読み飛ばす（異常ではない）。それ以外の不整合
/// ——キーが面区点形式でない・値をデコードできない・単一文字でない・URO の外——は
/// panic してビルドを止める。取りこぼしは呼び出し側の件数 assert でも検出される。
fn x0208_kanji_codepoints(entries: &[(String, String)]) -> Vec<u32> {
    let mut out = Vec::new();
    for (key, value) in entries {
        let (men, ku, ten) = parse_menkuten(key)
            .unwrap_or_else(|| panic!("{JIS2UCS}: キー {key:?} が面区点形式ではありません"));
        if !is_x0208_kanji_cell(men, ku, ten) {
            continue; // X 0213 だけにあるセル
        }
        let decoded = parse_html_entities(value)
            .unwrap_or_else(|| panic!("{JIS2UCS}: {key} の値 {value:?} をデコードできません"));
        let mut chars = decoded.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            panic!("{JIS2UCS}: {key} が単一文字ではありません: {decoded:?}");
        };
        let cp = c as u32;
        assert!(
            (URO_START..=URO_END).contains(&cp),
            "{JIS2UCS}: {key} ({c}, U+{cp:04X}) が URO の外にあります"
        );
        out.push(cp);
    }
    out
}

/// `"1-16-01"` 形式のキーを (面, 区, 点) に分解する。形式が違えば `None`。
fn parse_menkuten(key: &str) -> Option<(u32, u32, u32)> {
    let mut it = key.split('-');
    let men = it.next()?.parse().ok()?;
    let ku = it.next()?.parse().ok()?;
    let ten = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((men, ku, ten))
}

/// その面区点が JIS X 0208 の漢字か（面1 の 16-01〜47-51 ＋ 48-01〜84-06）。
///
/// 区47 点52〜94 と 区84 点7〜94 は JIS X 0213 で追加された 131 字なので除外する。
fn is_x0208_kanji_cell(men: u32, ku: u32, ten: u32) -> bool {
    if men != 1 {
        return false;
    }
    match ku {
        16..=46 => true,
        47 => ten <= 51, // 第1水準の末尾。47-52 以降は X 0213 の追加分
        48..=83 => true,
        84 => ten <= 6, // 第2水準の末尾（84-05 凜, 84-06 熙 は X 0208-1990 で追加）
        _ => false,
    }
}

/// 符号位置の集合を `base` 起点のビットマップに畳む（1 バイト 8 符号位置・**LSB 先頭**）。
fn to_bitmap(codepoints: &[u32], base: u32, end: u32) -> Vec<u8> {
    let mut bits = vec![0u8; ((end - base + 1) / 8) as usize];
    for &cp in codepoints {
        let idx = (cp - base) as usize;
        bits[idx / 8] |= 1 << (idx % 8);
    }
    bits
}

/// ビットマップを、基点・上限・件数とセットで Rust の items として整形する。
///
/// 基点やビット順を消費側が再定義せずに済むよう、**規約ごと生成物に含める**のが要点。
/// 消費側（`src/jis_x0208.rs`）は `include!` して `BITS`/`BASE`/`END`/`COUNT` を使うだけ。
fn format_bitmap_items(bits: &[u8]) -> String {
    let mut code = String::new();
    code.push_str(
        "// build.rs が data/jis2ucs.json から生成しています。手で編集しないでください。\n",
    );
    code.push_str("//\n");
    code.push_str("// 規約: 添字 = 符号位置 - BASE、1 バイトにつき 8 符号位置、**LSB 先頭**。\n");
    code.push_str("//       含まれる ⇔ BITS[i / 8] & (1 << (i % 8)) != 0\n\n");
    code.push_str("/// ビットマップが対象とする符号位置の下限。\n");
    code.push_str(&format!("const BASE: u32 = {URO_START:#X};\n"));
    code.push_str("/// 上限（この符号位置を含む）。\n");
    code.push_str(&format!("const END: u32 = {URO_END:#X};\n"));
    code.push_str("/// 収録字数（JIS X 0208 第1水準＋第2水準）。生成表の検証にだけ使う。\n");
    code.push_str("#[allow(dead_code)] // テストからのみ参照する\n");
    code.push_str(&format!("const COUNT: usize = {X0208_KANJI_COUNT};\n"));
    code.push_str("/// 各符号位置が JIS X 0208 の漢字かを示すビット列。\n");
    code.push_str(&format!("static BITS: [u8; {}] = [\n", bits.len()));
    for chunk in bits.chunks(16) {
        code.push_str("    ");
        for b in chunk {
            code.push_str(&format!("{b:#04x}, "));
        }
        code.push('\n');
    }
    code.push_str("];\n");
    code
}

// ---------------------------------------------------------------------------
// 共通ユーティリティ
// ---------------------------------------------------------------------------

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
