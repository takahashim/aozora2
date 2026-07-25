import { EditorView, showPanel, Panel } from '@codemirror/view'
import { t, onLangChange, type TranslationKey } from '@/i18n'
import {
  insertRuby,
  insertRubyRange,
  insertEmphasis,
  insertBold,
  insertHeadingLarge,
  insertHeadingMedium,
  insertHeadingSmall,
  insertIndent,
  insertQuoteBlock,
  undoCommand,
  redoCommand,
  AozoraCommand,
} from './commands'
import { openAnnotationPalette } from './palette'

interface ToolbarButton {
  id: string
  labelKey: TranslationKey
  tooltipKey: TranslationKey
  shortcut: string | null
  command: AozoraCommand
  icon: string
}

const toolbarButtons: ToolbarButton[] = [
  { id: 'undo', labelKey: 'toolbar.undo', tooltipKey: 'toolbar.undo.tooltip', shortcut: 'Cmd+Z', command: undoCommand, icon: '↩' },
  { id: 'redo', labelKey: 'toolbar.redo', tooltipKey: 'toolbar.redo.tooltip', shortcut: 'Cmd+Shift+Z', command: redoCommand, icon: '↪' },
  { id: 'ruby', labelKey: 'toolbar.ruby', tooltipKey: 'toolbar.ruby.tooltip', shortcut: 'Cmd+R', command: insertRuby, icon: 'あ' },
  { id: 'ruby-range', labelKey: 'toolbar.ruby-range', tooltipKey: 'toolbar.ruby-range.tooltip', shortcut: 'Cmd+Shift+R', command: insertRubyRange, icon: '｜' },
  { id: 'emphasis', labelKey: 'toolbar.emphasis', tooltipKey: 'toolbar.emphasis.tooltip', shortcut: 'Cmd+.', command: insertEmphasis, icon: '・' },
  { id: 'bold', labelKey: 'toolbar.bold', tooltipKey: 'toolbar.bold.tooltip', shortcut: 'Cmd+B', command: insertBold, icon: 'B' },
  { id: 'heading-large', labelKey: 'toolbar.heading-large', tooltipKey: 'toolbar.heading-large.tooltip', shortcut: null, command: insertHeadingLarge, icon: '大' },
  { id: 'heading-medium', labelKey: 'toolbar.heading-medium', tooltipKey: 'toolbar.heading-medium.tooltip', shortcut: null, command: insertHeadingMedium, icon: '中' },
  { id: 'heading-small', labelKey: 'toolbar.heading-small', tooltipKey: 'toolbar.heading-small.tooltip', shortcut: null, command: insertHeadingSmall, icon: '小' },
  { id: 'indent', labelKey: 'toolbar.indent', tooltipKey: 'toolbar.indent.tooltip', shortcut: 'Cmd+]', command: insertIndent, icon: '→' },
  { id: 'quote', labelKey: 'toolbar.quote', tooltipKey: 'toolbar.quote.tooltip', shortcut: 'Cmd+Shift+Q', command: insertQuoteBlock, icon: '引' },
  { id: 'palette', labelKey: 'toolbar.palette', tooltipKey: 'toolbar.palette.tooltip', shortcut: 'Cmd+E', command: openAnnotationPalette, icon: '＃' },
]

// Button groups for visual separation
const buttonGroups = [
  ['undo', 'redo'],
  ['ruby', 'ruby-range'],
  ['emphasis', 'bold'],
  ['heading-large', 'heading-medium', 'heading-small'],
  ['indent', 'quote'],
  ['palette'],
]

function createToolbar(view: EditorView): Panel {
  const dom = document.createElement('div')
  dom.className = 'aozora-toolbar'

  buttonGroups.forEach((groupIds, index) => {
    const group = document.createElement('div')
    group.className = 'toolbar-group'

    groupIds.forEach(id => {
      const btnConfig = toolbarButtons.find(b => b.id === id)
      if (!btnConfig) return

      const button = document.createElement('button')
      button.className = 'toolbar-btn'
      button.setAttribute('data-command', id)
      button.type = 'button'

      // Set tooltip
      const tooltipText = btnConfig.shortcut
        ? `${t(btnConfig.tooltipKey)} (${btnConfig.shortcut})`
        : t(btnConfig.tooltipKey)
      button.setAttribute('title', tooltipText)
      button.setAttribute('data-tooltip-key', btnConfig.tooltipKey)
      button.setAttribute('data-shortcut', btnConfig.shortcut || '')

      // Icon
      const iconSpan = document.createElement('span')
      iconSpan.className = 'toolbar-icon'
      iconSpan.textContent = btnConfig.icon
      button.appendChild(iconSpan)

      // Label
      const labelSpan = document.createElement('span')
      labelSpan.className = 'toolbar-label'
      labelSpan.textContent = t(btnConfig.labelKey)
      labelSpan.setAttribute('data-i18n', btnConfig.labelKey)
      button.appendChild(labelSpan)

      button.addEventListener('click', (e) => {
        e.preventDefault()
        btnConfig.command(view)
        view.focus()
      })

      group.appendChild(button)
    })

    dom.appendChild(group)

    // Add separator between groups (except after last group)
    if (index < buttonGroups.length - 1) {
      const separator = document.createElement('div')
      separator.className = 'toolbar-separator'
      dom.appendChild(separator)
    }
  })

  // Listen for language changes
  const unsubscribe = onLangChange(() => {
    // Update labels
    dom.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.getAttribute('data-i18n')
      if (key) el.textContent = t(key as Parameters<typeof t>[0])
    })
    // Update tooltips
    dom.querySelectorAll('[data-command]').forEach(el => {
      const id = el.getAttribute('data-command')
      const btnConfig = toolbarButtons.find(b => b.id === id)
      if (btnConfig) {
        const tooltipText = btnConfig.shortcut
          ? `${t(btnConfig.tooltipKey)} (${btnConfig.shortcut})`
          : t(btnConfig.tooltipKey)
        el.setAttribute('title', tooltipText)
      }
    })
  })

  return {
    dom,
    top: true,
    destroy: () => unsubscribe(),
  }
}

export const aozoraToolbar = showPanel.of(createToolbar)
