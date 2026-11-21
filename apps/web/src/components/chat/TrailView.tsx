import { useRef, useState } from 'react'
import type { Trail, TrailItem } from '../../api'
import { t } from '../../i18n'

/** A long path starts folded on its last steps. */
const FOLDED_SHOWS = 6

/** What a step got back: lines read, files found or changed, or an output's last lines. */
function result(item: TrailItem): string {
  if (item.count != null) {
    const [one, many] = item.kind === 'read' ? [t.trailLine, t.trailLines] : [t.trailFile, t.trailFiles]
    return (item.count === 1 ? one : many).replace('{n}', String(item.count))
  }
  return item.detail ?? ''
}

/** The path of a turn: each step with its result, and what the AI said between steps. */
export function TrailView({ trail, live }: { trail: Trail; live: boolean }) {
  const [open, setOpen] = useState(false)
  const toggle = useRef<HTMLButtonElement>(null)
  const foldable = trail.items.length > FOLDED_SHOWS + 2
  const hidden = !open && foldable ? trail.items.length - FOLDED_SHOWS : 0
  const shown = trail.items.slice(hidden)
  return (
    <ol className="trilha">
      {foldable && (
        <li className="trilha-antes">
          <button ref={toggle} type="button" className="ver-mais" aria-expanded={open} onClick={() => setOpen((expanded) => !expanded)}>
            {open ? t.trailRecent : t.trailEarlier.replace('{n}', String(hidden))}
          </button>
        </li>
      )}
      {hidden === 0 && trail.dropped > 0 && (
        <li className="trilha-antes">
          <small>{t.trailDropped.replace('{n}', String(trail.dropped))}</small>
        </li>
      )}
      {shown.map((item, index) =>
        item.kind === 'say' ? (
          <li key={index} className="trilha-fala">
            {item.target}
          </li>
        ) : (
          <li key={index} className="trilha-passo">
            <i className={`trilha-ponto ${item.ok === true ? 'ok' : item.ok === false ? 'erro' : live ? 'andando' : ''}`} aria-hidden="true" />
            <span className="trilha-acao">{t.stepWords[item.kind] ?? item.kind}</span>
            {item.target && <code>{item.target}</code>}
            {result(item) && <small>{result(item)}</small>}
          </li>
        ),
      )}
      {live && foldable && open && (
        <li>
          <button type="button" className="ver-mais trilha-recolher" onClick={() => {
            setOpen(false)
            toggle.current?.focus()
          }}>
            {t.trailRecent}
          </button>
        </li>
      )}
    </ol>
  )
}
