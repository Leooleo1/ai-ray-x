import { useState } from 'react'
import type { Bot, Routine } from '../api'
import type { Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import './controls.css'

const WEEKDAYS = [0, 1, 2, 3, 4, 5, 6]
const WORKDAYS = [0, 1, 2, 3, 4]

/** "Monday" for 0, in the reader's language (2026-11-16 was a Monday). */
const dayName = (d: number, style: 'short' | 'long') => new Date(2026, 10, 16 + d).toLocaleDateString(locale, { weekday: style }).replace('.', '')

function daysLabel(days: number[]): string {
  if (days.length === 7) return t.everyDay
  if (days.join() === WORKDAYS.join()) return t.workdays
  return days.map((d) => dayName(d, 'short')).join(', ')
}

const two = (n: number) => String(n).padStart(2, '0')

/** What the last run of a routine read, from its own conversation. */
function lastRead(bot: Bot, routine: Routine): number | null {
  const thread = bot.threads.find((x) => x.id === routine.thread)
  const answer = [...(thread?.messages ?? [])].reverse().find((m) => m.data.reply)
  return answer?.data.reply?.read_tokens ?? null
}

interface RoutinesEditorProps {
  bot: Bot
  bots: Bots
}

/** Questions on a schedule: they run only while raiox is open, each in its own conversation. */
export function RoutinesEditor({ bot, bots }: RoutinesEditorProps) {
  const [text, setText] = useState('')
  const [days, setDays] = useState<number[]>(WORKDAYS)
  const [time, setTime] = useState('09:00')
  // New routines skip days when nothing in the project changed.
  const [onlyIfChanged, setOnlyIfChanged] = useState(true)
  const toggleDay = (d: number) => setDays(days.includes(d) ? days.filter((x) => x !== d) : [...days, d].sort())
  const create = async () => {
    const [hour, minute] = time.split(':').map(Number)
    const done = await bots.act('routine_new', { id: bot.id, text, days, hour, minute, only_if_changed: onlyIfChanged })
    if (done) setText('')
  }
  return (
    <div className="rotinas">
      <p className="ajustes-nota">{t.routinesRule}</p>
      {bot.routines.length > 0 && (
        <ul className="rotinas-lista">
          {bot.routines.map((r) => {
            const read = lastRead(bot, r)
            const skipped = r.skipped_ms != null && r.skipped_ms === r.last_ms
            const last = r.last_ms ? new Date(r.last_ms).toLocaleString(locale, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : t.neverRan
            return (
              <li key={r.id}>
                <span className="rotina-texto">
                  <b>{r.text}</b>
                  <small>{`${daysLabel(r.days)} · ${two(r.hour)}:${two(r.minute)} · ${skipped ? t.skippedNothing : `${t.lastRun} ${last}${read ? ` · ${t.readWord} ${compact(read)}` : ''}`}`}</small>
                </span>
                <button
                  type="button"
                  className="rotina-modo"
                  title={t.onlyIfChanged}
                  aria-pressed={Boolean(r.only_if_changed)}
                  onClick={() => void bots.act('routine_changed', { id: bot.id, routine: r.id, on: !r.only_if_changed })}
                >
                  {r.only_if_changed ? t.routineIfChanged : t.routineAlways}
                </button>
                <button
                  type="button"
                  className={`config-chave ${r.enabled ? 'ligada' : ''}`}
                  role="switch"
                  aria-checked={r.enabled}
                  aria-label={r.text}
                  onClick={() => void bots.act('routine_toggle', { id: bot.id, routine: r.id, enabled: !r.enabled })}
                >
                  <i />
                </button>
                <button type="button" className="fechar-aba" aria-label={t.deleteRoutine} title={t.deleteRoutine} onClick={() => void bots.act('routine_delete', { id: bot.id, routine: r.id })}>
                  ×
                </button>
              </li>
            )
          })}
        </ul>
      )}
      <label className="ajustes-campo">
        <span>{t.routineAsk}</span>
        <input value={text} maxLength={500} placeholder={t.routineHint} onChange={(event) => setText(event.target.value)} />
      </label>
      <div className="rotina-quando">
        <div className="ajustes-grade" role="group" aria-label={t.routineDays} style={{ gridTemplateColumns: 'repeat(7, minmax(0, 1fr))' }}>
          {WEEKDAYS.map((d) => (
            <button key={d} type="button" aria-pressed={days.includes(d)} className={days.includes(d) ? 'ativo' : ''} onClick={() => toggleDay(d)}>
              <b>{dayName(d, 'short').slice(0, 3)}</b>
            </button>
          ))}
        </div>
        <input type="time" className="ajustes-modelo" value={time} aria-label={t.routineTime} onChange={(event) => setTime(event.target.value)} />
      </div>
      <label className="rotina-mudou">
        <button type="button" className={`config-chave ${onlyIfChanged ? 'ligada' : ''}`} role="switch" aria-checked={onlyIfChanged} onClick={() => setOnlyIfChanged(!onlyIfChanged)}>
          <i />
        </button>
        <span>{t.onlyIfChanged}</span>
      </label>
      <span className="acao">
        <button type="button" className="cheio" disabled={!text.trim() || days.length === 0} onClick={() => void create()}>
          {t.createRoutine}
        </button>
      </span>
    </div>
  )
}
