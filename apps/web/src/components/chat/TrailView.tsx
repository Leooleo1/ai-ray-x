import { useState } from 'react'
import type { Trail, TrailItem } from '../../api'
import { t } from '../../i18n'

/** A long path starts folded on its last steps; the rest opens on a tap. */
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
  const folds = !open && trail.items.length > FOLDED_SHOWS + 2
  const hidden = folds ? trail.items.length - FOLDED_SHOWS : 0
  const shown = trail.items.slice(hidden)
  const before = trail.dropped + hidden
  return (
    <ol className="trilha">
      {before > 0 && (
        <li className="trilha-antes">
          {folds ? (
            <button type="button" className="ver-mais" onClick={() => setOpen(true)}>
              {t.trailEarlier.replace('{n}', String(before))}
            </button>
          ) : (
            <small>{t.trailDropped.replace('{n}', String(trail.dropped))}</small>
          )}
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
    </ol>
  )
}
