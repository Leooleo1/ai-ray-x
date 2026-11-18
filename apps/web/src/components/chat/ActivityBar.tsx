import { useEffect, useState } from 'react'
import type { Face } from '../../avatars'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'

const TICK_MS = 1000

interface ActivityBarProps {
  face: Face
  speaker: string
  doing: string
  since: number
}

/** A bare line right above the composer while the bot works: its face, what it does, for how long. */
export function ActivityBar({ face, speaker, doing, since }: ActivityBarProps) {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => window.clearInterval(timer)
  }, [])
  return (
    <div className="atividade" role="status">
      <Avatar agent={face} size={18} state="thinking" />
      <span className="atividade-texto">
        <b style={{ color: face.color }}>{speaker}</b> {doing}
      </span>
      <span className="digitando" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
      <span className="atividade-tempo">{`${Math.max(0, Math.round((now - since) / 1000))} s · ${t.escToStop}`}</span>
    </div>
  )
}
