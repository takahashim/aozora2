import { createEditor, getContent, setContent, getEditor, openSearch, openReplace } from '@/editor'
import { initPreview, updatePreview, setBaseDir, getHtml } from '@/preview'
import { t, updateUI, toggleLang, onLangChange } from '@/i18n'
import {
  initResourcePaths,
  openTextFile,
  saveTextFile,
  saveHtmlFile,
  getDirectory,
  getFilename,
} from '@/commands/tauri'
import { Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu'
import { getVersion } from '@tauri-apps/api/app'

// State
let currentFilename = ''
let _currentFilePath = ''  // Prefixed with _ to indicate unused for now
let debounceTimer: number | null = null
let appVersion = ''

// Elements
const editorContainer = document.getElementById('editor-container')!
const previewFrame = document.getElementById('preview-frame') as HTMLIFrameElement
const fileInfo = document.getElementById('file-info')!
const status = document.getElementById('status')!
const openFileBtn = document.getElementById('open-file')!
const saveTextBtn = document.getElementById('save-text')!
const viewHtmlBtn = document.getElementById('view-html')!
const copyHtmlBtn = document.getElementById('copy-html')!
const saveHtmlBtn = document.getElementById('save-html')!
const htmlModal = document.getElementById('html-modal')!
const htmlSource = document.getElementById('html-source')!
const closeModalBtn = document.getElementById('close-modal')!
const langToggleBtn = document.getElementById('lang-toggle')!

// About modal elements
const aboutModal = document.getElementById('about-modal')!
const aboutTitle = document.getElementById('about-title')!
const aboutVersion = document.getElementById('about-version')!
const aboutDescription = document.getElementById('about-description')!
const closeAboutBtn = document.getElementById('close-about')!

// Update about modal content
function updateAboutModal(): void {
  aboutTitle.textContent = t('about.title')
  aboutVersion.textContent = t('about.version', { version: appVersion })
  // Replace \n with <br> for description
  aboutDescription.innerHTML = t('about.description').replace(/\n/g, '<br>')
}

// Show about dialog
function showAbout(): void {
  aboutModal.classList.add('show')
}

// Close about dialog
function closeAbout(): void {
  aboutModal.classList.remove('show')
}

// Setup application menu
async function setupMenu(): Promise<void> {
  // App submenu
  const appSubmenu = await Submenu.new({
    text: '青空ファーム',
    items: [
      await MenuItem.new({
        id: 'about',
        text: 'このアプリについて',
        action: () => showAbout(),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await PredefinedMenuItem.new({ item: 'Quit', text: '終了' }),
    ],
  })

  // File submenu
  const fileSubmenu = await Submenu.new({
    text: 'ファイル',
    items: [
      await MenuItem.new({
        id: 'open_file',
        text: 'ファイルを開く',
        accelerator: 'CmdOrCtrl+O',
        action: () => openFile(),
      }),
      await MenuItem.new({
        id: 'save_text',
        text: 'テキストを保存',
        accelerator: 'CmdOrCtrl+S',
        action: () => saveText(),
      }),
    ],
  })

  // Edit submenu
  const editSubmenu = await Submenu.new({
    text: '編集',
    items: [
      await MenuItem.new({
        id: 'search',
        text: '検索',
        accelerator: 'CmdOrCtrl+F',
        action: () => openSearch(),
      }),
      await MenuItem.new({
        id: 'replace',
        text: '置換',
        accelerator: 'CmdOrCtrl+Shift+F',
        action: () => openReplace(),
      }),
    ],
  })

  const menu = await Menu.new({
    items: [appSubmenu, fileSubmenu, editSubmenu],
  })

  await menu.setAsAppMenu()
}

// Initialize
async function init(): Promise<void> {
  // Initialize resource paths
  await initResourcePaths()

  // Get app version
  appVersion = await getVersion()

  // Setup application menu
  await setupMenu()

  // Initialize preview
  initPreview(previewFrame)

  // Create editor with change callback
  createEditor(editorContainer, debouncedConvert)

  // Setup event listeners
  setupEventListeners()

  // Update UI with translations
  updateUI()

  // Update about modal content
  updateAboutModal()

  // Listen for language changes to update about modal
  onLangChange(() => updateAboutModal())
}

// Debounced convert
function debouncedConvert(content: string): void {
  if (debounceTimer) {
    clearTimeout(debounceTimer)
  }
  // 大きな文書ほどプレビュー変換の頻度を下げる（base 500ms + 長さ比例、上限 2000ms）。
  const delay = Math.min(500 + Math.floor(content.length / 1000), 2000)
  debounceTimer = window.setTimeout(async () => {
    setStatus(t('status.converting'), '')
    await updatePreview(content)
    if (content.trim()) {
      setStatus(t('status.converted'), 'success')
    } else {
      setStatus(t('status.ready'), '')
    }
  }, delay)
}

// Set status message
function setStatus(message: string, type: string = ''): void {
  status.textContent = message
  status.className = 'status ' + type
}

// Open file
async function openFile(): Promise<void> {
  try {
    const result = await openTextFile()
    if (!result) return

    setContent(result.content)
    _currentFilePath = result.path
    currentFilename = getFilename(result.path)
    setBaseDir(getDirectory(result.path))
    fileInfo.textContent = t('file.info', { filename: currentFilename })

    // Trigger conversion
    await updatePreview(result.content)
    setStatus(t('status.converted'), 'success')
  } catch (error) {
    setStatus(t('error.open-file', { error: String(error) }), 'error')
  }
}

// Save text
async function saveText(): Promise<void> {
  const content = getContent()
  if (!content.trim()) {
    setStatus(t('status.no-text'), 'error')
    return
  }

  try {
    const defaultName = currentFilename || 'untitled.txt'
    const savePath = await saveTextFile(content, defaultName)

    if (savePath) {
      _currentFilePath = savePath
      currentFilename = getFilename(savePath)
      setBaseDir(getDirectory(savePath))
      fileInfo.textContent = t('file.info', { filename: currentFilename })
      setStatus(t('status.text-saved', { path: savePath }), 'success')
    }
  } catch (error) {
    setStatus(t('error.save-file', { error: String(error) }), 'error')
  }
}

// Save HTML
async function saveHtml(): Promise<void> {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.save-html') }), 'error')
    return
  }

  try {
    const defaultName = currentFilename
      ? currentFilename.replace(/\.txt$/i, '.html')
      : 'output.html'

    const savePath = await saveHtmlFile(html, defaultName)

    if (savePath) {
      setStatus(t('status.saved', { path: savePath }), 'success')
    }
  } catch (error) {
    setStatus(t('error.save-file', { error: String(error) }), 'error')
  }
}

// Copy HTML to clipboard
async function copyHtml(): Promise<void> {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.copy-html') }), 'error')
    return
  }

  try {
    await navigator.clipboard.writeText(html)
    setStatus(t('status.copied'), 'success')
  } catch (error) {
    setStatus(t('error.copy', { error: String(error) }), 'error')
  }
}

// View HTML source
function viewHtml(): void {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.view-html') }), 'error')
    return
  }

  htmlSource.textContent = html
  htmlModal.classList.add('show')
}

