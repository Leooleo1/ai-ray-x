import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { t } from '../../i18n'

/** The field grows with the text up to this many pixels, then scrolls. */
const MAX_HEIGHT = 168

// Module scope: switching to another bot and back keeps what was typed.
const drafts = new Map<string, string>()

interface ComposerProps {
  /** Whose draft this is (one per chat). */
  draftKey: string
  placeholder: string
  maxLength: number
  busy: boolean
  /** Resolves true when the message went out, so the draft clears. */
  onSend: (text: string) => Promise<boolean>
  onStop: () => void
}

/** One line that grows. Enter sends, Shift+Enter breaks the line; while the bot works, the button and Esc stop it. */
export function Composer({ draftKey, placeholder, maxLength, busy, onSend, onStop }: ComposerProps) {
  const [draft, setDraft] = useState(() => drafts.get(draftKey) ?? '')
  const field = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    setDraft(drafts.get(draftKey) ?? '')
    field.current?.focus()
  }, [draftKey])
  useEffect(() => {
    drafts.set(draftKey, draft)
    const el = field.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`
  }, [draft, draftKey])

  const send = async () => {
    const text = draft.trim()
    if (!text || busy) return
    if (await onSend(text)) setDraft('')
  }
  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Escape' && busy) {
      event.preventDefault()
      onStop()
    } else if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      void send()
    }
  }

  return (
    <form
      className="compositor"
      onSubmit={(event) => {
        event.preventDefault()
        void send()
      }}
    >
      <textarea
        ref={field}
        rows={1}
        value={draft}
        maxLength={maxLength}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKey}
      />
      {busy ? (
        <button className="enviar parar" type="button" onClick={onStop} aria-label={t.stop} title={`${t.stop} (Esc)`}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect x="7" y="7" width="10" height="10" rx="2" />
          </svg>
        </button>
      ) : (
        <button className="enviar" type="submit" disabled={!draft.trim()} aria-label={t.send}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 19V5M5 12l7-7 7 7" />
          </svg>
        </button>
      )}
    </form>
  )
}
