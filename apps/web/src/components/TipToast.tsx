import { useState } from 'react'
import { AGENTS } from '../agents'
import type { Report } from '../api'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { type Tip, markSeen, pendingTips, seenTips, useTipPrefs } from '../tips'
import { Avatar } from './Avatar'
import type { ChatNav } from './chat/nav'
import './tips.css'

interface TipToastProps {
  bots: Bots
  report: Report
  nav: ChatNav
}

/** One floating tip at a time from Raio, Dieta or Memo; each shows once, and each agent can be muted. */
export function TipToast({ bots, report, nav }: TipToastProps) {
  const [prefs, setPrefs] = useTipPrefs()
  // Bumped when a tip is closed, so the next one is worked out again.
  const [, setClosed] = useState(0)
  const tip = pendingTips(bots.state, report, prefs, seenTips())[0]
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
  return (
    <aside className="dica-flutuante" role="status" aria-live="polite" key={tip.key}>
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
