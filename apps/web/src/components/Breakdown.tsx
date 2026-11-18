import type { CompactionModel } from '../api'
import { compact, locale, t } from '../i18n'

const per100 = (n: number): string => n.toLocaleString(locale, { maximumFractionDigits: 1 })

interface BreakdownProps {
  model: CompactionModel
}

/** Today versus compacting at the chosen point: what each request re-reads,
 *  how often it summarizes, and what is re-read after each summary. */
export function CompactionBreakdown({ model }: BreakdownProps) {
  const best = model.best
  if (!best) return null
  return (
    <dl className="comparativo">
      <dt />
      <dd className="cabeca">{t.today}</dd>
      <dd className="cabeca">
        {t.compactingAt} {compact(best.cap)}
      </dd>
      <dt>{t.rereadPerRequest}</dt>
      <dd>{compact(model.now_avg_context)}</dd>
      <dd className="novo">{compact(best.avg_context)}</dd>
      <dt>{t.summariesPer100}</dt>
      <dd>{per100(model.now_compactions_per_100)}</dd>
      <dd className="novo">{per100(best.compactions_per_100)}</dd>
      <dt>{t.afterSummary}</dt>
      <dd>—</dd>
      <dd className="novo">
        {compact(model.floor)} + {compact(model.refill)}
      </dd>
    </dl>
  )
}
