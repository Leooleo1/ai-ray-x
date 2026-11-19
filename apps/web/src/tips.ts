// Floating tips from Raio, Dieta and Memo: worked out here from what the panel
// already has (no AI, no request), each shown once, each agent mutable.

import { useEffect, useState } from 'react'
import type { AgentId } from './agents'
import type { Bot, BotMessage, BotsState, Report } from './api'
import { suggestionTitle } from './diet'
import { compact, percent, t } from './i18n'
import { describeOutcome } from './outcome'

const PREFS_KEY = 'raiox.dicas'
const SEEN_KEY = 'raiox.dicas.vistas'
const CHANGED = 'raiox-dicas'
const MAX_SEEN = 300
/** An answer that read this much makes a fresh conversation worth it for short questions. */
const HEAVY_READ = 150_000

export type TipAction = { kind: 'message'; bot: string; thread: string; ms: number } | { kind: 'open'; bot: string } | { kind: 'house'; bot: string; agent: AgentId } | { kind: 'new-thread'; bot: string }

export interface Tip {
  key: string
  agent: AgentId
  text: string
  label: string
  action: TipAction
}

export type TipPrefs = Record<AgentId, boolean>

const DEFAULT_PREFS: TipPrefs = { raio: true, dieta: true, memo: true }

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}

export function readPrefs(): TipPrefs {
  return { ...DEFAULT_PREFS, ...readJson<Partial<TipPrefs>>(PREFS_KEY, {}) }
}

export function savePrefs(next: TipPrefs): void {
  localStorage.setItem(PREFS_KEY, JSON.stringify(next))
  window.dispatchEvent(new Event(CHANGED))
}

/** The person's choice of which agents may float tips, kept in step across the panel. */
export function useTipPrefs(): [TipPrefs, (next: TipPrefs) => void] {
  const [prefs, setPrefs] = useState(readPrefs)
  useEffect(() => {
    const sync = () => setPrefs(readPrefs())
    window.addEventListener(CHANGED, sync)
    return () => window.removeEventListener(CHANGED, sync)
  }, [])
  return [prefs, savePrefs]
}

export function seenTips(): Set<string> {
  return new Set(readJson<string[]>(SEEN_KEY, []))
}

export function markSeen(key: string): void {
  const kept = [...seenTips(), key].slice(-MAX_SEEN)
  localStorage.setItem(SEEN_KEY, JSON.stringify(kept))
}

const allMessages = (bot: Bot): BotMessage[] => [...bot.messages, ...bot.threads.flatMap((x) => x.messages)].sort((a, b) => a.ms - b.ms)

function raioTips(bots: Bot[]): Tip[] {
  return bots.flatMap((bot) => {
    const last = [...allMessages(bot)].reverse().find((m) => m.data.reply)
    const read = last?.data.reply?.read_tokens ?? 0
    if (!last || read < HEAVY_READ) return []
    return [{ key: `raio:${bot.id}:${last.ms}`, agent: 'raio', text: t.tipRaio.replace('{project}', bot.name).replace('{n}', compact(read)), label: t.tipNewThread, action: { kind: 'new-thread', bot: bot.id } }]
  })
}

function dietaTips(bots: Bot[], report: Report): Tip[] {
  const claude = report.sources.find((s) => s.overview.source === 'claude')
  const applicable = report.applicable.claude
  const best = claude?.suggestions.find((s) => applicable.includes(s.id))
  if (!best) return []
  // One project at a time: the first one where the biggest cut is not in place yet.
  const projects = [...new Map(bots.map((b) => [b.dir, b])).values()]
  const target = projects.find((bot) => !report.changes.some((c) => c.suggestion === best.id && c.undone_ms === null && c.file.startsWith(`${bot.dir}/`)))
  if (!target) return []
  const text = t.tipDieta.replace('{project}', target.name).replace('{share}', percent(best.saving_share)).replace('{cut}', suggestionTitle(best.id, best.title).toLowerCase())
  return [{ key: `dieta:${target.dir}:${best.id}`, agent: 'dieta', text, label: t.tipSeeCuts, action: { kind: 'house', bot: target.id, agent: 'dieta' } }]
}

