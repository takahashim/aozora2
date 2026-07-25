import { EditorView, ViewPlugin, ViewUpdate } from '@codemirror/view'
import { t, onLangChange } from '@/i18n'
import { insertAnnotation } from './commands'

type CategoryType = 'decoration' | 'structure' | 'block'

interface AnnotationCommand {
  id: string
  labelKey: string
  category: CategoryType
  template: string
  hasSelection: boolean
}

const annotationCommands: AnnotationCommand[] = [
  // Decoration (装飾系)
  { id: 'bouten', labelKey: 'palette.cmd.bouten', category: 'decoration', template: '［＃傍点］{text}［＃傍点終わり］', hasSelection: true },
  { id: 'nijubouten', labelKey: 'palette.cmd.nijubouten', category: 'decoration', template: '［＃「{text}」に二重傍点］', hasSelection: true },
  { id: 'kenten', labelKey: 'palette.cmd.kenten', category: 'decoration', template: '［＃「{text}」に圏点］', hasSelection: true },
  { id: 'bousen', labelKey: 'palette.cmd.bousen', category: 'decoration', template: '［＃「{text}」に傍線］', hasSelection: true },
  { id: 'nijubousen', labelKey: 'palette.cmd.nijubousen', category: 'decoration', template: '［＃「{text}」に二重傍線］', hasSelection: true },
  { id: 'futoji', labelKey: 'palette.cmd.futoji', category: 'decoration', template: '［＃太字］{text}［＃太字終わり］', hasSelection: true },
  { id: 'shatai', labelKey: 'palette.cmd.shatai', category: 'decoration', template: '［＃「{text}」に斜体］', hasSelection: true },

  // Structure (構造系)
  { id: 'omidashi', labelKey: 'palette.cmd.omidashi', category: 'structure', template: '［＃「{text}」は大見出し］', hasSelection: true },
  { id: 'nakamidashi', labelKey: 'palette.cmd.nakamidashi', category: 'structure', template: '［＃「{text}」は中見出し］', hasSelection: true },
  { id: 'komidashi', labelKey: 'palette.cmd.komidashi', category: 'structure', template: '［＃「{text}」は小見出し］', hasSelection: true },
  { id: 'jisage', labelKey: 'palette.cmd.jisage', category: 'structure', template: '［＃3字下げ］', hasSelection: false },
  { id: 'jitsuki', labelKey: 'palette.cmd.jitsuki', category: 'structure', template: '［＃地付き］', hasSelection: false },

  // Block (ブロック系)
  { id: 'inyou', labelKey: 'palette.cmd.inyou', category: 'block', template: '［＃ここから引用］\n{text}\n［＃ここで引用終わり］', hasSelection: true },
  { id: 'honbun', labelKey: 'palette.cmd.honbun', category: 'block', template: '［＃ここから本文］\n{text}\n［＃ここで本文終わり］', hasSelection: true },
  { id: 'keigakomi', labelKey: 'palette.cmd.keigakomi', category: 'block', template: '［＃ここから罫囲み］\n{text}\n［＃ここで罫囲み終わり］', hasSelection: true },
]

const categoryOrder: CategoryType[] = ['decoration', 'structure', 'block']

const categoryLabelKeys: Record<CategoryType, string> = {
  decoration: 'palette.category.decoration',
  structure: 'palette.category.structure',
  block: 'palette.category.block',
}

// Palette state
let paletteElement: HTMLElement | null = null
let currentView: EditorView | null = null
let selectedIndex = 0
let filteredCommands: AnnotationCommand[] = []
let unsubscribeLang: (() => void) | null = null

function updateFilteredCommands(query: string): void {
  if (!query) {
    filteredCommands = [...annotationCommands]
  } else {
    const lowerQuery = query.toLowerCase()
    filteredCommands = annotationCommands.filter(cmd => {
      const label = t(cmd.labelKey as Parameters<typeof t>[0])
      return label.toLowerCase().includes(lowerQuery) ||
             cmd.template.toLowerCase().includes(lowerQuery)
    })
  }
  selectedIndex = 0
}

