import { useEffect, useRef, useState } from 'react'
import { AGENTS } from '../agents'
import type { Report } from '../api'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { type Tip, markSeen, pendingTips, seenTips, useTipPrefs } from '../tips'
import { Avatar } from './Avatar'
import type { ChatNav } from './chat/nav'
import './tips.css'

/** A tip stays open this long, then folds into its agent's face. */
const OPEN_MS = 8000

interface TipToastProps {
  bots: Bots
  report: Report
  nav: ChatNav
  /** Lifted over the composer on the pages that have one. */
  raised: boolean
}

/** One floating tip at a time from Raio, Dieta or Memo; each shows once, and each agent can be muted. */
export function TipToast({ bots, report, nav, raised }: TipToastProps) {
  const [prefs, setPrefs] = useTipPrefs()
  // Bumped when a tip is closed, so the next one is worked out again.
  const [, setClosed] = useState(0)
  const [open, setOpen] = useState(true)
  const timer = useRef<number | undefined>(undefined)
  const tip = pendingTips(bots.state, report, prefs, seenTips())[0]
  const fold = () => {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setOpen(false), OPEN_MS)
  }
  // Each new tip opens, then folds by itself unless the pointer rests on it.
  useEffect(() => {
    if (!tip) return undefined
    setOpen(true)
    fold()
    return () => window.clearTimeout(timer.current)
  }, [tip?.key])
  if (!tip) return null
  const close = () => {
    markSeen(tip.key)
    setClosed((n) => n + 1)
  }
  const act = async (current: Tip) => {
    close()
    const { action } = current
    if (action.kind === 'house') await bots.act('house', { id: action.bot, agent: action.agent })
    if (action.kind === 'new-thread') await bots.act('thread_new', { id: action.bot })
    nav.openBot(action.bot)
  }
  const face = AGENTS[tip.agent]
  const place = raised ? 'elevada' : ''
  if (!open) {
    return (
      <button type="button" className={`dica-rosto ${place}`} aria-label={`${face.name}: ${tip.text}`} title={face.name} onClick={() => setOpen(true)}>
        <Avatar agent={face} size={30} state="idle" />
        <i aria-hidden="true" />
      </button>
    )
  }
  return (
    <aside
      className={`dica-flutuante ${place}`}
      role="status"
      aria-live="polite"
      key={tip.key}
      onMouseEnter={() => window.clearTimeout(timer.current)}
      onMouseLeave={fold}
    >
      <header>
        <Avatar agent={face} size={28} state="idle" />
        <span>
          <b style={{ color: face.color }}>{face.name}</b>
          <small>{t.tipFree}</small>
        </span>
        <button type="button" className="icone-btn" aria-label={t.tipClose} title={t.tipClose} onClick={close}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        </button>
      </header>
      <p>{tip.text}</p>
      <footer>
        <button type="button" className="dica-acao" onClick={() => void act(tip)}>
          {tip.label}
        </button>
        <button type="button" className="lado-link" onClick={() => setPrefs({ ...prefs, [tip.agent]: false })}>
          {t.tipMute.replace('{name}', face.name)}
        </button>
      </footer>
    </aside>
  )
}
