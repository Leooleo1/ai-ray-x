// Pure functions that turn the report into what each card draws.

import type { Report, Source, SourceReport } from './api'

export interface DayTotal {
  date: string
  tokens: number
}

export interface Row {
  name: string
  source: Source
  value: number
  share?: number
}

const MAX_PLATE_DAYS = 180
const DAY_MS = 86_400_000
const HEAVY_CONTEXT = 400_000

export function totalTokens(source: SourceReport): number {
  return source.overview.models.reduce((sum, m) => sum + m.input + m.cache_read + m.cache_write + m.output, 0)
}

/** Every day between the first and last active day (empty days included), capped to the latest 180. */
export function plateDays(report: Report): DayTotal[] {
  const byDate = new Map<string, number>()
  for (const { overview } of report.sources) {
    for (const day of overview.daily) {
      byDate.set(day.date, (byDate.get(day.date) ?? 0) + day.tokens)
    }
  }
  const dates = [...byDate.keys()].sort()
  if (dates.length === 0) return []
  const start = Date.parse(`${dates[0]}T12:00:00Z`)
  const end = Date.parse(`${dates[dates.length - 1]}T12:00:00Z`)
  const all = Array.from({ length: Math.round((end - start) / DAY_MS) + 1 }, (_, i) => {
    const date = new Date(start + i * DAY_MS).toISOString().slice(0, 10)
    return { date, tokens: byDate.get(date) ?? 0 }
  })
  return all.slice(-MAX_PLATE_DAYS)
}

export function longestStreak(days: DayTotal[]): number {
  return days.reduce(
    (acc, day) => {
      const run = day.tokens > 0 ? acc.run + 1 : 0
      return { run, best: Math.max(acc.best, run) }
    },
    { run: 0, best: 0 },
  ).best
}

export function hours(report: Report): number[] {
  return Array.from({ length: 24 }, (_, h) => report.sources.reduce((sum, s) => sum + (s.overview.by_hour[h] ?? 0), 0))
}

export function reportedCost(report: Report): number | null {
  const costs = report.sources.map((s) => s.overview.reported_cost_usd).filter((c): c is number => c !== null)
  return costs.length > 0 ? costs.reduce((a, b) => a + b, 0) : null
}

/** Share of requests whose context is above 400K tokens. */
export function heavyShare(source: SourceReport): number {
  return source.efficiency.buckets
    .filter((b) => b.up_to === null || b.up_to > HEAVY_CONTEXT)
    .reduce((sum, b) => sum + b.share_of_calls, 0)
}

export function rereadShare(source: SourceReport): number {
  return source.efficiency.cost_shares.find(([name]) => name === 'cache_read')?.[1] ?? 0
}

/** What stayed loaded, as a share of all context re-read by every agent (one common base). */
export function carriedRows(report: Report, limit: number): Row[] {
  const context = report.sources.reduce((sum, s) => sum + s.efficiency.context.total, 0) || 1
  return report.sources
    .flatMap((s) =>
      s.efficiency.carried.map((c) => ({
        name: c.name,
        source: s.overview.source,
        value: c.carried_tokens,
        share: c.carried_tokens / context,
      })),
    )
    .sort((a, b) => b.value - a.value)
    .slice(0, limit)
}

export function projectRows(report: Report, limit: number): Row[] {
  const merged = new Map<string, Row>()
  for (const { overview } of report.sources) {
    for (const [path, tokens] of overview.projects) {
      const name = path.split('/').pop() ?? path
      const current = merged.get(name)
      const source = current && current.value > tokens ? current.source : overview.source
      merged.set(name, { name, source, value: (current?.value ?? 0) + tokens })
    }
  }
  return [...merged.values()].sort((a, b) => b.value - a.value).slice(0, limit)
}

export function secretTotals(report: Report): Record<string, number> {
  return report.sources
    .flatMap((s) => s.secrets)
    .reduce<Record<string, number>>((acc, [kind, count]) => ({ ...acc, [kind]: (acc[kind] ?? 0) + count }), {})
}
