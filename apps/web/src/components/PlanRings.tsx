import { useEffect, useState } from 'react'
import { type PlanLimits, type PlanWindow, fetchLimits } from '../api'
import { AGENT_LABEL } from '../avatars'
import { locale, t } from '../i18n'
import { AgentLogo } from './AgentLogo'
import './plan-rings.css'

/** The plans are read again this often (a turn may have used some). */
const POLL_MS = 60_000
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

/** How much of each AI's plan is used: the five hours outside, the week inside. */
export function PlanRings({ collapsed }: { collapsed: boolean }) {
  const [limits, setLimits] = useState<PlanLimits>({})
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
  const agents = (['claude', 'codex'] as const).filter((agent) => limits[agent])
  if (agents.length === 0) return null
  return (
    <div className={`aneis ${collapsed ? 'recolhidos' : ''}`} aria-label={t.planTitle}>
      {agents.map((agent) => {
        const plan = limits[agent]!
        const tip = t.planTip
          .replace('{ai}', AGENT_LABEL[agent])
          .replace('{h}', String(Math.round(plan.five_hour.used)))
          .replace('{hr}', when(plan.five_hour.resets_at))
          .replace('{w}', String(Math.round(plan.week.used)))
          .replace('{wr}', when(plan.week.resets_at))
        return (
          <span key={agent} className="anel" title={plan.old ? `${tip} ${t.planOld}` : tip} role="img" aria-label={tip}>
            <svg viewBox={`0 0 ${SIZE} ${SIZE}`} width={SIZE} height={SIZE} aria-hidden="true">
              <Arc radius={OUTER} part={plan.five_hour} />
              <Arc radius={INNER} part={plan.week} />
            </svg>
            <span className="anel-rosto">
              <AgentLogo agent={agent} size={15} />
            </span>
          </span>
        )
      })}
    </div>
  )
}