// Close modal
function closeModal(): void {
  htmlModal.classList.remove('show')
}

// Setup event listeners
function setupEventListeners(): void {
  openFileBtn.addEventListener('click', openFile)
  saveTextBtn.addEventListener('click', saveText)
  viewHtmlBtn.addEventListener('click', viewHtml)
  copyHtmlBtn.addEventListener('click', copyHtml)
  saveHtmlBtn.addEventListener('click', saveHtml)
  closeModalBtn.addEventListener('click', closeModal)
  closeAboutBtn.addEventListener('click', closeAbout)
  langToggleBtn.addEventListener('click', () => toggleLang())

  // Close modal on backdrop click
  htmlModal.addEventListener('click', (e) => {
    if (e.target === htmlModal) {
      closeModal()
    }
  })

  aboutModal.addEventListener('click', (e) => {
    if (e.target === aboutModal) {
      closeAbout()
    }
  })

  // Global keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    // Escape to close modal
    if (e.key === 'Escape') {
      if (htmlModal.classList.contains('show')) {
        closeModal()
      }
      if (aboutModal.classList.contains('show')) {
        closeAbout()
      }
    }
  })

  // Drag and drop
  const editor = getEditor()
  if (editor) {
    editor.dom.addEventListener('dragover', (e) => {
      e.preventDefault()
    })

    editor.dom.addEventListener('drop', async (e) => {
      e.preventDefault()

      const files = e.dataTransfer?.files
      if (!files || files.length === 0) return

      const file = files[0]
      if (!file.name.endsWith('.txt')) {
        setStatus(t('status.drop-txt'), 'error')
        return
      }

      try {
        const arrayBuffer = await file.arrayBuffer()
        const content = decodeText(new Uint8Array(arrayBuffer))
        setContent(content)
        currentFilename = file.name
        fileInfo.textContent = t('file.info', { filename: currentFilename })
        await updatePreview(content)
        setStatus(t('status.converted'), 'success')
      } catch (error) {
        setStatus(t('error.read-file', { error: String(error) }), 'error')
      }
    })
  }

  // Prevent default drag behavior on window
  window.addEventListener('dragover', (e) => e.preventDefault())
  window.addEventListener('drop', (e) => e.preventDefault())
}

// Decode text with encoding detection
function decodeText(bytes: Uint8Array): string {
  try {
    const utf8Decoder = new TextDecoder('utf-8', { fatal: true })
    return utf8Decoder.decode(bytes)
  } catch {
    const sjisDecoder = new TextDecoder('shift_jis')
    return sjisDecoder.decode(bytes)
  }
}

// Start the app
init()
