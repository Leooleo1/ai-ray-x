import { useState } from 'react'
import type { Bot } from '../../api'
import { t } from '../../i18n'
import { clock } from './BotLine'

interface PinsLineProps {
  bot: Bot
  onGo: (pin: { thread: string; ms: number }) => void
}

/** "Pinned: N ›" next to the memories: the answers the person kept at hand, in any conversation. */
export function PinsLine({ bot, onGo }: PinsLineProps) {
  const [open, setOpen] = useState(false)
  if (bot.pins.length === 0) return null
  const conversations = [{ id: 'main', title: bot.name, messages: bot.messages }, ...bot.threads]
  const items = bot.pins
    .map((pin) => {
      const where = conversations.find((c) => c.id === pin.thread)
      const message = where?.messages.find((m) => m.ms === pin.ms)
      const text = (message?.data.reply?.text ?? '').replace(/[`*#>]/g, '').split('\n').find((l) => l.trim()) ?? ''
      return message ? { pin, text, where: where?.title ?? '' } : null
    })
    .filter((item): item is { pin: { thread: string; ms: number }; text: string; where: string } => item !== null)
  if (items.length === 0) return null
  return (
    <div className="memorias">
      <button type="button" className="memorias-linha" aria-expanded={open} onClick={() => setOpen(!open)}>
        {t.pinnedCount.replace('{n}', String(items.length))}
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d={open ? 'M6 15l6-6 6 6' : 'M9 6l6 6-6 6'} />
        </svg>
      </button>
      {open && (
        <div className="memorias-lista">
          <ul>
            {items.map(({ pin, text, where }) => (
              <li key={`${pin.thread}-${pin.ms}`}>
                <button
                  type="button"
                  className="fixada-item"
                  onClick={() => {
                    setOpen(false)
                    onGo(pin)
                  }}
                >
                  <span className="memorias-tipo">{where}</span>
                  <span className="memorias-titulo">{text}</span>
                  <span className="memorias-data">{clock(pin.ms)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}