function renderPalette(): void {
  if (!paletteElement) return

  const container = paletteElement.querySelector('.palette-commands')
  if (!container) return

  container.innerHTML = ''

  // Group commands by category
  const groupedCommands: Record<CategoryType, AnnotationCommand[]> = {
    decoration: [],
    structure: [],
    block: [],
  }

  filteredCommands.forEach(cmd => {
    groupedCommands[cmd.category].push(cmd)
  })

  let globalIndex = 0

  categoryOrder.forEach(category => {
    const commands = groupedCommands[category]
    if (commands.length === 0) return

    // Category header
    const header = document.createElement('div')
    header.className = 'palette-category-header'
    header.textContent = t(categoryLabelKeys[category] as Parameters<typeof t>[0])
    container.appendChild(header)

    // Commands
    commands.forEach(cmd => {
      const item = document.createElement('div')
      item.className = 'palette-item'
      if (globalIndex === selectedIndex) {
        item.classList.add('selected')
      }
      item.setAttribute('data-index', String(globalIndex))

      const label = document.createElement('span')
      label.className = 'palette-item-label'
      label.textContent = t(cmd.labelKey as Parameters<typeof t>[0])
      item.appendChild(label)

      const preview = document.createElement('span')
      preview.className = 'palette-item-preview'
      // Show a simplified preview
      preview.textContent = cmd.template.replace('{text}', '...').replace(/\n/g, ' ')
      item.appendChild(preview)

      item.addEventListener('click', () => {
        executeCommand(cmd)
      })

      item.addEventListener('mouseenter', () => {
        selectedIndex = globalIndex
        updateSelection()
      })

      container.appendChild(item)
      globalIndex++
    })
  })
}

function updateSelection(): void {
  if (!paletteElement) return

  const items = paletteElement.querySelectorAll('.palette-item')
  items.forEach((item, index) => {
    if (index === selectedIndex) {
      item.classList.add('selected')
      item.scrollIntoView({ block: 'nearest' })
    } else {
      item.classList.remove('selected')
    }
  })
}

function executeCommand(cmd: AnnotationCommand): void {
  if (!currentView) return

  insertAnnotation(currentView, cmd.template, cmd.hasSelection)
  closePalette()
  currentView.focus()
}

function executeSelectedCommand(): void {
  if (filteredCommands.length === 0) return
  const cmd = filteredCommands[selectedIndex]
  if (cmd) {
    executeCommand(cmd)
  }
}

function closePalette(): void {
  if (paletteElement) {
    paletteElement.remove()
    paletteElement = null
  }
  if (unsubscribeLang) {
    unsubscribeLang()
    unsubscribeLang = null
  }
  currentView = null
}

function handleKeydown(e: KeyboardEvent): void {
  if (!paletteElement) return

  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      selectedIndex = Math.min(selectedIndex + 1, filteredCommands.length - 1)
      updateSelection()
      break
    case 'ArrowUp':
      e.preventDefault()
      selectedIndex = Math.max(selectedIndex - 1, 0)
      updateSelection()
      break
    case 'Enter':
      e.preventDefault()
      executeSelectedCommand()
      break
    case 'Escape':
      e.preventDefault()
      closePalette()
      currentView?.focus()
      break
  }
}

function createPaletteElement(view: EditorView): HTMLElement {
  const palette = document.createElement('div')
  palette.className = 'aozora-palette-overlay'

  const content = document.createElement('div')
  content.className = 'aozora-palette'

  // Search input
  const searchContainer = document.createElement('div')
  searchContainer.className = 'palette-search-container'

  const searchInput = document.createElement('input')
  searchInput.type = 'text'
  searchInput.className = 'palette-search'
  searchInput.placeholder = t('palette.search')
  searchInput.setAttribute('data-i18n-placeholder', 'palette.search')

  searchInput.addEventListener('input', () => {
    updateFilteredCommands(searchInput.value)
    renderPalette()
  })

  searchInput.addEventListener('keydown', handleKeydown)

  searchContainer.appendChild(searchInput)
  content.appendChild(searchContainer)

  // Commands container
  const commands = document.createElement('div')
  commands.className = 'palette-commands'
  content.appendChild(commands)

  palette.appendChild(content)

  // Close on backdrop click
  palette.addEventListener('click', (e) => {
    if (e.target === palette) {
      closePalette()
      view.focus()
    }
  })

  return palette
}

export function openAnnotationPalette(view: EditorView): boolean {
  // Close existing palette if open
  if (paletteElement) {
    closePalette()
  }

  currentView = view
  updateFilteredCommands('')

  paletteElement = createPaletteElement(view)
  document.body.appendChild(paletteElement)

  renderPalette()

  // Focus search input
  const searchInput = paletteElement.querySelector('.palette-search') as HTMLInputElement
  if (searchInput) {
    searchInput.focus()
  }

  // Listen for language changes
  unsubscribeLang = onLangChange(() => {
    if (paletteElement) {
      // Update placeholder
      const input = paletteElement.querySelector('[data-i18n-placeholder]') as HTMLInputElement
      if (input) {
        const key = input.getAttribute('data-i18n-placeholder')
        if (key) input.placeholder = t(key as Parameters<typeof t>[0])
      }
      // Re-render commands
      renderPalette()
    }
  })

  return true
}

// ViewPlugin to handle palette cleanup
export const aozoraPalette = ViewPlugin.fromClass(class {
  constructor(_view: EditorView) {}

  update(_update: ViewUpdate) {}

  destroy() {
    closePalette()
  }
})
