// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aozora_core::analysis::{analyze as analyze_document, Analysis};
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
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            convert_to_html,
            convert_file_to_html,
            analyze,
            get_resource_paths,
        ]);

    // メニューは macOS のみ。macOS は Edit メニュー（コピー/切り取り/貼り付け/全選択/取り消し）
    // が無いと Cmd+C/V/X が webview に届かない。Windows/Linux は WebView が Ctrl+C/V/X を
    // 直接処理するのでメニュー不要（付けると不要なメニューバーが出て不自然）。
    #[cfg(target_os = "macos")]
    let builder = builder.menu(|handle| tauri::menu::Menu::default(handle));

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
