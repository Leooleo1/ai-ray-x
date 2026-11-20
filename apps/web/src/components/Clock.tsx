import { useEffect, useState } from 'react'
import { type Bot, type TaskBoardState, fetchTasks, taskAction } from '../api'
import { compact, t } from '../i18n'
import { freeWindow, windowLength } from '../model'
import { ClockRing, type RingPin } from './ClockRing'
import './clock.css'

/** Routines listed beside the dial; the rest are counted. */
const SHOWN_ROUTINES = 3
const NOW_EVERY_MS = 60_000

interface ClockProps {
  hours: number[]
  /** Each hour's projects that used it most: [project, tokens]. */
  hourProjects: [string, number][][]
  bots: Bot[]
  onOpenBot: (id: string) => void
}

const two = (n: number) => String(n).padStart(2, '0')
/** A project as the Projects card names it: its folder, or the home folder's own name. */
const projectName = (path: string) => (path === '~' ? t.home : (path.split('/').pop() ?? path))
const nowHour = () => {
  const d = new Date()
  return d.getHours() + d.getMinutes() / 60
}

interface ActiveRoutine {
  id: string
  bot: Bot
  hour: number
  minute: number
}

function activeRoutines(bots: Bot[]): ActiveRoutine[] {
  return bots
    .flatMap((bot) => (bot.routines ?? []).filter((r) => r.enabled).map((r) => ({ id: r.id, bot, hour: r.hour, minute: r.minute })))
    .sort((a, b) => a.hour * 60 + a.minute - (b.hour * 60 + b.minute))
}

/** When the person uses their AIs, and when the AIs work for them: the free hours, the Desk and the routines. */
export function Clock({ hours, hourProjects, bots, onOpenBot }: ClockProps) {
  const [board, setBoard] = useState<TaskBoardState | null>(null)
  const [selected, setSelected] = useState<number | null>(null)
  const [pinned, setPinned] = useState<number | null>(null)
  const [now, setNow] = useState(nowHour)
  useEffect(() => {
    fetchTasks().then(setBoard).catch(() => setBoard(null))
    const timer = window.setInterval(() => setNow(nowHour()), NOW_EVERY_MS)
    return () => window.clearInterval(timer)
  }, [])

  const suggested = freeWindow(hours)
  const chosen = board?.window ?? null
  const working = Boolean(board?.auto && chosen)
  const shown = working && chosen ? chosen : suggested
  const routines = activeRoutines(bots)
  const pins: RingPin[] = routines.map((r) => ({ id: r.id, at: r.hour + r.minute / 60, label: `${two(r.hour)}:${two(r.minute)} · ${r.bot.name}` }))
  const hour = selected ?? pinned
  const center =
    hour === null
      ? { value: `${windowLength(shown)}h`, label: t.clockFree }
      : { value: `${hour}h`, label: t.clockRequests.replace('{n}', compact(hours[hour] ?? 0)) }

  const act = (body: Record<string, unknown>) => void taskAction('auto', body).then(setBoard)
  const desk = board?.auto ? (chosen ? t.clockDeskOn : t.clockDeskAny) : t.clockDeskOff

  return (
    <section className="cartao relogio">
      <header className="relogio-topo">
        <h2>{t.clock}</h2>
        <span className="relogio-agora">{`${t.clockNow} ${two(Math.floor(now))}:${two(Math.floor((now % 1) * 60))}`}</span>
      </header>
      <div className="relogio-corpo">
        <ClockRing
          hours={hours}
          window={shown}
          windowOn={working}
          pins={pins}
          now={now}
          selected={hour}
          center={center}
          onHover={setSelected}
          onPick={setPinned}
        />
        {pinned !== null ? (
          <HourDetail hour={pinned} projects={hourProjects[pinned] ?? []} onClose={() => setPinned(null)} />
        ) : (
          <div className="relogio-lado">
            <span className="relogio-rotulo">{t.clockFreeLabel}</span>
            <b className="relogio-janela">{`${two(shown.start)}h → ${two(shown.end)}h`}</b>
            <span className={`relogio-mesa ${working ? 'ligada' : ''}`}>{desk}</span>
            <span className="relogio-rotulo relogio-rotinas-titulo">{t.clockRoutines}</span>
            {routines.length === 0 ? (
              <span className="relogio-vazio">{t.clockNoRoutines}</span>
            ) : (
              <ul className="relogio-rotinas">
                {routines.slice(0, SHOWN_ROUTINES).map((r) => (
                  <li key={r.id}>
                    <button type="button" onClick={() => onOpenBot(r.bot.id)}>
                      <time>{`${two(r.hour)}:${two(r.minute)}`}</time>
                      <span>{r.bot.name}</span>
                    </button>
                  </li>
                ))}
                {routines.length > SHOWN_ROUTINES && <li className="relogio-vazio">{t.clockMore.replace('{n}', String(routines.length - SHOWN_ROUTINES))}</li>}
              </ul>
            )}
            {working ? (
              <button type="button" className="relogio-parar" onClick={() => act({ on: false })}>
                {t.clockStop}
              </button>
            ) : (
              <button type="button" className="relogio-usar" disabled={!board} onClick={() => act({ on: true, window: suggested })}>
                {t.clockUseFree}
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  )
}

interface HourDetailProps {
  hour: number
  projects: [string, number][]
  onClose: () => void
}

/** One hour opened: the projects that used it most. */
function HourDetail({ hour, projects, onClose }: HourDetailProps) {
  const total = projects.reduce((sum, [, tokens]) => sum + tokens, 0)
  return (
    <div className="relogio-lado">
      <span className="relogio-rotulo">{t.clockAt.replace('{h}', String(hour))}</span>
      {projects.length === 0 ? (
        <span className="relogio-vazio">{t.clockNothing}</span>
      ) : (
        <ul className="relogio-projetos">
          {projects.map(([name, tokens]) => (
            <li key={name}>
              <span>{projectName(name)}</span>
              <b>{compact(tokens)}</b>
              <i style={{ width: `${Math.max(6, (tokens / Math.max(1, total)) * 100)}%` }} aria-hidden="true" />
            </li>
          ))}
        </ul>
      )}
      <button type="button" className="relogio-parar" onClick={onClose}>
        {t.clockClose}
      </button>
    </div>
  )
}
