# aozora_farm（青空ファーム）

GUI application for aozora2 - Aozora Bunko format previewer and editor.

## Features

- Editor with syntax highlighting, completion and diagnostics (CodeMirror 6)
- Real-time HTML preview
- Drag and drop file conversion
- Open/Save file dialogs (Shift_JIS / UTF-8 auto detection)
- Copy HTML to clipboard

## Development

### Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js](https://nodejs.org/) (v18+)

### Setup

```bash
cd crates/aozora_farm
npm install
```

### Run in development mode

```bash
npm run tauri:dev
```

### Test

```bash
npm run test:run   # vitest
npx tsc --noEmit   # type check
```

### Build for production

```bash
npm run tauri:build
```

The built application will be in `target/release/bundle/` at the workspace root
(this crate is part of the aozora2 cargo workspace, so build artifacts are shared).

Note: `cargo build` at the workspace root does **not** build this crate — it is
excluded from `default-members`. Use `cargo build -p aozora_farm`.

## Architecture

```
aozora_farm/
├── index.html
├── src/                 # Frontend (TypeScript)
│   ├── main.ts
│   ├── editor/          # CodeMirror 6 editor, completion, diagnostics
│   ├── preview/         # HTML preview (iframe)
│   ├── commands/        # Tauri command bridge
│   ├── i18n/            # ja / en
│   └── styles/
├── src-tauri/           # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       └── main.rs
└── package.json
```

See [CLAUDE.md](CLAUDE.md) for build pitfalls and native menu notes.

## Future Plans

- [ ] Multiple file tabs
- [ ] Settings panel (encoding, output options)
- [ ] Export to various formats (PDF, EPUB)
