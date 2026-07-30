// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aozora_core::analysis::{analyze as analyze_document, Analysis};
use aozora_core::encoding::{
    encode_shift_jis, normalize_for_shift_jis, CharsetPolicy, UnencodableChar,
};
use aozora_core::html::{convert_editor, RenderOptions};
use tauri::Manager;

/// Convert Aozora Bunko format text to HTML
///
/// エディタは LF 改行なので、CRLF 前提の convert ではなく convert_editor を使う
/// （LF を CRLF に正規化してから変換する。素の convert だと全文がタイトル化して
/// 本文が空になる）。
#[tauri::command]
fn convert_to_html(input: &str) -> Result<String, String> {
    // プレビューは外字を極力 Unicode 文字（数値文字参照）で出す。Unicode を持たない外字
    // だけ画像になるので、画像取得が激減してプレビューが軽くなる（オラクル用の convert は
    // 既定 use_unicode=false のまま＝aozora2html と画像でバイト一致）。
    let options = RenderOptions::default().with_unicode(true);
    let html = convert_editor(input, &options);
    Ok(html)
}

/// Convert file content to HTML
#[tauri::command]
fn convert_file_to_html(content: &str, filename: &str) -> Result<ConvertResult, String> {
    let html = convert_to_html(content)?;

    Ok(ConvertResult {
        html,
        filename: filename.to_string(),
    })
}

#[derive(serde::Serialize)]
struct ConvertResult {
    html: String,
    filename: String,
}

/// エディタ用の静的解析（セマンティックトークン／アウトライン／診断）を返す。
/// 位置は 0 起点（行・char）。フロント側で CodeMirror の位置に変換する。
#[tauri::command]
fn analyze(input: &str) -> Result<Analysis, String> {
    Ok(analyze_document(input))
}

/// Shift_JIS ＋ CRLF で保存する。本文テキストと変換後の HTML の両方で使う。
///
/// 青空文庫のファイルは Shift_JIS ＋ CRLF で、HTML も `<?xml encoding="Shift_JIS"?>` を
/// 宣言する。エディタの内容（UTF-8・LF）を両方そろえてから書き出す。符号位置の揺れ
/// （macOS の日本語入力が作る U+301C など）は [`normalize_for_shift_jis`] で寄せるが、
/// Shift_JIS に無い文字（絵文字・JIS 外の漢字）は外字注記 `※［＃…］` で書くべきものなので、
/// 置き換えず位置つきのエラーにする。
///
/// HTML は既に CRLF なので改行の正規化は素通しになる（べき等）。
#[tauri::command]
fn save_shift_jis(path: &str, content: &str) -> Result<(), SaveSjisError> {
    let crlf = content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");
    // 方針は既定（CP932）。半角カナや NEC/IBM 拡張を含む既存ファイルを開いて
    // 保存し直すだけで失敗しないようにする。青空文庫形式として直接書くべきでない
    // 文字は保存を止めるのではなく、エディタの診断（non-x0208-char）で知らせる。
    let bytes = encode_shift_jis(&normalize_for_shift_jis(&crlf), CharsetPolicy::default())
        .map_err(|chars| SaveSjisError::Unencodable { chars })?;
    std::fs::write(path, bytes).map_err(|e| SaveSjisError::Io {
        message: e.to_string(),
    })
}

/// [`save_shift_jis`] の失敗。フロントは kind で分岐する。
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SaveSjisError {
    /// Shift_JIS で符号化できない文字があった（保存していない）
    Unencodable { chars: Vec<UnencodableChar> },
    /// 書き出しに失敗した
    Io { message: String },
}

/// Get the path to bundled resources
#[tauri::command]
fn get_resource_paths(app_handle: tauri::AppHandle) -> Result<ResourcePaths, String> {
    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?;

    Ok(ResourcePaths {
        gaiji: resource_dir.join("gaiji").to_string_lossy().to_string(),
        css: resource_dir.join("css").to_string_lossy().to_string(),
    })
}

#[derive(serde::Serialize)]
struct ResourcePaths {
    gaiji: String,
    css: String,
}

fn main() {
    // ネイティブメニューはフロント（src/main.ts の setupMenu）が組んで setAsAppMenu で
    // 設定する。ここで menu を設定しても上書きされるだけなので置かない。macOS の
    // 起動直後（フロント初期化まで）は tauri の enable_macos_default_menu が
    // Menu::default を自動で入れる。
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            convert_to_html,
            convert_file_to_html,
            analyze,
            save_shift_jis,
            get_resource_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
