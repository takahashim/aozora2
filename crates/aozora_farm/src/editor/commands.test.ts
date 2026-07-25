import { describe, it, expect } from 'vitest'
import { EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import {
  insertRuby,
  insertRubyRange,
  insertEmphasis,
  insertDoubleEmphasis,
  insertCircleMarks,
  insertSideLine,
  insertDoubleSideLine,
  insertBold,
  insertItalic,
  insertHeadingLarge,
  insertHeadingMedium,
  insertHeadingSmall,
  insertIndent,
  insertRightAlign,
  insertQuoteBlock,
  insertMainTextBlock,
  insertFramedBlock,
} from './commands'

// Helper to create EditorView with initial content and selection
function createView(doc: string, from: number, to?: number): EditorView {
  const state = EditorState.create({
    doc,
    selection: { anchor: from, head: to ?? from },
  })
  const parent = document.createElement('div')
  return new EditorView({ state, parent })
}

// Helper to get document content after command
function getDoc(view: EditorView): string {
  return view.state.doc.toString()
}

// Helper to get selection range after command
function getSelection(view: EditorView): { from: number; to: number } {
  const sel = view.state.selection.main
  return { from: sel.from, to: sel.to }
}

describe('Ruby commands', () => {
  describe('insertRuby', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertRuby(view)
      expect(getDoc(view)).toBe('漢字《よみ》')
      expect(getSelection(view)).toEqual({ from: 0, to: 2 }) // 漢字 selected
    })

    it('wraps selected text with ruby brackets', () => {
      const view = createView('東京', 0, 2)
      insertRuby(view)
      expect(getDoc(view)).toBe('東京《》')
      expect(getSelection(view)).toEqual({ from: 3, to: 3 }) // cursor inside 《》
    })
  })

  describe('insertRubyRange', () => {
    it('inserts template with range marker when no selection', () => {
      const view = createView('', 0)
      insertRubyRange(view)
      expect(getDoc(view)).toBe('｜漢字《よみ》')
      expect(getSelection(view)).toEqual({ from: 1, to: 3 }) // 漢字 selected
    })

    it('wraps selected text with range marker and ruby brackets', () => {
      const view = createView('東京', 0, 2)
      insertRubyRange(view)
      expect(getDoc(view)).toBe('｜東京《》')
      expect(getSelection(view)).toEqual({ from: 4, to: 4 }) // cursor inside 《》
    })
  })
})

describe('Inline annotation commands', () => {
  describe('insertEmphasis', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertEmphasis(view)
      expect(getDoc(view)).toBe('［＃傍点］テキスト［＃傍点終わり］')
      // テキスト should be selected (after ［＃傍点］ which is 5 chars)
      expect(getSelection(view)).toEqual({ from: 5, to: 9 })
    })

    it('wraps selected text with block annotations', () => {
      const view = createView('歌', 0, 1)
      insertEmphasis(view)
      expect(getDoc(view)).toBe('［＃傍点］歌［＃傍点終わり］')
    })
  })

  describe('insertDoubleEmphasis', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertDoubleEmphasis(view)
      expect(getDoc(view)).toBe('テキスト［＃「テキスト」に二重傍点］')
    })

    it('adds annotation after selected text', () => {
      const view = createView('重要', 0, 2)
      insertDoubleEmphasis(view)
      expect(getDoc(view)).toBe('重要［＃「重要」に二重傍点］')
    })
  })

  describe('insertCircleMarks', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertCircleMarks(view)
      expect(getDoc(view)).toBe('テキスト［＃「テキスト」に圏点］')
    })

    it('adds annotation after selected text', () => {
      const view = createView('強調', 0, 2)
      insertCircleMarks(view)
      expect(getDoc(view)).toBe('強調［＃「強調」に圏点］')
    })
  })

  describe('insertSideLine', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertSideLine(view)
      expect(getDoc(view)).toBe('テキスト［＃「テキスト」に傍線］')
    })

    it('adds annotation after selected text', () => {
      const view = createView('傍線', 0, 2)
      insertSideLine(view)
      expect(getDoc(view)).toBe('傍線［＃「傍線」に傍線］')
    })
  })

  describe('insertDoubleSideLine', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertDoubleSideLine(view)
      expect(getDoc(view)).toBe('テキスト［＃「テキスト」に二重傍線］')
    })

    it('adds annotation after selected text', () => {
      const view = createView('二重線', 0, 3)
      insertDoubleSideLine(view)
      expect(getDoc(view)).toBe('二重線［＃「二重線」に二重傍線］')
    })
  })

  describe('insertBold', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertBold(view)
      expect(getDoc(view)).toBe('［＃太字］テキスト［＃太字終わり］')
      // テキスト should be selected (after ［＃太字］ which is 5 chars)
      expect(getSelection(view)).toEqual({ from: 5, to: 9 })
    })

    it('wraps selected text with block annotations', () => {
      const view = createView('強調', 0, 2)
      insertBold(view)
      expect(getDoc(view)).toBe('［＃太字］強調［＃太字終わり］')
    })
  })

  describe('insertItalic', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertItalic(view)
      expect(getDoc(view)).toBe('テキスト［＃「テキスト」に斜体］')
    })

    it('adds annotation after selected text', () => {
      const view = createView('斜体', 0, 2)
      insertItalic(view)
      expect(getDoc(view)).toBe('斜体［＃「斜体」に斜体］')
    })
  })
})

