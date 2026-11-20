import type { CSSProperties } from 'react'
import { t } from '../i18n'
import './skeleton.css'

export type SkeletonShape = 'list' | 'usage' | 'rows'

interface SkeletonProps {
  /** list: rows with a round mark and two lines; rows: lines only; usage: a figure and a chart, then rows. */
  shape?: SkeletonShape
  rows?: number
}

/** Bar widths that vary from row to row without randomness, so a re-render does not jump. */
const width = (i: number, base: number, spread: number) => `${base + ((i * 37) % spread)}%`

/** What a place will hold while it loads: bars of faint dots in its shape, read by a slow beam. */
export function Skeleton({ shape = 'list', rows = 5 }: SkeletonProps) {
  return (
    <div className={`esqueleto ${shape}`} role="status" aria-live="polite" aria-label={t.loadingShort}>
      {shape === 'usage' && (
        <div className="esq-topo">
          <span>
            <i className="esq-barra numero" />
            <i className="esq-barra" style={{ width: '60%' }} />
          </span>
          <i className="esq-bloco" />
        </div>
      )}
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="esq-linha" style={{ '--i': i } as CSSProperties}>
          {shape !== 'rows' && <i className="esq-ponto" />}
          <span>
            <i className="esq-barra" style={{ width: width(i, 46, 40) }} />
            <i className="esq-barra fina" style={{ width: width(i + 2, 24, 36) }} />
          </span>
        </div>
      ))}
    </div>
  )
}
