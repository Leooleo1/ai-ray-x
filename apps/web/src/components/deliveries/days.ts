import type { Delivery } from '../../api'
import { locale, t } from '../../i18n'

const DAY_MS = 86_400_000
/** The days the rhythm shows, today included. */
export const RHYTHM_DAYS = 14
/** The days "this week" counts, today included. */
export const WEEK_DAYS = 7

/** A day by its local midnight. */
export const dayStart = (ms: number): number => new Date(ms).setHours(0, 0, 0, 0)

/** The last `count` days, oldest first (from noon, so a clock change does not skip one). */
export function lastDays(count: number, now = Date.now()): number[] {
  const noon = new Date(now).setHours(12, 0, 0, 0)
  return Array.from({ length: count }, (_, index) => dayStart(noon - (count - 1 - index) * DAY_MS))
}

/** "Today", "Yesterday", or the date, for a day's heading. */
export function dayLabel(start: number): string {
  const [yesterday, today] = lastDays(2)
  if (start === today) return t.today
  if (start === yesterday) return t.yesterday
  return new Date(start).toLocaleDateString(locale, { weekday: 'short', day: 'numeric', month: 'short' })
}

export interface Day {
  start: number
  items: Delivery[]
}

/** Deliveries by day, in the order they come (newest first). */
export function byDay(items: Delivery[]): Day[] {
  return items.reduce<Day[]>((days, item) => {
    const start = dayStart(item.ms)
    const last = days[days.length - 1]
    return last?.start === start ? [...days.slice(0, -1), { start, items: [...last.items, item] }] : [...days, { start, items: [item] }]
  }, [])
}

export type Tone = 'codigo' | 'feita' | 'outra' | 'desfeita'

/** The colour a delivery takes: code it changed, a task it finished, or something it showed. */
export function toneOf(item: Delivery): Tone {
  if (item.undone) return 'desfeita'
  if (item.kind === 'change' || item.kind === 'draft') return 'codigo'
  if (item.kind === 'task') return 'feita'
  return 'outra'
}

export const hourOf = (ms: number): string => new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