function memoTips(bots: Bot[], report: Report): Tip[] {
  const unsent = bots.flatMap((bot) => {
    const messages = allMessages(bot)
    return messages.flatMap((m) =>
      (m.data.reply?.to ?? [])
        .filter((note) => !messages.some((s) => s.kind === 'sent' && s.data.answer_ms === m.ms && s.data.project === note.project))
        .map((note) => ({ key: `memo:to:${bot.id}:${m.ms}:${note.project}`, agent: 'memo' as const, text: t.tipMemoTo.replace('{from}', bot.name).replace('{to}', note.project), label: t.tipOpen, action: { kind: 'open' as const, bot: bot.id } })),
    )
  })
  const measured = report.outcomes
    .filter((o) => o.conclusive)
    .flatMap((o) => {
      const change = report.changes.find((c) => c.id === o.change_id)
      const bot = change && bots.find((b) => change.file.startsWith(`${b.dir}/`))
      if (!bot) return []
      const text = t.tipMemoResult.replace('{cut}', suggestionTitle(o.suggestion, o.suggestion).toLowerCase()).replace('{project}', bot.name).replace('{result}', describeOutcome(o))
      return [{ key: `memo:resultado:${o.change_id}`, agent: 'memo' as const, text, label: t.tipOpen, action: { kind: 'open' as const, bot: bot.id } }]
    })
  return [...unsent, ...measured]
}

const WEEK_MS = 7 * 86_400_000
/** A week this much heavier than the one before is worth a tip, past a floor that ignores tiny projects. */
const WEEK_JUMP = 2
const WEEK_FLOOR = 200_000

/** Raio's weekly report: a project that re-read twice as much as the week before. */
function weekTips(bots: Bot[]): Tip[] {
  const projects = [...new Map(bots.map((b) => [b.dir, b])).values()]
  const bucket = Math.floor(Date.now() / WEEK_MS)
  return projects.flatMap((bot) => {
    const week = bot.week
    if (!week || week.last < WEEK_FLOOR || week.this < WEEK_JUMP * week.last) return []
    const text = t.tipWeek.replace('{project}', bot.name).replace('{x}', (week.this / week.last).toFixed(1)).replace('{this}', compact(week.this)).replace('{tool}', week.top?.tool ?? '—')
    return [{ key: `raio:semana:${bot.dir}:${bucket}`, agent: 'raio' as const, text, label: t.tipOpen, action: { kind: 'house' as const, bot: bot.id, agent: 'raio' as const } }]
  })
}

/** What each routine answered on its latest run, once. */
function routineTips(bots: Bot[]): Tip[] {
  return bots.flatMap((bot) =>
    bot.routines.flatMap((routine) => {
      const thread = bot.threads.find((x) => x.id === routine.thread)
      const answer = thread?.messages.findLast((m) => m.author === 'bot' && m.data.reply && m.ms >= (routine.last_ms ?? 0))
      if (!thread || !answer?.data.reply) return []
      const line = answer.data.reply.text.replace(/[`*#>]/g, '').split('\n').find((l) => l.trim()) ?? ''
      const text = t.tipRoutine.replace('{project}', bot.name).replace('{text}', line.length > 140 ? `${line.slice(0, 140)}…` : line)
      return [{ key: `rotina:${routine.id}:${answer.ms}`, agent: 'memo' as const, text, label: t.tipOpen, action: { kind: 'message' as const, bot: bot.id, thread: thread.id, ms: answer.ms } }]
    }),
  )
}

/** The tips not seen yet, from the agents the person did not mute, oldest kind first. */
export function pendingTips(state: BotsState, report: Report, prefs: TipPrefs, seen: Set<string>): Tip[] {
  const bots = state.bots
  return [...routineTips(bots), ...memoTips(bots, report), ...weekTips(bots), ...raioTips(bots), ...dietaTips(bots, report)].filter((tip) => prefs[tip.agent] && !seen.has(tip.key))
}
