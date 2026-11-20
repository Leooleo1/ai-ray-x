import { useEffect, useState } from 'react'
import { type Notice, onNotice } from '../notify'
import './notices.css'

/** How long a note stays. */
const SHOWN_MS = 2600
/** Most notes on screen at once; an older one gives way. */
const MAX_SHOWN = 3

const ICON = {
  ok: 'M5 12.5l4.5 4.5L19 7.5',
  undo: 'M9 8L5 12l4 4M5 12h9.5a4.5 4.5 0 0 1 0 9H12',
}

/** The notes at the foot of the panel: each one draws its mark, says one thing and goes. */
export function Notices() {
  const [shown, setShown] = useState<Notice[]>([])
  useEffect(
    () =>
      onNotice((notice) => {
        setShown((was) => [...was.slice(-(MAX_SHOWN - 1)), notice])
        window.setTimeout(() => setShown((was) => was.filter((n) => n.id !== notice.id)), SHOWN_MS)
      }),
    [],
  )
  return (
    <div className="avisos-pilha" role="status" aria-live="polite">
      {shown.map((notice) => (
        <p key={notice.id} className={`aviso-nota ${notice.kind}`}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d={ICON[notice.kind]} pathLength={100} />
          </svg>
          {notice.text}
        </p>
      ))}
    </div>
  )
}
