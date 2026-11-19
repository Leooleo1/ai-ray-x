import { useState } from 'react'
import { t } from '../i18n'

interface PairScreenProps {
  message: string | null
  /** Pairs with a typed code (for an app opened from the home screen, which a QR link does not reach). */
  onPair: (code: string) => Promise<void>
}

/** What a device sees before it is paired, or after it was removed. */
export function PairScreen({ message, onPair }: PairScreenProps) {
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const ready = code.replace(/[^a-z0-9]/gi, '').length === 10
  return (
    <main className="parear">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="6.5" y="2.5" width="11" height="19" rx="2.5" />
        <path d="M10.5 18.5h3" />
      </svg>
      <b>{t.pairTitle}</b>
      <p>{t.pairHint}</p>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          setBusy(true)
          void onPair(code).finally(() => setBusy(false))
        }}
      >
        <input
          value={code}
          maxLength={11}
          placeholder="ABCDE-FGHJK"
          aria-label={t.pairCode}
          autoCapitalize="characters"
          autoComplete="one-time-code"
          spellCheck={false}
          onChange={(event) => setCode(event.target.value.toUpperCase())}
        />
        <button type="submit" className="cheio" disabled={!ready || busy}>
          {t.pairButton}
        </button>
      </form>
      {message && <p className="nota failed">{t.pairFailed.replace('{e}', message)}</p>}
    </main>
  )
}
