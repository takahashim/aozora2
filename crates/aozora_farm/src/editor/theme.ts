import { EditorView } from '@codemirror/view'

// エディタの基本テーマ
export const aozoraEditorTheme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--editor-bg)',
    height: '100%',
  },
  '.cm-content': {
    fontFamily: '"Hiragino Mincho ProN", "Noto Serif CJK JP", serif',
    fontSize: '1rem',
    lineHeight: 'var(--editor-line-height)',
    padding: '16px',
  },
  '.cm-scroller': {
    overflow: 'auto',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--bg-secondary)',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--bg-secondary)',
  },
  '.cm-activeLine': {
    backgroundColor: 'rgba(108, 146, 73, 0.08)',
  },
  '&.cm-focused .cm-cursor': {
    borderLeftColor: 'var(--accent)',
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
    backgroundColor: 'rgba(108, 146, 73, 0.2)',
  },
  '.cm-searchMatch': {
    backgroundColor: '#ffe066',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: '#ff9500',
  },
})

// 記法のハイライトは lsp.ts のセマンティックトークン（Decoration.mark）に一本化した。
// 旧 StreamLanguage（aozora-lang.ts）＋ HighlightStyle は撤去。