describe('Heading annotation commands', () => {
  describe('insertHeadingLarge', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertHeadingLarge(view)
      expect(getDoc(view)).toBe('［＃「見出し」は大見出し］見出し')
      // 見出し text at the end should be selected
      const sel = getSelection(view)
      expect(getDoc(view).slice(sel.from, sel.to)).toBe('見出し')
    })

    it('adds annotation before selected text', () => {
      const view = createView('第一章', 0, 3)
      insertHeadingLarge(view)
      expect(getDoc(view)).toBe('［＃「第一章」は大見出し］第一章')
    })
  })

  describe('insertHeadingMedium', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertHeadingMedium(view)
      expect(getDoc(view)).toBe('［＃「見出し」は中見出し］見出し')
    })

    it('adds annotation before selected text', () => {
      const view = createView('第一節', 0, 3)
      insertHeadingMedium(view)
      expect(getDoc(view)).toBe('［＃「第一節」は中見出し］第一節')
    })
  })

  describe('insertHeadingSmall', () => {
    it('inserts template when no selection', () => {
      const view = createView('', 0)
      insertHeadingSmall(view)
      expect(getDoc(view)).toBe('［＃「見出し」は小見出し］見出し')
    })

    it('adds annotation before selected text', () => {
      const view = createView('一', 0, 1)
      insertHeadingSmall(view)
      expect(getDoc(view)).toBe('［＃「一」は小見出し］一')
    })
  })
})

describe('Structure commands', () => {
  describe('insertIndent', () => {
    it('inserts indent annotation with number selected', () => {
      const view = createView('', 0)
      insertIndent(view)
      expect(getDoc(view)).toBe('［＃3字下げ］')
      expect(getSelection(view)).toEqual({ from: 2, to: 3 }) // 3 selected for editing
    })
  })

  describe('insertRightAlign', () => {
    it('inserts right align annotation', () => {
      const view = createView('', 0)
      insertRightAlign(view)
      expect(getDoc(view)).toBe('［＃地付き］')
    })
  })
})

describe('Block commands', () => {
  describe('insertQuoteBlock', () => {
    it('inserts block markers when no selection', () => {
      const view = createView('', 0)
      insertQuoteBlock(view)
      expect(getDoc(view)).toBe('［＃ここから引用］\n\n［＃ここで引用終わり］')
    })

    it('wraps selected text with block markers', () => {
      const view = createView('引用文', 0, 3)
      insertQuoteBlock(view)
      expect(getDoc(view)).toBe('［＃ここから引用］\n引用文\n［＃ここで引用終わり］')
    })
  })

  describe('insertMainTextBlock', () => {
    it('inserts block markers when no selection', () => {
      const view = createView('', 0)
      insertMainTextBlock(view)
      expect(getDoc(view)).toBe('［＃ここから本文］\n\n［＃ここで本文終わり］')
    })

    it('wraps selected text with block markers', () => {
      const view = createView('本文テキスト', 0, 6)
      insertMainTextBlock(view)
      expect(getDoc(view)).toBe('［＃ここから本文］\n本文テキスト\n［＃ここで本文終わり］')
    })
  })

  describe('insertFramedBlock', () => {
    it('inserts block markers when no selection', () => {
      const view = createView('', 0)
      insertFramedBlock(view)
      expect(getDoc(view)).toBe('［＃ここから罫囲み］\n\n［＃ここで罫囲み終わり］')
    })

    it('wraps selected text with block markers', () => {
      const view = createView('囲み内容', 0, 4)
      insertFramedBlock(view)
      expect(getDoc(view)).toBe('［＃ここから罫囲み］\n囲み内容\n［＃ここで罫囲み終わり］')
    })
  })
})
