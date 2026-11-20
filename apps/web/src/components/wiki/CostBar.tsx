import type { CSSProperties } from 'react'
import { lit } from '../../pixels'

/** Pixels across and down: a thin strip under a page's name. */
const COLS = 28
const ROWS = 2
/** A page read at all shows at least this many lit columns. */
const MIN_LIT = 2

interface CostBarProps {
  /** What reading this page cost this week, in tokens. */
  tokens: number
  /** The most any page cost this week: the bar's full length. */
  most: number
}

/** A page's weekly reading cost as a strip of pixels, lit up to its share of the costliest page. */
export function CostBar({ tokens, most }: CostBarProps) {
  const edge = tokens > 0 && most > 0 ? Math.max(MIN_LIT, Math.round((tokens / most) * COLS)) : 0
  return (
    <span className="wiki-custo" aria-hidden="true">
      {Array.from({ length: COLS * ROWS }, (_, cell) => (
        <i key={cell} style={{ '--luz': `${Math.round(lit(cell % COLS, Math.floor(cell / COLS), edge) * 100)}%` } as CSSProperties} />
      ))}
    </span>
  )
}
