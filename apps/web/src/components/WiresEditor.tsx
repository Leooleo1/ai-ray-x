import type { Bots } from '../bots'
import { t } from '../i18n'

interface WiresEditorProps {
  bots: Bots
}

/** Every pair of projects with bots, as a switch: on means the two know of each other and can message. */
export function WiresEditor({ bots }: WiresEditorProps) {
  const projects = [...new Set(bots.state.bots.map((b) => b.name))].sort()
  const pairs = projects.flatMap((a, i) => projects.slice(i + 1).map((b) => [a, b] as const))
  const on = (a: string, b: string) => bots.state.links.some(([x, y]) => x === a && y === b)
  if (pairs.length === 0) return <p className="nota">{t.noWires}</p>
  return (
    <ul className="config-lista fios">
      {pairs.map(([a, b]) => (
        <li key={`${a}|${b}`} className="fio">
          <span className="fio-par">
            <b>{a}</b>
            <i className={`fio-linha ${on(a, b) ? 'ligado' : ''}`} aria-hidden="true" />
            <b>{b}</b>
          </span>
          <button
            type="button"
            role="switch"
            aria-checked={on(a, b)}
            aria-label={`${a} · ${b}`}
            className={`config-chave ${on(a, b) ? 'ligada' : ''}`}
            onClick={() => void bots.act('link_toggle', { a, b })}
          >
            <i />
            {on(a, b) ? t.turnedOn : t.turnOn}
          </button>
        </li>
      ))}
    </ul>
  )
}
