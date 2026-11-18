import { t } from '../i18n'

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
  const max = Math.max(1, ...hours)
  const peak = hours.indexOf(Math.max(...hours))
  const point = (hour: number, radius: number) => {
    const angle = (hour / 24) * 2 * Math.PI - Math.PI / 2
    return { x: CENTER + Math.cos(angle) * radius, y: CENTER + Math.sin(angle) * radius }
  }
  return (
    <section className="cartao relogio">
      <h2>{t.clock}</h2>
      <div className="anel">
        <svg viewBox="0 0 256 256" role="img" aria-label={`${t.peak} ${peak}h`}>
          {hours.map((value, hour) => {
            const from = point(hour, INNER)
            const to = point(hour, INNER + 6 + MAX_LENGTH * (value / max))
            return (
              <line
                key={hour}
                className={hour === peak ? 'marca-pico' : 'marca'}
                x1={from.x}
                y1={from.y}
                x2={to.x}
                y2={to.y}
                pathLength={1}
                style={{ animationDelay: `${hour * 28}ms`, ...(hour === peak ? {} : { opacity: 0.3 + 0.7 * (value / max) }) }}
              />
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
            {peak}h
          </text>
          <text className="pico-rotulo" x={CENTER} y={CENTER + 18} textAnchor="middle">
            {t.peak}
          </text>
        </svg>
      </div>
    </section>
  )
}
