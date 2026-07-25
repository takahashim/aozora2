import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'

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

// シンタックスハイライトのスタイル
const aozoraHighlightStyle = HighlightStyle.define([
  // ルビ《》 - 落ち着いた緑
  { tag: tags.string, color: '#6c9249' },
  // 注記［＃...］ - 落ち着いた青
  { tag: tags.keyword, color: '#5a7a9e' },
  // ルビ開始記号 ｜ - グレー
  { tag: tags.operator, color: '#8a8a8a' },
  // 外字記号 ※ - オレンジ
  { tag: tags.atom, color: '#b8860b' },
])

export const aozoraHighlighting = syntaxHighlighting(aozoraHighlightStyle)
