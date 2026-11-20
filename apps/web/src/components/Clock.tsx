import { useState } from 'react'
import { compact, t } from '../i18n'

const CENTER = 128
const INNER = 58
const MAX_LENGTH = 40
const LABEL_RADIUS = 118
const LABELS = [0, 6, 12, 18]

interface ClockProps {
  hours: number[]
}

/** Ring of 24 marks: each mark grows with the requests made in that hour. */
export function Clock({ hours }: ClockProps) {
  const [hovered, setHovered] = useState<number | null>(null)
  const max = Math.max(1, ...hours)
  const peak = hours.indexOf(Math.max(...hours))
  const point = (hour: number, radius: number) => {
    const angle = (hour / 24) * 2 * Math.PI - Math.PI / 2
    return { x: CENTER + Math.cos(angle) * radius, y: CENTER + Math.sin(angle) * radius }
  }
  const active = hovered ?? peak
  const activeCount = hours[active] ?? 0
  const subLabel =
    hovered === null
      ? t.peak
      : activeCount === 1
        ? t.requestsCountOne
        : t.requestsCount.replace('{n}', compact(activeCount))
  return (
    <section className="cartao relogio">
      <h2>{t.clock}</h2>
      <div className="anel">
        <svg viewBox="0 0 256 256" role="img" aria-label={`${t.peak} ${peak}h`} onMouseLeave={() => setHovered(null)}>
          {hours.map((value, hour) => {
            const from = point(hour, INNER)
            const to = point(hour, INNER + 6 + MAX_LENGTH * (value / max))
            const hitTo = point(hour, INNER + 8 + MAX_LENGTH)
            const isHovered = hovered === hour
            const isPeak = hour === peak
            return (
              <g key={hour} className="marca-grupo" onMouseEnter={() => setHovered(hour)}>
                <line
                  className={isHovered ? 'marca-ativa' : isPeak ? 'marca-pico' : 'marca'}
                  x1={from.x}
                  y1={from.y}
                  x2={to.x}
                  y2={to.y}
                  pathLength={1}
                  style={{
                    animationDelay: `${hour * 28}ms`,
                    ...(isHovered || isPeak ? {} : { opacity: hovered !== null ? 0.22 : 0.3 + 0.7 * (value / max) }),
                  }}
                />
                <line className="marca-hit" x1={from.x} y1={from.y} x2={hitTo.x} y2={hitTo.y} />
              </g>
            )
          })}
          {LABELS.map((hour) => {
            const at = point(hour, LABEL_RADIUS)
            return (
              <text key={hour} className="rotulo-hora" x={at.x} y={at.y} textAnchor="middle" dominantBaseline="central">
                {hour}
              </text>
            )
          })}
          <text className="pico-valor" x={CENTER} y={CENTER - 4} textAnchor="middle">
            {active}h
          </text>
          <text className="pico-rotulo" x={CENTER} y={CENTER + 18} textAnchor="middle">
            {subLabel}
          </text>
        </svg>
      </div>
    </section>
  )
}
