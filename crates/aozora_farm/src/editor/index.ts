import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightActiveLine, dropCursor, KeyBinding } from '@codemirror/view'
import { EditorState, Extension, Compartment } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { search, highlightSelectionMatches, openSearchPanel, closeSearchPanel, findNext, findPrevious, selectNextOccurrence } from '@codemirror/search'
import { bracketMatching } from '@codemirror/language'
import { aozoraEditorTheme } from './theme'
import { getCodeMirrorPhrases, onLangChange } from '@/i18n'
import { aozoraKeymap } from './keymap'
import { aozoraToolbar } from './toolbar'
import { aozoraPalette } from './palette'
import { aozoraLsp, setLiveAnalysis, analyzeNow } from './lsp'
import { charsetHighlight } from './charset'

export { setLiveAnalysis, analyzeNow }
import { aozoraCompletion } from './completion'

export type ChangeCallback = (content: string) => void

let editorView: EditorView | null = null
let changeCallback: ChangeCallback | null = null

// Compartment for dynamic language/phrases reconfiguration
const phrasesCompartment = new Compartment()

function getPhrasesExtension(): Extension {
  return EditorState.phrases.of(getCodeMirrorPhrases())
}

// Custom search commands
function toggleSearch(view: EditorView): boolean {
  const searchPanel = view.dom.querySelector('.cm-search')

  if (!searchPanel) {
    // Panel not open - open with search only
    view.dom.classList.remove('show-replace')
    openSearchPanel(view)
  } else if (view.dom.classList.contains('show-replace')) {
    // Replace is showing - hide replace
    view.dom.classList.remove('show-replace')
  } else {
    // Search only is showing - show replace
    view.dom.classList.add('show-replace')
  }
  return true
}

// Custom keymap for search
const customSearchKeymap: KeyBinding[] = [
  { key: "Mod-f", run: toggleSearch, scope: "editor search-panel" },
  { key: "F3", run: findNext, shift: findPrevious, scope: "editor search-panel", preventDefault: true },
  { key: "Mod-g", run: findNext, shift: findPrevious, scope: "editor search-panel", preventDefault: true },
  { key: "Escape", run: closeSearchPanel, scope: "editor search-panel" },
  { key: "Mod-d", run: selectNextOccurrence, preventDefault: true },
]

const baseExtensions: Extension[] = [
  lineNumbers(),
  EditorView.lineWrapping,
  highlightActiveLineGutter(),
  highlightActiveLine(),
  dropCursor(),
  history(),
  bracketMatching(),
  highlightSelectionMatches(),
  search({ top: true }),
  keymap.of([
    ...aozoraKeymap,
    ...customSearchKeymap,
    ...defaultKeymap,
    ...historyKeymap,
  ]),
  aozoraToolbar,
  aozoraPalette,
  aozoraEditorTheme,
  aozoraCompletion,
  ...aozoraLsp(),
  charsetHighlight,
]

export function createEditor(parent: HTMLElement, onChange?: ChangeCallback): EditorView {
  changeCallback = onChange || null

  const updateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged && changeCallback) {
      changeCallback(update.state.doc.toString())
    }
  })

  editorView = new EditorView({
    state: EditorState.create({
      doc: '',
      extensions: [
        ...baseExtensions,
        phrasesCompartment.of(getPhrasesExtension()),
        updateListener,
      ],
    }),
    parent,
  })

  // Listen for language changes and reconfigure phrases
  onLangChange(() => {
    if (editorView) {
      editorView.dispatch({
        effects: phrasesCompartment.reconfigure(getPhrasesExtension())
      })
    }
  })

  return editorView
}

export function getEditor(): EditorView | null {
  return editorView
}

export function getContent(): string {
  return editorView?.state.doc.toString() || ''
}

export function setContent(content: string): void {
  if (!editorView) return

  editorView.dispatch({
    changes: {
      from: 0,
      to: editorView.state.doc.length,
      insert: content,
    },
  })
}

export function focus(): void {
  editorView?.focus()
}

export function openSearch(): void {
  if (!editorView) return
  editorView.dom.classList.remove('show-replace')
  openSearchPanel(editorView)
}

export function openReplace(): void {
  if (!editorView) return
  editorView.dom.classList.add('show-replace')
  openSearchPanel(editorView)
}
