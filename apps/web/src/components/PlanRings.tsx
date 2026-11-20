import { useEffect, useState } from 'react'
import { type PlanLimits, type PlanWindow, fetchLimits } from '../api'
import { AGENT_LABEL } from '../avatars'
import { locale, t } from '../i18n'
import { AgentLogo } from './AgentLogo'
import './plan-rings.css'

/** The plans are read again this often (a turn may have used some). */
const POLL_MS = 15_000
const SIZE = 40
const OUTER = 17
const INNER = 12
/** Past these shares of a window, its ring warns, then alarms. */
const WARN = 70
const ALARM = 90

const level = (used: number) => (used >= ALARM ? 'alarme' : used >= WARN ? 'aviso' : 'ok')

const when = (seconds: number | null) =>
  seconds ? new Date(seconds * 1000).toLocaleString(locale, { weekday: 'short', hour: '2-digit', minute: '2-digit' }) : ''

/** One window as an arc that fills clockwise from the top. */
function Arc({ radius, part }: { radius: number; part: PlanWindow }) {
  const length = 2 * Math.PI * radius
  return (
    <>
      <circle className="anel-trilho" cx={SIZE / 2} cy={SIZE / 2} r={radius} />
      <circle
        className={`anel-uso ${level(part.used)}`}
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={radius}
        strokeDasharray={`${(part.used / 100) * length} ${length}`}
        transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}
      />
    </>
  )
}

/** "3 min ago", from a reading's time in seconds. */
function ago(seconds: number): string {
  const minutes = Math.max(0, Math.round((Date.now() / 1000 - seconds) / 60))
  if (minutes < 1) return t.planNow
  if (minutes < 60) return t.planMinutes.replace('{n}', String(minutes))
  const hours = Math.round(minutes / 60)
  return hours < 48 ? t.planHours.replace('{n}', String(hours)) : t.planDays.replace('{n}', String(Math.round(hours / 24)))
}

/** One window as a line: its name, a bar, the share used and when it starts again. */
function WindowLine({ label, part }: { label: string; part: PlanWindow }) {
  return (
    <div className="anel-janela">
      <span>{label}</span>
      <i className="anel-barra">
        <i className={level(part.used)} style={{ width: `${Math.min(100, part.used)}%` }} />
      </i>
      <b>{`${Math.round(part.used)}%`}</b>
      <small>{!part.resets_at ? '' : part.resets_at * 1000 <= Date.now() ? t.planRenewed : t.planResets.replace('{w}', when(part.resets_at))}</small>
    </div>
  )
}

/** How much of each AI's plan is used: the five hours outside, the week inside. Pointing or clicking opens the detail. */
export function PlanRings({ collapsed }: { collapsed: boolean }) {
  const [limits, setLimits] = useState<PlanLimits>({})
  const [hovered, setHovered] = useState(false)
  const [pinned, setPinned] = useState(false)
  useEffect(() => {
    let alive = true
    const load = () => fetchLimits().then((next) => alive && setLimits(next)).catch(() => undefined)
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [])
  useEffect(() => {
    if (!pinned) return undefined
    const away = (event: PointerEvent) => !(event.target as Element).closest('.aneis') && setPinned(false)
    const escape = (event: KeyboardEvent) => event.key === 'Escape' && setPinned(false)
    window.addEventListener('pointerdown', away)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('pointerdown', away)
      window.removeEventListener('keydown', escape)
    }
  }, [pinned])
  const agents = (['claude', 'codex'] as const).filter((agent) => limits[agent])
  if (agents.length === 0) return null
  const open = hovered || pinned
  return (
    <div className={`aneis ${collapsed ? 'recolhidos' : ''}`} onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}>
      {agents.map((agent) => {
        const plan = limits[agent]!
        return (
          <button key={agent} type="button" className="anel" aria-label={`${t.planTitle}: ${AGENT_LABEL[agent]}`} aria-expanded={open} onClick={() => setPinned(!pinned)}>
            <svg viewBox={`0 0 ${SIZE} ${SIZE}`} width={SIZE} height={SIZE} aria-hidden="true">
              <Arc radius={OUTER} part={plan.five_hour} />
              <Arc radius={INNER} part={plan.week} />
            </svg>
            <span className="anel-rosto">
              <AgentLogo agent={agent} size={15} />
            </span>
          </button>
        )
      })}
      {open && (
        <div className="aneis-painel" role="dialog" aria-label={t.planTitle}>
          {agents.map((agent) => {
            const plan = limits[agent]!
            return (
              <section key={agent}>
                <header>
                  <AgentLogo agent={agent} size={14} />
                  <b>{plan.plan ? `${AGENT_LABEL[agent]} · ${plan.plan}` : AGENT_LABEL[agent]}</b>
                  <small>{t.planRead.replace('{w}', ago(plan.read_at))}</small>
                </header>
                <WindowLine label={t.planFiveHours} part={plan.five_hour} />
                <WindowLine label={t.planWeek} part={plan.week} />
                <p>{agent === 'claude' ? t.planFromClaude : plan.live ? t.planFromCodexLive : t.planFromCodex}</p>
              </section>
            )
          })}
        </div>
      )}
    </div>
  )
}
