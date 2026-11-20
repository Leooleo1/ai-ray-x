import type { UsageSlice } from '../../api'
import { compact, percent, t } from '../../i18n'
import { money } from './money'

interface UsageTableProps {
  name: string
  rows: UsageSlice[]
  total: number
  colorOf: (row: UsageSlice) => string | undefined
}

/** One row per model, AI, project or day: what it would cost, its share and its tokens. */
export function UsageTable({ name, rows, total, colorOf }: UsageTableProps) {
  return (
    <div className="uso-tabela" role="table">
      <div className="uso-linha cabeca" role="row">
        <span role="columnheader">{name}</span>
        <span role="columnheader">{t.usageCost}</span>
        <span role="columnheader">{t.usageShare}</span>
        <span role="columnheader">{t.usageTokens}</span>
      </div>
      <div className="uso-corpo">
        {rows.map((row) => {
          const color = colorOf(row)
          return (
            <div key={row.key} className="uso-linha" role="row">
              <span role="cell" className="uso-nome" title={row.key}>
                {color && <i style={{ background: color }} />}
                {row.label ?? row.key}
              </span>
              <span role="cell">{row.usd === null ? <small>{t.usageNoPrice}</small> : money(row.usd)}</span>
              <span role="cell">{row.usd === null || total <= 0 ? '—' : percent(row.usd / total)}</span>
              <span role="cell">{compact(row.tokens)}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
