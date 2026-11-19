import { useEffect, useState } from 'react'
import { type Bot, type Delivery, fetchDeliveries } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import { compact, locale, t } from '../i18n'
import { Avatar } from './Avatar'
import type { ChatNav } from './chat/nav'
import { ShotBlock } from './chat/ShotBlock'
import './desk.css'
import './deliveries.css'

const DAY_MS = 86_400_000

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** "Hoje", "Ontem", or the date, for a day's heading. */
function dayLabel(ms: number): string {
  const start = new Date().setHours(0, 0, 0, 0)
  if (ms >= start) return t.today
  if (ms >= start - DAY_MS) return t.yesterday
  return new Date(ms).toLocaleDateString(locale, { weekday: 'short', day: 'numeric', month: 'short' })
}

const hour = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })

/** Deliveries by day, newest first. */
function byDay(items: Delivery[]): { label: string; items: Delivery[] }[] {
  return items.reduce<{ label: string; items: Delivery[] }[]>((days, item) => {
    const label = dayLabel(item.ms)
    const last = days[days.length - 1]
    return last?.label === label ? [...days.slice(0, -1), { label, items: [...last.items, item] }] : [...days, { label, items: [item] }]
  }, [])
}

interface DeliveriesProps {
  bots: Bot[]
  nav: ChatNav
}

/** What the bots produced, in one place: changes, screenshots, routine results, finished tasks. */
export function Deliveries({ bots, nav }: DeliveriesProps) {
  const [items, setItems] = useState<Delivery[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    fetchDeliveries()
      .then((loaded) => alive && setItems(loaded))
      .catch((failure: unknown) => alive && setError(messageOf(failure)))
    return () => {
      alive = false
    }
  }, [])
  if (error) return <p className="nota failed quebra">{error}</p>
  if (!items) return null
  return (
    <main className="mesa entregas">
      {items.length === 0 && (
        <section className="mesa-secao">
          <p className="mesa-vazio">{t.deliveriesEmpty}</p>
        </section>
      )}
      {byDay(items).map((day) => (
        <section key={day.label} className="mesa-secao">
          <h2>
            {day.label}
            <small>{day.items.length}</small>
          </h2>
          <ul className="entregas-lista">
            {day.items.map((item) => {
              const bot = bots.find((b) => b.id === item.bot)
              if (!bot) return null
              return (
                <li key={`${item.bot}-${item.thread}-${item.ms}-${item.kind}`} className={item.undone ? 'desfeita' : ''}>
                  <Avatar agent={faceForBot(bot)} size={22} state="still" />
                  <span className="entregas-corpo">
                    <span className="entregas-meta">
                      <span className={`entregas-tipo ${item.kind}`}>{t.deliveryKinds[item.kind]}</span>
                      <small>{[bot.name, AGENT_LABEL[bot.agent], hour(item.ms), item.tokens ? `${compact(item.tokens)} tokens` : '', item.undone ? t.deliveryUndone : ''].filter(Boolean).join(' · ')}</small>
                    </span>
                    <b>{item.kind === 'shot' ? t.deliveryShotOf.replace('{target}', item.title) : item.title}</b>
                    {item.files && item.files.length > 0 && item.kind !== 'draft' && <code>{item.files.join('  ')}</code>}
                    {item.kind === 'routine' && item.detail && <p>{item.detail}</p>}
                    {item.kind === 'shot' && item.detail && <ShotBlock bot={bot} message={{ ms: item.ms, author: 'you', kind: 'shot', data: { name: item.detail } }} />}
                  </span>
                  <button type="button" className="vidro-btn" onClick={() => nav.openMessage(item.bot, item.thread, item.ms)}>
                    {t.deliveryOpen}
                  </button>
                </li>
              )
            })}
          </ul>
        </section>
      ))}
    </main>
  )
}
