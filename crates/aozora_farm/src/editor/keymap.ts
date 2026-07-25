import { KeyBinding } from '@codemirror/view'
import {
  insertRuby,
  insertRubyRange,
  insertEmphasis,
  insertBold,
  insertIndent,
  insertQuoteBlock,
} from './commands'
import { openAnnotationPalette } from './palette'

export const aozoraKeymap: KeyBinding[] = [
  { key: 'Mod-r', run: insertRuby, preventDefault: true },
  { key: 'Mod-Shift-r', run: insertRubyRange, preventDefault: true },
  { key: 'Mod-.', run: insertEmphasis, preventDefault: true },
  { key: 'Mod-b', run: insertBold, preventDefault: true },
  { key: 'Mod-]', run: insertIndent, preventDefault: true },
  { key: 'Mod-Shift-q', run: insertQuoteBlock, preventDefault: true },
  { key: 'Mod-e', run: openAnnotationPalette, preventDefault: true },
]
