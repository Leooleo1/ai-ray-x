import { useLayoutEffect, useRef, useState } from 'react'
import type { UsageDay } from '../../api'
import { locale, t } from '../../i18n'
import { money } from './money'

const MIN_WIDTH = 320
const HEIGHT = 220
const LEFT = 64
const BOTTOM = 26
const TOP = 8
const TICKS = 4
const LABELS = 4

interface Series {
  key: string
  color: string
}

interface CostChartProps {
  days: UsageDay[]
  series: Series[]
}

const dayLabel = (iso: string) =>
  new Date(`${iso}T12:00:00`).toLocaleDateString(locale, { day: 'numeric', month: 'short' }).replace(/\./g, '').replace(' de ', ' ')

/** A round step for the scale: 1, 2 or 5 times a power of ten. */
function niceStep(max: number): number {
  const rough = max / TICKS
  const power = 10 ** Math.floor(Math.log10(rough || 1))
  return [1, 2, 5, 10].map((m) => m * power).find((step) => step >= rough) ?? power * 10
}

/** Each AI's cost per day as a line with a faint fill, on a ruler in dollars. */
export function CostChart({ days, series }: CostChartProps) {
  const [hovered, setHovered] = useState<number | null>(null)
  const box = useRef<HTMLElement>(null)
  const [WIDTH, setWidth] = useState(MIN_WIDTH)
  // Drawn at the size it shows, so the ruler's text never stretches.
  useLayoutEffect(() => {
    const el = box.current
    if (!el) return undefined
    const measure = () => setWidth(Math.max(MIN_WIDTH, el.clientWidth))
    measure()
    if (typeof ResizeObserver === 'undefined') return undefined
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])
  const peak = Math.max(0, ...days.flatMap((day) => series.map((s) => day.usd[s.key] ?? 0)))
  const step = niceStep(peak)
  const top = Math.max(step, Math.ceil(peak / step) * step)
  const plotW = WIDTH - LEFT
  const plotH = HEIGHT - BOTTOM - TOP
  const x = (i: number) => LEFT + (days.length <= 1 ? plotW / 2 : (i / (days.length - 1)) * plotW)
  const y = (v: number) => TOP + plotH - (v / top) * plotH
  const ticks = Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step)
  const labelEvery = Math.max(1, Math.ceil(days.length / LABELS))
  const active = hovered !== null ? days[hovered] : undefined
  return (
    <figure className="uso-grafico" ref={box}>
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={t.usageChart} onMouseLeave={() => setHovered(null)}>
        {ticks.map((tick) => (
          <g key={tick}>
            <line className="uso-regua" x1={LEFT} x2={WIDTH} y1={y(tick)} y2={y(tick)} />
            <text className="uso-eixo" x={LEFT - 10} y={y(tick) + 4} textAnchor="end">
              {money(tick, tick > 0 && tick < 10 ? 2 : 0)}
            </text>
          </g>
        ))}
        {days.map((day, i) =>
          i % labelEvery === 0 || i === days.length - 1 ? (
            <text key={day.date} className="uso-eixo" x={x(i)} y={HEIGHT - 6} textAnchor={i === 0 ? 'start' : i === days.length - 1 ? 'end' : 'middle'}>
              {dayLabel(day.date)}
            </text>
          ) : null,
        )}
        {series.map((s) => {
          const points = days.map((day, i) => `${x(i)},${y(day.usd[s.key] ?? 0)}`)
          return (
            <g key={s.key}>
              <polygon points={`${x(0)},${y(0)} ${points.join(' ')} ${x(days.length - 1)},${y(0)}`} fill={s.color} opacity={0.08} />
              <polyline points={points.join(' ')} fill="none" stroke={s.color} strokeWidth={1.6} strokeLinejoin="round" />
            </g>
          )
        })}
        {active && hovered !== null && <line className="uso-mira" x1={x(hovered)} x2={x(hovered)} y1={TOP} y2={TOP + plotH} />}
        {days.map((day, i) => (
          <rect
            key={day.date}
            x={x(i) - plotW / Math.max(1, days.length) / 2}
            y={TOP}
            width={plotW / Math.max(1, days.length)}
            height={plotH}
            fill="transparent"
            onMouseEnter={() => setHovered(i)}
          />
        ))}
      </svg>
      <figcaption className="uso-dia">
        {active ? (
          <>
            <b>{dayLabel(active.date)}</b>
            {series.map((s) => (
              <span key={s.key}>
                <i style={{ background: s.color }} />
                {money(active.usd[s.key] ?? 0)}
              </span>
            ))}
          </>
        ) : (
          <span>{t.usageChartHint}</span>
        )}
      </figcaption>
    </figure>
  )
}
