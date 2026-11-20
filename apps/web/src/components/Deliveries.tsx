import { useCallback, useEffect, useState } from 'react'
import { type Bot, type Delivery, fetchDeliveries } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import { compact, locale, t } from '../i18n'
import { Avatar } from './Avatar'
import type { ChatNav } from './chat/nav'
import { ShotBlock } from './chat/ShotBlock'
import { SelectChip } from './SelectChip'
import './desk.css'
import './deliveries.css'

const DAY_MS = 86_400_000
const KIND_ORDER = ['change', 'draft', 'shot', 'routine', 'task'] as const

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
  const [refreshing, setRefreshing] = useState(false)
  const [kind, setKind] = useState<string>('all')
  const [project, setProject] = useState<string>('all')
  const [error, setError] = useState<string | null>(null)
  const load = useCallback((silent = false) => {
    if (!silent) setRefreshing(true)
    setError(null)
    return fetchDeliveries()
      .then((loaded) => setItems(loaded))
      .catch((failure: unknown) => setError(messageOf(failure)))
      .finally(() => setRefreshing(false))
  }, [])
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
  if (!items) {
    return (
      <main className="mesa entregas">
        <section className="mesa-secao">
          <p className="mesa-vazio">{t.deliveriesLoading}</p>
        </section>
      </main>
    )
  }
  const presentKinds = KIND_ORDER.filter((k) => items.some((item) => item.kind === k))
  const projectNames = [...new Set(items.map((item) => bots.find((b) => b.id === item.bot)?.name).filter((n): n is string => Boolean(n)))].sort((a, b) => a.localeCompare(b))
  const filtered = items.filter((item) => {
    if (kind !== 'all' && item.kind !== kind) return false
    if (project !== 'all') {
      const bot = bots.find((b) => b.id === item.bot)
      if (!bot || bot.name !== project) return false
    }
    return true
  })
  return (
    <main className="mesa entregas">
      {items.length > 0 && (
        <header className="entregas-filtros">
          {presentKinds.length > 1 && (
            <div className="entregas-pills" role="tablist" aria-label={t.deliveriesTitle}>
              <button type="button" role="tab" aria-selected={kind === 'all'} className={kind === 'all' ? 'ativo' : ''} onClick={() => setKind('all')}>
                {t.deliveryFilterAll}
              </button>
              {presentKinds.map((k) => (
                <button key={k} type="button" role="tab" aria-selected={kind === k} className={kind === k ? 'ativo' : ''} onClick={() => setKind(k)}>
                  {t.deliveryKinds[k]}
                </button>
              ))}
            </div>
          )}
          <div className="entregas-controles">
            {projectNames.length > 1 && (
              <SelectChip
                label={t.projects}
                prefix={t.projects}
                value={project}
                align="right"
                options={[{ value: 'all', label: t.deliveryFilterAll }, ...projectNames.map((name) => ({ value: name, label: name }))]}
                onChange={setProject}
              />
            )}
            <button type="button" className="vidro-btn entregas-atualizar" disabled={refreshing} onClick={() => void load()}>
              {refreshing ? t.wikiLoading : t.deliveryRefresh}
            </button>
          </div>
        </header>
      )}
      {items.length === 0 && (
        <section className="mesa-secao">
          <p className="mesa-vazio">{t.deliveriesEmpty}</p>
        </section>
      )}
      {items.length > 0 && filtered.length === 0 && (
        <section className="mesa-secao">
          <p className="mesa-vazio">{t.deliveriesNoMatch}</p>
        </section>
      )}
      {byDay(filtered).map((day) => (
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
