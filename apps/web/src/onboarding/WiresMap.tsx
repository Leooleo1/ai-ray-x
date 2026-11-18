import type { Bot } from '../api'
import { faceForBot } from '../avatars'
import { Avatar } from '../components/Avatar'

/** Where two projects sit, in % of the map: side by side. */
const PAIR: [number, number][] = [
  [27, 46],
  [73, 46],
]
const RADIUS_X = 34
const RADIUS_Y = 36

/** Three projects or more sit on a ring, the first at the top. */
function ring(count: number): [number, number][] {
  if (count === 2) return PAIR
  return Array.from({ length: count }, (_, i) => {
    const angle = -Math.PI / 2 + (2 * Math.PI * i) / count
    return [50 + RADIUS_X * Math.cos(angle), 48 + RADIUS_Y * Math.sin(angle)]
  })
}

interface WiresMapProps {
  bots: Bot[]
  links: [string, string][]
}

/** The person's projects as a map: every pair a faint line, the wired ones lit. */
export function WiresMap({ bots, links }: WiresMapProps) {
  const projects = [...new Map(bots.map((bot) => [bot.name, bot])).values()].sort((a, b) => a.name.localeCompare(b.name))
  const points = ring(projects.length)
  const pairs = projects.flatMap((a, i) => projects.slice(i + 1).map((b, k) => ({ a, b, i, j: i + 1 + k })))
  const lit = (a: string, b: string) => links.some(([x, y]) => (x === a && y === b) || (x === b && y === a))
  return (
    <div className="bv-mapa" aria-hidden="true">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none">
        {pairs.map(({ a, b, i, j }) => (
          <line
            key={`${a.name}|${b.name}`}
            x1={points[i][0]}
            y1={points[i][1]}
            x2={points[j][0]}
            y2={points[j][1]}
            className={lit(a.name, b.name) ? 'ligado' : ''}
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </svg>
      {projects.map((bot, i) => (
        <span key={bot.name} className="bv-no" style={{ left: `${points[i][0]}%`, top: `${points[i][1]}%` }}>
          <Avatar agent={faceForBot(bot)} size={44} state="still" />
          <b>{bot.name}</b>
        </span>
      ))}
    </div>
  )
}
