import type { HourWindow } from '../api'

const SIZE = 220
const CENTER = SIZE / 2
const INNER = 46
const MAX_LENGTH = 32
const ARC = 92
const LABEL = 106
const LABELS = [0, 6, 12, 18]

export interface RingPin {
  id: string
  /** Hour of the day with its minutes, 9.5 for 09:30. */
  at: number
  label: string
}

interface ClockRingProps {
  hours: number[]
  /** The free hours: dashed while only suggested, solid once the AIs work in them. */
  window: HourWindow
  windowOn: boolean
  pins: RingPin[]
  now: number
  selected: number | null
  center: { value: string; label: string }
  /** Pointing at an hour (none when the pointer leaves). */
  onHover: (hour: number | null) => void
  /** Clicking an hour opens it. */
  onPick: (hour: number) => void
}

const point = (hour: number, radius: number) => {
  const angle = (hour / 24) * 2 * Math.PI - Math.PI / 2
  return { x: CENTER + Math.cos(angle) * radius, y: CENTER + Math.sin(angle) * radius }
}

/** The arc of a window, from the start of its first hour to the start of the hour after it. */
function arcPath(window: HourWindow): string {
  const length = (window.end - window.start + 24) % 24
  const from = point(window.start, ARC)
  const to = point(window.start + length, ARC)
  return `M ${from.x} ${from.y} A ${ARC} ${ARC} 0 ${length > 12 ? 1 : 0} 1 ${to.x} ${to.y}`
}

/** Two layers on one dial: the person's use (marks inside) and the AIs' hours (the arc and its pins outside). */
export function ClockRing({ hours, window, windowOn, pins, now, selected, center, onHover, onPick }: ClockRingProps) {
  const max = Math.max(1, ...hours)
  const peak = hours.indexOf(Math.max(...hours))
  const hand = point(now, ARC)
  return (
    <svg className="mostrador" viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label={`${center.value} ${center.label}`} onMouseLeave={() => onHover(null)}>
      <circle className="mostrador-trilho" cx={CENTER} cy={CENTER} r={ARC} />
      <path className={`mostrador-janela ${windowOn ? 'ligada' : ''}`} d={arcPath(window)} pathLength={1} />
      {hours.map((value, hour) => {
        const from = point(hour, INNER)
        const to = point(hour, INNER + 6 + MAX_LENGTH * (value / max))
        const dim = selected !== null && selected !== hour
        return (
          <line
            key={hour}
            pathLength={1}
            className={`mostrador-marca ${hour === peak ? 'pico' : ''} ${selected === hour ? 'escolhida' : ''}`}
            x1={from.x}
            y1={from.y}
            x2={to.x}
            y2={to.y}
            style={{ animationDelay: `${hour * 24}ms`, opacity: dim ? 0.18 : hour === peak ? 1 : 0.3 + 0.7 * (value / max) }}
          />
        )
      })}
      {LABELS.map((hour) => {
        const at = point(hour, LABEL)
        return (
          <text key={hour} className="mostrador-hora" x={at.x} y={at.y} textAnchor="middle" dominantBaseline="central">
            {hour}
          </text>
        )
      })}
      {pins.map((pin) => {
        const at = point(pin.at, ARC)
        return (
          <circle key={pin.id} className="mostrador-pino" cx={at.x} cy={at.y} r={4.5}>
            <title>{pin.label}</title>
          </circle>
        )
      })}
      <circle className="mostrador-agora" cx={hand.x} cy={hand.y} r={2.5} />
      {/* One wide, invisible spoke per hour: hovering or clicking it opens that hour. */}
      {hours.map((_, hour) => {
        const from = point(hour, 30)
        const to = point(hour, ARC + 8)
        return <line key={hour} className="mostrador-alvo" x1={from.x} y1={from.y} x2={to.x} y2={to.y} onMouseEnter={() => onHover(hour)} onClick={() => onPick(hour)} />
      })}
      <text className="mostrador-valor" x={CENTER} y={CENTER - 2} textAnchor="middle">
        {center.value}
      </text>
      <text className="mostrador-rotulo" x={CENTER} y={CENTER + 18} textAnchor="middle">
        {center.label}
      </text>
    </svg>
  )
}
