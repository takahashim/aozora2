import { ja } from './ja'
import { en } from './en'

export type TranslationKey = keyof typeof ja
type TranslationMap = Record<TranslationKey, string>
type Translations = Record<string, TranslationMap>

const translations: Translations = { ja, en }

let currentLang = localStorage.getItem('aozora_farm_lang') || 'ja'

// Language change listeners
type LangChangeListener = (lang: string) => void
const langChangeListeners: LangChangeListener[] = []

export function onLangChange(listener: LangChangeListener): () => void {
  langChangeListeners.push(listener)
  return () => {
    const index = langChangeListeners.indexOf(listener)
    if (index >= 0) langChangeListeners.splice(index, 1)
  }
}

// CodeMirror phrase mapping (CodeMirror key -> our i18n key)
const codeMirrorPhraseMap: Record<string, TranslationKey> = {
  "Find": "search.find",
  "Replace": "search.replace",
  "next": "search.next",
  "previous": "search.previous",
  "regexp": "search.regexp",
  "replace": "search.replace-one",
  "replace all": "search.replace-all",
  "close": "search.close",
  "current match": "search.current-match",
  "replaced $ matches": "search.replaced-matches",
  "replaced match on line $": "search.replaced-match-on-line",
  "on line": "search.on-line",
  "Go to line": "search.goto-line",
  "go": "search.go",
}

export function getCodeMirrorPhrases(): Record<string, string> {
  const lang = translations[currentLang] || translations['en']
  const phrases: Record<string, string> = {}

  for (const [cmKey, i18nKey] of Object.entries(codeMirrorPhraseMap)) {
    phrases[cmKey] = lang[i18nKey] || cmKey
  }

  return phrases
}

export function t(key: TranslationKey, params: Record<string, string> = {}): string {
  const lang = translations[currentLang] || translations['en']
  let text: string = lang?.[key] || translations['en']?.[key] || key

  for (const [param, value] of Object.entries(params)) {
    text = text.replace(`{${param}}`, value)
  }

  return text
}

export function getLang(): string {
  return currentLang
}

export function setLang(lang: string): void {
  if (!translations[lang]) return

  currentLang = lang
  localStorage.setItem('aozora_farm_lang', lang)
  updateUI()

  // Notify listeners
  langChangeListeners.forEach(listener => listener(lang))
}

export function toggleLang(): void {
  setLang(currentLang === 'ja' ? 'en' : 'ja')
}

export function updateUI(): void {
  // Update elements with data-i18n attribute
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const key = el.getAttribute('data-i18n') as TranslationKey
    el.textContent = t(key)
  })

  // Update elements with data-i18n-placeholder attribute
  document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
    const key = el.getAttribute('data-i18n-placeholder') as TranslationKey
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      el.placeholder = t(key)
    }
  })

  // Update language toggle button
  const langToggle = document.getElementById('lang-toggle')
  if (langToggle) {
    langToggle.textContent = currentLang === 'ja' ? 'EN' : '日本語'
  }

  // Update document lang attribute
  document.documentElement.lang = currentLang

  // Update document title
  document.title = t('app.title')
}
