import { useState } from 'react'
import { saveSettings } from '../api'
import { locale, t } from '../i18n'
import './language.css'

const LANGUAGES = [
  { id: 'en', label: 'English' },
  { id: 'pt-BR', label: 'Português' },
] as const

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** English or Portuguese for the whole panel; kept in raiox's settings, so it holds across launches. */
export function LanguagePicker() {
  const [error, setError] = useState<string | null>(null)
  const current = locale === 'pt-BR' ? 'pt-BR' : 'en'
  const pick = async (language: string) => {
    if (language === current) return
    try {
      await saveSettings({ language })
      window.location.reload()
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  return (
    <span className="idioma" role="radiogroup" aria-label={t.language} title={error ?? t.language}>
      {LANGUAGES.map((language) => (
        <button key={language.id} type="button" role="radio" aria-checked={current === language.id} className={current === language.id ? 'ativo' : ''} onClick={() => void pick(language.id)}>
          {language.label}
        </button>
      ))}
    </span>
  )
}
