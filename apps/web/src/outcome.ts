// Before/after of an applied change, in one line.

import type { Outcome } from './api'
import { compact, locale, percent } from './i18n'

const MIN_REQUESTS = 200
const MIN_SESSIONS = 2
const pt = locale === 'pt-BR'

const per100 = (n: number): string => n.toLocaleString(locale, { maximumFractionDigits: 1 })

export function describeOutcome(o: Outcome): string {
  if (!o.conclusive) {
    const requests = Math.min(o.after.requests, MIN_REQUESTS)
    const sessions = Math.min(o.after.sessions, MIN_SESSIONS)
    return pt
      ? `medindo: ${requests} de ${MIN_REQUESTS} pedidos e ${sessions} de ${MIN_SESSIONS} sessões depois do ajuste`
      : `measuring: ${requests} of ${MIN_REQUESTS} requests and ${sessions} of ${MIN_SESSIONS} sessions since the change`
  }
  const change = `${o.context_change > 0 ? '+' : ''}${percent(o.context_change)}`
  return pt
    ? `contexto médio ${compact(o.before.avg_context)} → ${compact(o.after.avg_context)} (${change}) · compactações ${per100(o.before.compactions_per_100)} → ${per100(o.after.compactions_per_100)} a cada 100 pedidos`
    : `average context ${compact(o.before.avg_context)} → ${compact(o.after.avg_context)} (${change}) · compactions ${per100(o.before.compactions_per_100)} → ${per100(o.after.compactions_per_100)} per 100 requests`
}
