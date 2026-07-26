import { EditorView } from '@codemirror/view'
import { EditorSelection } from '@codemirror/state'
import { undo, redo } from '@codemirror/commands'

export type AozoraCommand = (view: EditorView) => boolean

// Undo/Redo commands
export const undoCommand: AozoraCommand = (view) => undo(view)
export const redoCommand: AozoraCommand = (view) => redo(view)

// Helper: insert annotation after selected text (Aozora format: text［＃「text」に...］)
function insertAnnotationAfter(
  view: EditorView,
  annotationType: string,
  placeholder: string
): boolean {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    // No selection: insert template with placeholder
    const insertText = `${placeholder}［＃「${placeholder}」${annotationType}］`
    view.dispatch({
      changes: { from: selection.from, insert: insertText },
      selection: EditorSelection.single(selection.from, selection.from + placeholder.length)
    })
    return true
  }

  const selectedText = state.sliceDoc(selection.from, selection.to)
  const annotation = `［＃「${selectedText}」${annotationType}］`

  view.dispatch({
    changes: { from: selection.to, insert: annotation },
    selection: EditorSelection.cursor(selection.to + annotation.length)
  })
  return true
}

// Helper: wrap text with block annotations (Aozora format: ［＃type］text［＃type終わり］)
function insertBlockAnnotation(
  view: EditorView,
  annotationType: string,
  placeholder: string
): boolean {
  const { state } = view
  const selection = state.selection.main
  const startTag = `［＃${annotationType}］`
  const endTag = `［＃${annotationType}終わり］`

  if (selection.empty) {
    // No selection: insert template with placeholder
    const insertText = `${startTag}${placeholder}${endTag}`
    view.dispatch({
      changes: { from: selection.from, insert: insertText },
      selection: EditorSelection.single(selection.from + startTag.length, selection.from + startTag.length + placeholder.length)
    })
    return true
  }

  const selectedText = state.sliceDoc(selection.from, selection.to)
  const insertText = `${startTag}${selectedText}${endTag}`

  view.dispatch({
    changes: { from: selection.from, to: selection.to, insert: insertText },
    selection: EditorSelection.cursor(selection.from + insertText.length)
  })
  return true
}

// Helper: insert heading annotation (Aozora format: ［＃「text」は...］text)
function insertHeadingAnnotation(
  view: EditorView,
  headingType: string,
  placeholder: string
): boolean {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    // No selection: insert template
    const insertText = `［＃「${placeholder}」${headingType}］${placeholder}`
    const textStart = insertText.length - placeholder.length
    view.dispatch({
      changes: { from: selection.from, insert: insertText },
      selection: EditorSelection.single(selection.from + textStart, selection.from + textStart + placeholder.length)
    })
    return true
  }

  const selectedText = state.sliceDoc(selection.from, selection.to)
  const annotation = `［＃「${selectedText}」${headingType}］`

  view.dispatch({
    changes: { from: selection.from, insert: annotation },
    selection: EditorSelection.cursor(selection.from + annotation.length + selectedText.length)
  })
  return true
}

// Ruby: 漢字《かんじ》
export const insertRuby: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    // Insert template: 漢字《よみ》
    const template = '漢字《よみ》'
    view.dispatch({
      changes: { from: selection.from, insert: template },
      selection: EditorSelection.single(selection.from, selection.from + 2) // Select 漢字
    })
  } else {
    // Wrap selection with 《》 and position cursor for ruby input
    const selectedText = state.sliceDoc(selection.from, selection.to)
    const insertText = `${selectedText}《》`
    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert: insertText },
      selection: EditorSelection.cursor(selection.from + selectedText.length + 1) // Inside 《》
    })
  }
  return true
}

// Ruby with range marker: ｜東京《とうきょう》
export const insertRubyRange: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    const template = '｜漢字《よみ》'
    view.dispatch({
      changes: { from: selection.from, insert: template },
      selection: EditorSelection.single(selection.from + 1, selection.from + 3) // Select 漢字
    })
  } else {
    const selectedText = state.sliceDoc(selection.from, selection.to)
    const insertText = `｜${selectedText}《》`
    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert: insertText },
      selection: EditorSelection.cursor(selection.from + 1 + selectedText.length + 1) // Inside 《》
    })
  }
  return true
}

// Emphasis: ［＃傍点］text［＃傍点終わり］
export const insertEmphasis: AozoraCommand = (view) => {
  return insertBlockAnnotation(view, '傍点', 'テキスト')
}

// Double emphasis: text［＃「text」に二重傍点］
export const insertDoubleEmphasis: AozoraCommand = (view) => {
  return insertAnnotationAfter(view, 'に二重傍点', 'テキスト')
}

// Circle marks: text［＃「text」に圏点］
export const insertCircleMarks: AozoraCommand = (view) => {
  return insertAnnotationAfter(view, 'に圏点', 'テキスト')
}

// Side line: text［＃「text」に傍線］
export const insertSideLine: AozoraCommand = (view) => {
  return insertAnnotationAfter(view, 'に傍線', 'テキスト')
}

// Double side line: text［＃「text」に二重傍線］
export const insertDoubleSideLine: AozoraCommand = (view) => {
  return insertAnnotationAfter(view, 'に二重傍線', 'テキスト')
}

