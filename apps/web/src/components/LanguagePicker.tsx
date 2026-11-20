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
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const current = locale === 'pt-BR' ? 'pt-BR' : 'en'
  const pick = async (language: string) => {
    if (language === current || saving) return
    setSaving(true)
    setError(null)
    try {
      await saveSettings({ language })
      window.location.reload()
    } catch (failure: unknown) {
      setSaving(false)
      setError(messageOf(failure))
    }
  }
  return (
    <span className="idioma-wrap">
      <span className={`idioma ${saving ? 'salvando' : ''}`} role="radiogroup" aria-label={t.language} aria-busy={saving} title={error ?? t.language}>
        {LANGUAGES.map((language) => (
          <button
            key={language.id}
            type="button"
            role="radio"
            aria-checked={current === language.id}
            className={current === language.id ? 'ativo' : ''}
            disabled={saving}
            onClick={() => void pick(language.id)}
          >
            {language.label}
          </button>
        ))}
      </span>
      {error && <small className="nota failed">{error}</small>}
    </span>
  )
}
