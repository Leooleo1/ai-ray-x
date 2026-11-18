import type { DayTotal } from '../model'
import { compact, locale, splitCompact, t, usd } from '../i18n'
import { useCountUp } from '../motion'

const ROWS = 8
const STEP = 10
const DOT = 3

interface SourceTotal {
  label: string
  tokens: number
}

interface HeroProps {
  total: number
  cost: number | null
  days: DayTotal[]
  streak: number
  bySource: SourceTotal[]
}

/** The signature: the hero number and the days share one dot grid, like an X-ray plate. */
export function Hero({ total, cost, days, streak, bySource }: HeroProps) {
  const shown = useCountUp(total)
  const { unit } = splitCompact(total, true)
  // Count inside the final scale, so the unit never changes mid-count.
  const value = (shown / scaleOf(total)).toLocaleString(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 })
  return (
    <section className="cartao heroi">
      <span className="rotulo">{t.tokens}</span>
      <div className="numeros">
        <span className="numero">
          {value}
          <span className="unidade">{unit}</span>
        </span>
        {cost !== null && (
          <span className="valor-api">
            <strong>{usd(cost)}</strong>
            <span>{t.apiValue}</span>
          </span>
        )}
      </div>
      <DotPlate days={days} />
      <div className="rodape">
        {bySource.map((s) => (
          <span key={s.label}>
            {s.label} <b>{compact(s.tokens)}</b>
          </span>
        ))}
        <span>
          <b>{streak}</b> {t.streak}
        </span>
      </div>
    </section>
  )
}

interface DotPlateProps {
  days: DayTotal[]
}

function DotPlate({ days }: DotPlateProps) {
  const max = Math.max(1, ...days.map((d) => d.tokens))
  const busiest = days.findIndex((d) => d.tokens === max)
  const dots = days.flatMap((day, col) => {
    const lit = day.tokens > 0 ? Math.max(1, Math.round(Math.sqrt(day.tokens / max) * ROWS)) : 0
    return Array.from({ length: ROWS }, (_, row) => {
      const on = ROWS - row <= lit
      const tone = !on ? 'apagado' : col === busiest ? 'pico' : 'aceso'
      return (
        <circle
          key={`${col}-${row}`}
          className={tone}
          style={on ? { animationDelay: `${col * 14 + (ROWS - row) * 30}ms` } : undefined}
          cx={col * STEP + STEP / 2}
          cy={row * STEP + STEP / 2}
          r={DOT}
        />
      )
    })
  })
  return (
    <svg className="chapa" viewBox={`0 0 ${days.length * STEP} ${ROWS * STEP}`} role="img" aria-label={`${days.length} ${t.days}`}>
      {dots}
    </svg>
  )
}

function scaleOf(n: number): number {
  return [1e9, 1e6, 1e3].find((size) => n >= size) ?? 1
}
