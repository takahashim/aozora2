# aozora2-gui

GUI application for aozora2 - Aozora Bunko format converter.

## Features

- Drag and drop file conversion
- Real-time HTML preview
- Open/Save file dialogs
- Copy HTML to clipboard

## Development

### Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js](https://nodejs.org/) (v18+)
- [Tauri CLI](https://tauri.app/)

### Setup

```bash
cd crates/aozora2-gui
npm install
```

### Run in development mode

```bash
npm run tauri:dev
```

### Build for production

```bash
npm run tauri:build
```

The built application will be in `src-tauri/target/release/bundle/`.

## Architecture

```
aozora2-gui/
├── src/                 # Frontend (HTML/CSS/JS)
│   ├── index.html
│   ├── style.css
│   └── main.js
├── src-tauri/           # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       └── main.rs
└── package.json
```

## Future Plans

- [ ] Edit mode with syntax highlighting
- [ ] Multiple file tabs
- [ ] Settings panel (encoding, output options)
- [ ] Export to various formats (PDF, EPUB)