// Bold: ［＃太字］text［＃太字終わり］
export const insertBold: AozoraCommand = (view) => {
  return insertBlockAnnotation(view, '太字', 'テキスト')
}

// Italic: text［＃「text」に斜体］
export const insertItalic: AozoraCommand = (view) => {
  return insertAnnotationAfter(view, 'に斜体', 'テキスト')
}

// Large heading: ［＃「text」は大見出し］text
export const insertHeadingLarge: AozoraCommand = (view) => {
  return insertHeadingAnnotation(view, 'は大見出し', '見出し')
}

// Medium heading: ［＃「text」は中見出し］text
export const insertHeadingMedium: AozoraCommand = (view) => {
  return insertHeadingAnnotation(view, 'は中見出し', '見出し')
}

// Small heading: ［＃「text」は小見出し］text
export const insertHeadingSmall: AozoraCommand = (view) => {
  return insertHeadingAnnotation(view, 'は小見出し', '見出し')
}

// Indent: ［＃3字下げ］
export const insertIndent: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main
  const template = '［＃3字下げ］'
  view.dispatch({
    changes: { from: selection.from, insert: template },
    selection: EditorSelection.single(selection.from + 2, selection.from + 3) // Select the number
  })
  return true
}

// Right align: ［＃地付き］
export const insertRightAlign: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main
  const template = '［＃地付き］'
  view.dispatch({
    changes: { from: selection.from, insert: template },
    selection: EditorSelection.cursor(selection.from + template.length)
  })
  return true
}

// Main text block
export const insertMainTextBlock: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    const template = '［＃ここから本文］\n\n［＃ここで本文終わり］'
    view.dispatch({
      changes: { from: selection.from, insert: template },
      selection: EditorSelection.cursor(selection.from + '［＃ここから本文］\n'.length)
    })
  } else {
    const selectedText = state.sliceDoc(selection.from, selection.to)
    const insertText = `［＃ここから本文］\n${selectedText}\n［＃ここで本文終わり］`
    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert: insertText }
    })
  }
  return true
}

// Framed block
export const insertFramedBlock: AozoraCommand = (view) => {
  const { state } = view
  const selection = state.selection.main

  if (selection.empty) {
    const template = '［＃ここから罫囲み］\n\n［＃ここで罫囲み終わり］'
    view.dispatch({
      changes: { from: selection.from, insert: template },
      selection: EditorSelection.cursor(selection.from + '［＃ここから罫囲み］\n'.length)
    })
  } else {
    const selectedText = state.sliceDoc(selection.from, selection.to)
    const insertText = `［＃ここから罫囲み］\n${selectedText}\n［＃ここで罫囲み終わり］`
    view.dispatch({
      changes: { from: selection.from, to: selection.to, insert: insertText }
    })
  }
  return true
}

// Generic annotation insert helper for palette
export function insertAnnotation(view: EditorView, template: string, hasSelection: boolean): boolean {
  const { state } = view
  const selection = state.selection.main

  // Check if template is a block format (contains newlines)
  const isBlockFormat = template.includes('\n')
  // Check if template is a wrap format (text is wrapped by annotations, e.g., ［＃傍点］{text}［＃傍点終わり］)
  const isWrapFormat = template.includes('{text}') && !template.endsWith('］') === false &&
    template.indexOf('{text}') > 0 && template.indexOf('{text}') + 6 < template.length

  if (hasSelection && !selection.empty) {
    const selectedText = state.sliceDoc(selection.from, selection.to)

    if (isBlockFormat || isWrapFormat) {
      // Block/wrap format: replace selection with template filled in
      const insertText = template.replace('{text}', selectedText)
      view.dispatch({
        changes: { from: selection.from, to: selection.to, insert: insertText }
      })
    } else {
      // Inline format: text［＃「text」...］
      // Template format: ［＃「{text}」に傍点］
      // Output: text + annotation
      const annotation = template.replace('{text}', selectedText)
      view.dispatch({
        changes: { from: selection.to, insert: annotation },
        selection: EditorSelection.cursor(selection.to + annotation.length)
      })
    }
  } else {
    // No selection: insert template with placeholder
    const placeholder = 'テキスト'

    if (isBlockFormat) {
      // Block format (multi-line)
      const insertText = template.replace('{text}', '')
      const cursorPos = template.indexOf('{text}')
      view.dispatch({
        changes: { from: selection.from, insert: insertText },
        selection: EditorSelection.cursor(selection.from + cursorPos)
      })
    } else if (isWrapFormat) {
      // Wrap format: ［＃傍点］テキスト［＃傍点終わり］
      const insertText = template.replace('{text}', placeholder)
      const textStart = template.indexOf('{text}')
      view.dispatch({
        changes: { from: selection.from, insert: insertText },
        selection: EditorSelection.single(selection.from + textStart, selection.from + textStart + placeholder.length)
      })
    } else if (template.includes('{text}')) {
      // Inline format: テキスト［＃「テキスト」...］
      const annotation = template.replace('{text}', placeholder)
      const insertText = placeholder + annotation
      view.dispatch({
        changes: { from: selection.from, insert: insertText },
        selection: EditorSelection.single(selection.from, selection.from + placeholder.length)
      })
    } else {
      // No placeholder, just insert
      view.dispatch({
        changes: { from: selection.from, insert: template },
        selection: EditorSelection.cursor(selection.from + template.length)
      })
    }
  }
  return true
}
