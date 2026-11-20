import { useEffect, useRef, useState } from 'react'
import type { Bot, BotMessage } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import type { ChatNav } from './nav'

interface CarryMenuProps {
  bot: Bot
  thread: string
  bots: Bots
  nav: ChatNav
}

/** "Take to…": this conversation's last exchanges go to another AI (a new conversation) or into a channel. */
export function CarryMenu({ bot, thread, bots, nav }: CarryMenuProps) {
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return undefined
    const away = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false)
    }
    const esc = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', esc)
    }
  }, [open])
  const others = bots.state.bots.filter((b) => b.id !== bot.id)
  const channels = bots.state.channels ?? []
  const toBot = async (target: Bot) => {
    setOpen(false)
    const next = await bots.act('carry', { id: bot.id, thread, to_bot: target.id })
    // The new conversation is the target's newest one.
    const made = next?.bots.find((b) => b.id === target.id)?.threads.reduce<string | null>((newest, x) => (!newest || x.id > newest ? x.id : newest), null)
    if (made) nav.openMessage(target.id, made, 0)
  }
  const toChannel = async (id: string) => {
    setOpen(false)
    if (await bots.act('carry', { id: bot.id, thread, to_channel: id })) nav.openChannel(id)
  }
  return (
    <div className="levar" ref={box}>
      <button type="button" className={`icone-btn ${open ? 'ativo' : ''}`} title={t.carryTo} aria-label={t.carryTo} aria-expanded={open} onClick={() => setOpen(!open)}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M14 4h6v6M20 4l-9 9M18 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10" />
        </svg>
      </button>
      {open && (
        <div className="levar-painel" role="menu">
          <p className="levar-nota">{t.carryHint}</p>
          {others.length === 0 && channels.length === 0 && <p className="levar-nota">{t.carryEmpty}</p>}
          {others.length > 0 && <span className="levar-grupo">{t.carryOtherAi}</span>}
          {others.map((other) => (
            <button key={other.id} type="button" role="menuitem" onClick={() => void toBot(other)}>
              <Avatar agent={faceForBot(other)} size={20} state="still" />
              <span>{other.name}</span>
              <small>{AGENT_LABEL[other.agent]}</small>
            </button>
          ))}
          {channels.length > 0 && <span className="levar-grupo">{t.channels}</span>}
          {channels.map((channel) => (
            <button key={channel.id} type="button" role="menuitem" onClick={() => void toChannel(channel.id)}>
              <span className="levar-cerquilha" aria-hidden="true">
                #
              </span>
              <span>{channel.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

/** Markdown marks read as noise in a short excerpt: the words stay, the marks go. */
const plain = (text: string) =>
  text
    .replace(/[`*]/g, '')
    .replace(/^(#+|>)\s?/gm, '')
    .replace(/\s+-\s+/g, ' · ')

/** Context the person brought from another conversation: folded, its exchanges on a tap. */
export function CarriedBlock({ message }: { message: BotMessage }) {
  const items = message.data.items ?? []
  const agent = message.data.agent ? AGENT_LABEL[message.data.agent] : ''
  return (
    <details className="trazido">
      <summary>
        {t.carriedFrom.replace('{p}', [message.data.project, agent].filter(Boolean).join(' · '))}
        <small>{t.carriedCount.replace('{n}', String(items.length))}</small>
      </summary>
      <ul>
        {items.map((item, index) => (
          <li key={index}>
            <b>{item.who === 'you' ? t.carriedYou : t.carriedAi}</b>
            <span>{plain(item.text)}</span>
          </li>
        ))}
      </ul>
    </details>
  )
}
