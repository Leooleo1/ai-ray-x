import { lit } from '../../pixels'

interface PixelBarProps {
  /** Fill ratio from 0 to 1. */
  ratio: number
  /** Accent color (CSS value or var). */
  color: string
  cols?: number
  rows?: number
  label?: string
}

/** Horizontal field of pixels lit up to `ratio` and dithered past it (same language as Bars/Effort). */
export function PixelBar({ ratio, color, cols = 24, rows = 2, label }: PixelBarProps) {
  const clamped = Math.max(0, Math.min(1, ratio))
  const edge = clamped > 0 ? Math.max(1, Math.round(clamped * cols)) : 0
  return (
    <span
      className="efi-px-barra"
      style={{ ['--cor' as string]: color, gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {Array.from({ length: cols * rows }, (_, idx) => {
        const col = idx % cols
        const row = Math.floor(idx / cols)
        const light = Math.round(lit(col, row, edge) * 100)
        return <i key={idx} style={{ ['--luz' as string]: `${light}%` }} />
      })}
    </span>
  )
}

export interface SplitSegment {
  key: string
  label: string
  share: number
  color: string
}

interface PixelSplitBarProps {
  segments: SplitSegment[]
  cols?: number
  rows?: number
  label: string
}

/** Multi-segment pixel matrix representing a composition (e.g. cost shares across cache/input/output). */
export function PixelSplitBar({ segments, cols = 40, rows = 2, label }: PixelSplitBarProps) {
  const total = segments.reduce((sum, s) => sum + s.share, 0)
  const columns: { color: string; light: number }[] = []
  if (total > 0) {
    let cursor = 0
    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i]
      const count = i === segments.length - 1 ? Math.max(0, cols - cursor) : Math.max(1, Math.round((seg.share / total) * cols))
      const end = Math.min(cols, cursor + count)
      for (let c = cursor; c < end; c++) {
        const local = c - cursor
        const span = Math.max(1, end - cursor)
        // Brighter toward the right edge of each segment, using the same dither light range.
        const light = Math.round(lit(local, 0, span) * 100)
        columns.push({ color: seg.color, light })
      }
      cursor = end
    }
  }
  while (columns.length < cols) columns.push({ color: 'var(--nevoa)', light: 0 })

  return (
    <div
      className="efi-px-split"
      style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
      role="img"
      aria-label={label}
    >
      {Array.from({ length: cols * rows }, (_, idx) => {
        const col = idx % cols
        const row = Math.floor(idx / cols)
        const spec = columns[col]
        const rowLight = spec.light > 0 ? Math.max(35, spec.light - row * 12) : 0
        return <i key={idx} style={{ ['--cor' as string]: spec.color, ['--luz' as string]: `${rowLight}%` }} />
      })}
    </div>
  )
}

export interface PixelColumn {
  label: string
  ratio: number
  valueLabel: string
  title: string
  color: string
}

interface PixelHistogramProps {
  columns: PixelColumn[]
  rows?: number
  subCols?: number
  label: string
}

/** Vertical histogram rendered as fields of pixels lit bottom-to-top and dithered past the edge. */
export function PixelHistogram({ columns, rows = 9, subCols = 4, label }: PixelHistogramProps) {
  return (
    <div className="efi-px-hist" role="img" aria-label={label}>
      {columns.map((col) => {
        const edge = col.ratio > 0.005 ? Math.max(1, Math.round(col.ratio * rows)) : 0
        return (
          <div key={col.label} className="efi-px-col" title={col.title}>
            <span className="efi-px-col-pct">{col.valueLabel}</span>
            <div
              className="efi-px-col-grade"
              style={{
                ['--cor' as string]: col.color,
                gridTemplateColumns: `repeat(${subCols}, minmax(0, 1fr))`,
              }}
              aria-hidden="true"
            >
              {Array.from({ length: rows * subCols }, (_, idx) => {
                const rTop = Math.floor(idx / subCols)
                const rBottom = rows - 1 - rTop
                const c = idx % subCols
                const light = Math.round(lit(rBottom, c, edge) * 100)
                return <i key={idx} style={{ ['--luz' as string]: `${light}%` }} />
              })}
            </div>
            <span className="efi-px-col-rotulo">{col.label}</span>
          </div>
        )
      })}
    </div>
  )
}
