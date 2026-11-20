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

/** Each hour's projects that used it most, all sources together, biggest first. */
export function hourProjects(report: Report): [string, number][][] {
  return Array.from({ length: 24 }, (_, h) => {
    const sums = new Map<string, number>()
    for (const s of report.sources) for (const [project, tokens] of s.overview.by_hour_projects?.[h] ?? []) sums.set(project, (sums.get(project) ?? 0) + tokens)
    return [...sums.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3)
  })
}

/** Share of the busiest hour under which an hour counts as free. */
const FREE_SHARE = 0.1
const MIN_FREE = 3
const MAX_FREE = 8
const FALLBACK_FREE = 4

/** The person's longest quiet stretch (hours with little use, past midnight too), for the AIs to work in. */
export function freeWindow(hours: number[]): { start: number; end: number } {
  const max = Math.max(1, ...hours)
  const quiet = hours.map((v) => v <= max * FREE_SHARE)
  let best = { start: 0, length: 0 }
  for (let start = 0; start < 24; start++) {
    if (!quiet[start] || quiet[(start + 23) % 24]) continue
    let length = 0
    while (length < 24 && quiet[(start + length) % 24]) length++
    if (length > best.length) best = { start, length }
  }
  if (best.length >= MIN_FREE) {
    const length = Math.min(best.length, MAX_FREE)
    return { start: best.start, end: (best.start + length) % 24 }
  }
  // No quiet stretch: the hours used least, a few in a row.
  let least = { start: 0, sum: Infinity }
  for (let start = 0; start < 24; start++) {
    const sum = Array.from({ length: FALLBACK_FREE }, (_, i) => hours[(start + i) % 24]).reduce((a, b) => a + b, 0)
    if (sum < least.sum) least = { start, sum }
  }
  return { start: least.start, end: (least.start + FALLBACK_FREE) % 24 }
}

/** How many hours a window holds. */
export function windowLength(w: { start: number; end: number }): number {
  return (w.end - w.start + 24) % 24
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
