import { type CSSProperties, type PointerEvent, useEffect, useRef, useState } from 'react'
import { t } from '../i18n'
import './effort.css'

const COLS = 44
const ROWS = 5
/** Columns past the edge where the light breaks into dither, like a glow fading out. */
const EDGE = 4
/** Ordered 4×4 dither thresholds, 0 to 1. */
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((n) => (n + 0.5) / 16)

interface EffortMeterProps {
  label: string
  /** The bot's own level; none follows the CLI's default. */
  current: string | null
  fallback: string | null
  /** The CLI's levels, weakest first. */
  options: string[]
  /** Picking the default level gives null (back to the default). */
  onPick: (value: string | null) => void
  color: string
}

/** A cell's light: lit up to the level, dithered past it, brighter toward the edge. */
function lit(col: number, row: number, edge: number): number {
  if (col < edge) return 0.35 + 0.65 * (col / Math.max(1, edge))
  const past = col - edge
  if (past >= EDGE) return 0
  return BAYER[(row % 4) * 4 + (col % 4)] > (past + 1) / (EDGE + 1) ? 0.3 : 0
}

/** Effort as a field of pixels that lights up to the chosen level; the field itself is the control. */
export function EffortMeter({ label, current, fallback, options, onPick, color }: EffortMeterProps) {
  const chosen = current ?? fallback
  const saved = Math.max(0, options.indexOf(chosen ?? ''))
  // While dragging, the field follows the pointer; the choice is saved on release.
  const [dragged, setDragged] = useState<number | null>(null)
  const field = useRef<HTMLDivElement>(null)
  useEffect(() => setDragged(null), [current])
  const level = dragged ?? saved
  const edge = Math.round(((level + 1) / options.length) * COLS)

  const levelAt = (x: number) => {
    const box = field.current?.getBoundingClientRect()
    if (!box) return level
    const share = Math.min(0.999, Math.max(0, (x - box.left) / box.width))
    return Math.floor(share * options.length)
  }
  const pick = (index: number) => {
    setDragged(index)
    const value = options[index]
    if (value !== chosen) onPick(value === fallback ? null : value)
  }
  const onDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId)
    setDragged(levelAt(event.clientX))
  }
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) setDragged(levelAt(event.clientX))
  }
  const onUp = (event: PointerEvent<HTMLDivElement>) => pick(levelAt(event.clientX))

  return (
    <div className="esforco" style={{ '--esforco-cor': color } as CSSProperties}>
      <div className="esforco-topo">
        <span>{label}</span>
        <b>
          {options[level]}
          {options[level] === fallback && <small>{t.defaultOption.toLowerCase()}</small>}
        </b>
      </div>
      <div
        ref={field}
        className="esforco-campo"
        role="slider"
        tabIndex={0}
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={options.length - 1}
        aria-valuenow={level}
        aria-valuetext={options[level]}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onKeyDown={(event) => {
          if (event.key === 'ArrowRight' || event.key === 'ArrowUp') pick(Math.min(options.length - 1, level + 1))
          if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') pick(Math.max(0, level - 1))
        }}
      >
        {Array.from({ length: ROWS * COLS }, (_, cell) => {
          const col = cell % COLS
          const row = Math.floor(cell / COLS)
          return <i key={cell} style={{ '--luz': `${Math.round(lit(col, row, edge) * 100)}%`, transitionDelay: `${col * 7}ms` } as CSSProperties} />
        })}
      </div>
      <div className="esforco-niveis" style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}>
        {options.map((option, index) => (
          <button key={option} type="button" className={`${index === level ? 'ativo' : ''} ${option === fallback ? 'padrao' : ''}`} tabIndex={-1} onClick={() => pick(index)}>
            {option}
          </button>
        ))}
      </div>
    </div>
  )
}
