import { Skeleton } from './Skeleton'
import { useEffect, useState } from 'react'
import { type Bot, type Delivery, fetchDeliveries } from '../api'
import { t } from '../i18n'
import type { ChatNav } from './chat/nav'
import { DeliveryRow } from './deliveries/DeliveryRow'
import { byDay, dayLabel, dayStart } from './deliveries/days'
import { Rhythm } from './deliveries/Rhythm'
import { SelectChip } from './SelectChip'
import './desk.css'
import './deliveries/entregas.css'

const KIND_ORDER = ['change', 'draft', 'shot', 'routine', 'audit', 'task'] as const
/** How often the page reads the conversations again (an AI may deliver while it is open). */
const POLL_MS = 15_000
const ALL = 'all'

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** The deliveries, read now and every few seconds after; the last list stays when a read fails. */
function useDeliveries(): { items: Delivery[] | null; error: string | null } {
  const [items, setItems] = useState<Delivery[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    const load = () =>
      fetchDeliveries()
        .then((loaded) => {
          if (!alive) return
          setItems(loaded)
          setError(null)
        })
        .catch((failure: unknown) => alive && setError(messageOf(failure)))
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [])
  return { items, error }
}

interface DeliveriesProps {
  bots: Bot[]
  nav: ChatNav
}

/** What the AIs produced, in one place: the last two weeks at a glance, then each day's deliveries on a rail. */
export function Deliveries({ bots, nav }: DeliveriesProps) {
  const { items, error } = useDeliveries()
  const [kind, setKind] = useState<string>(ALL)
  const [project, setProject] = useState<string>(ALL)
  const [day, setDay] = useState<number | null>(null)
  const botOf = (id: string) => bots.find((b) => b.id === id)

  if (!items) {
    return (
      <main className="mesa entregas">
        {error ? <p className="nota failed quebra">{error}</p> : <Skeleton rows={6} />}
      </main>
    )
  }
  const projectNames = [...new Set(items.map((item) => botOf(item.bot)?.name).filter((n): n is string => Boolean(n)))].sort((a, b) => a.localeCompare(b))
  const ofProject = items.filter((item) => project === ALL || botOf(item.bot)?.name === project)
  const kinds = KIND_ORDER.filter((k) => ofProject.some((item) => item.kind === k))
  const shown = ofProject.filter((item) => (kind === ALL || item.kind === kind) && (day === null || dayStart(item.ms) === day))
  const days = byDay(shown)

  return (
    <main className="mesa entregas">
      {error && <p className="nota failed quebra">{error}</p>}
      {items.length > 0 && (
        <header className="entregas-topo">
          <div className="entregas-abas" role="tablist" aria-label={t.deliveriesTitle}>
            {[ALL, ...kinds].map((k) => (
              <button key={k} type="button" role="tab" aria-selected={kind === k} className={kind === k ? 'ativo' : ''} onClick={() => setKind(k)}>
                {k === ALL ? t.deliveryFilterAll : t.deliveryTabs[k]}
                <small>{k === ALL ? ofProject.length : ofProject.filter((item) => item.kind === k).length}</small>
              </button>
            ))}
          </div>
          {projectNames.length > 1 && (
            <SelectChip
              label={t.projects}
              prefix={t.projects}
              value={project}
              align="right"
              options={[{ value: ALL, label: t.deliveryFilterAll }, ...projectNames.map((name) => ({ value: name, label: name }))]}
              onChange={setProject}
            />
          )}
        </header>
      )}
      <Rhythm items={ofProject} selected={day} onPick={setDay} />
      <section className="entregas-diario" aria-label={t.deliveriesTitle}>
        {items.length === 0 && (
          <div className="entregas-vazio-arte">
            <img className="arte-vazia pelucia" src="/mascotes/vazio-entregas.webp" alt="" />
            <p className="entregas-vazio">{t.deliveriesEmpty}</p>
          </div>
        )}
        {items.length > 0 && shown.length === 0 && <p className="entregas-vazio">{t.deliveriesNoMatch}</p>}
        {days.map(({ start, items: dayItems }) => (
          <div key={start} className="entregas-dia">
            <h3>
              {dayLabel(start)}
              <small>{dayItems.length}</small>
              {day === start && (
                <button type="button" className="acao-texto" onClick={() => setDay(null)}>
                  {t.deliveriesAllDays}
                </button>
              )}
            </h3>
            <ol>
              {dayItems.map((item) => {
                const bot = botOf(item.bot)
                return bot ? <DeliveryRow key={`${item.bot}-${item.thread}-${item.ms}-${item.kind}`} item={item} bot={bot} nav={nav} /> : null
              })}
            </ol>
          </div>
        ))}
      </section>
    </main>
  )
}
