// Pieces the tutorial's scenes are built from: a timeline of beats, typed
// text, a cursor that walks to a marked element, and a miniature chat window
// in the panel's own look.

import { type ReactNode, type RefObject, useEffect, useLayoutEffect, useState } from 'react'
import type { Face } from '../avatars'
import { Avatar } from '../components/Avatar'

/** With reduced motion, scenes show their last beat and nothing moves. */
export const REDUCED_MOTION = window.matchMedia('(prefers-reduced-motion: reduce)').matches

/** The beat a scene is on: each time (ms from the start) moves it one beat ahead. */
export function useBeats(times: readonly number[]): number {
  const [beat, setBeat] = useState(REDUCED_MOTION ? times.length : 0)
  useEffect(() => {
    if (REDUCED_MOTION) return undefined
    const timers = times.map((ms, index) => window.setTimeout(() => setBeat(index + 1), ms))
    return () => timers.forEach((timer) => window.clearTimeout(timer))
  }, [times])
  return beat
}

const TYPE_MS = 34
const PARK_X = 0.82
const PARK_Y = 0.9

/** Text that types itself once `active` turns on. */
export function Typed({ text, active }: { text: string; active: boolean }) {
  const [shown, setShown] = useState(REDUCED_MOTION ? text.length : 0)
  useEffect(() => {
    if (!active || REDUCED_MOTION) return undefined
    const timer = window.setInterval(() => setShown((n) => (n >= text.length ? n : n + 1)), TYPE_MS)
    return () => window.clearInterval(timer)
  }, [active, text])
  return <>{active ? text.slice(0, shown) : ''}</>
}

interface CursorProps {
  stage: RefObject<HTMLDivElement | null>
  /** The `data-alvo` of the element to point at; null keeps it parked. */
  target: string | null
  pressed: boolean
}

/** A pointer that glides to the marked element and presses it. */
export function Cursor({ stage, target, pressed }: CursorProps) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null)
  useLayoutEffect(() => {
    const box = stage.current
    if (!box) return
    const outer = box.getBoundingClientRect()
    const element = target ? box.querySelector<HTMLElement>(`[data-alvo="${target}"]`) : null
    if (!element) {
      // Parked low on the right, where a hand rests, until there is something to point at.
      setAt((now) => now ?? { x: outer.width * PARK_X, y: outer.height * PARK_Y })
      return
    }
    const inner = element.getBoundingClientRect()
    setAt({ x: inner.left - outer.left + inner.width / 2, y: inner.top - outer.top + inner.height / 2 })
  }, [stage, target])
  if (REDUCED_MOTION) return null
  return (
    <span className={`tour-cursor ${at && target ? 'visivel' : ''} ${pressed ? 'apertado' : ''}`} style={at ? { transform: `translate(${at.x}px, ${at.y}px)` } : undefined} aria-hidden="true">
      <svg viewBox="0 0 24 24">
        <path d="M5 3l14 8.5-6.2 1.3L9.6 19z" />
      </svg>
    </span>
  )
}

interface WindowProps {
  face: Face
  project: string
  agent: string
  children: ReactNode
  /** The composer line at the bottom, when the scene shows one. */
  composer?: ReactNode
}

/** The chat as the panel draws it, at the size of the scene. */
export function Window({ face, project, agent, children, composer }: WindowProps) {
  return (
    <div className="tour-janela">
      <header>
        <Avatar agent={face} size={20} state="still" />
        <b>{project}</b>
        <small>{agent}</small>
      </header>
      <div className="tour-msgs">{children}</div>
      {composer}
    </div>
  )
}

/** Shows its children from a beat on, rising into place. */
export function From({ beat, at, children, className = '' }: { beat: number; at: number; children: ReactNode; className?: string }) {
  return <div className={`tour-surge ${beat >= at ? 'dentro' : ''} ${className}`}>{children}</div>
}

/** Code-ish words (file and function names) in mono. */
export function Rich({ text }: { text: string }) {
  const parts = text.split(/(\b[\w./]+\.(?:ts|js)\b|\b[a-z]+[A-Z]\w*\b)/g)
  return <>{parts.map((part, i) => (i % 2 === 1 ? <code key={i}>{part}</code> : part))}</>
}
