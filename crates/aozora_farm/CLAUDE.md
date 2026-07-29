# CLAUDE.md

青空文庫形式のプレビュー・編集 GUI（Tauri 2 ＋ TypeScript / CodeMirror 6）。

## 構成

- `src/` … フロント（TS）。`editor/`（CodeMirror）・`preview/`（iframe）・`i18n/`・`commands/`
- `src-tauri/` … Rust。`aozora-core` を呼ぶ Tauri コマンド（`convert_to_html` / `analyze` ほか）

本文の変換は `aozora_core::html::convert_editor` を使う。エディタは LF 改行なので、
CRLF 前提の `convert` に渡すと全文がタイトル化して本文が空になる。

## ビルドと起動

- **ワークスペースの `default-members` から外れている**（GUI は GTK/webkit のシステム
  ライブラリが要る）。素の `cargo build` / `cargo test` では**ビルドされない**ので、
  `cargo build -p aozora_farm` と明示する。
- **フロントの `dist/` はビルド時にバイナリへ埋め込まれる**（`generate_context!`）。
  フロントだけ直して `npm run build` しても、既存バイナリの中身は古いまま。
  バイナリを直接起動して確認するなら Rust 側の再ビルドまで必要:

  ```
  npm run build && touch src-tauri/src/main.rs && cargo build -p aozora_farm
  ```

- フロントを触る作業は `npm run tauri:dev` が確実（`devUrl` の Vite を見るので
  ホットリロードが効く）。
- テストは `npm run test:run`（vitest）、型は `npx tsc --noEmit`。

## ネイティブメニュー

- メニューは**フロント**の `src/main.ts` `setupMenu()` が組み、`setAsAppMenu()` で
  設定する。これは Rust 側や tauri 既定（`enable_macos_default_menu`）のメニューを
  **置き換える**ので、コピー・カット・ペースト・すべてを選択といった標準項目も
  ここに並べる必要がある。macOS では Cmd+C/V/X がメニュー項目のキーエクイバレント
  経由で webview に届くため、項目が無いとショートカット自体が効かなくなる。
- ラベルは i18n の `menu.*` キー。ネイティブメニューは後からラベルを差し替えられない
  ので、言語切替のたびに `setupMenu()` を呼び直して組み直す。
