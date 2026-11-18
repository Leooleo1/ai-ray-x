// Exchanges between two projects' bots through ai-memory, put together from
// the bots' own chats: what one suggested for the other, what the person
// sent, what the other received and answered. A project may have more than
// one bot (Claude Code and Cursor); the exchange is between projects.

import type { Bot, BotMessage } from './api'

/** The subject raiox gives a message it sends: "raiox · <sending project>". */
const SUBJECT = /^raiox · (.+)$/

export type ExchangeItem =
  | { kind: 'suggested'; ms: number; from: string; bot: Bot; text: string }
  | { kind: 'sent'; ms: number; from: string; to: string }
  | { kind: 'received'; ms: number; from: string; to: string; bot: Bot; text: string }
  | { kind: 'answered'; ms: number; from: string; bot: Bot; text: string; proposal: string | null }

export interface Exchange {
  /** The two projects, in the order of the first message. */
  a: string
  b: string
  items: ExchangeItem[]
  lastMs: number
  /** Sent but not yet handed to the other bot. */
  waiting: number
}

export const exchangeKey = (a: string, b: string) => [a, b].sort().join('|')

/** The answer a bot gave right after an ask (skipping notes the panel wrote). */
function answerAfter(messages: BotMessage[], index: number): BotMessage | undefined {
  for (let i = index + 1; i < messages.length; i += 1) {
    const m = messages[i]
    if (m.author === 'bot' && m.kind === 'answer') return m
    if (m.author === 'you' && m.kind === 'ask') return undefined
  }
  return undefined
}

function itemsOf(bots: Bot[]): Map<string, { pair: [string, string]; items: ExchangeItem[] }> {
  const projects = new Set(bots.map((b) => b.name))
  const byPair = new Map<string, { pair: [string, string]; items: ExchangeItem[] }>()
  const add = (a: string, b: string, item: ExchangeItem) => {
    if (a === b || !projects.has(a) || !projects.has(b)) return
    const key = exchangeKey(a, b)
    const known = byPair.get(key)
    byPair.set(key, { pair: known?.pair ?? [a, b], items: [...(known?.items ?? []), item] })
  }
  for (const bot of bots) {
    bot.messages.forEach((m, index) => {
      for (const note of m.data.reply?.to ?? []) {
        add(bot.name, note.project, { kind: 'suggested', ms: m.ms, from: bot.name, bot, text: note.text })
      }
      if (m.kind === 'sent' && m.data.project) {
        add(bot.name, m.data.project, { kind: 'sent', ms: m.ms, from: bot.name, to: m.data.project })
      }
      const from = m.data.via === 'ai-memory' ? m.data.text?.match(SUBJECT)?.[1] : undefined
      if (from) {
        add(from, bot.name, { kind: 'received', ms: m.ms, from, to: bot.name, bot, text: m.data.body ?? '' })
        const answer = answerAfter(bot.messages, index)
        if (answer?.data.reply) {
          add(from, bot.name, { kind: 'answered', ms: answer.ms, from: bot.name, bot, text: answer.data.reply.text, proposal: answer.data.reply.proposal })
        }
      }
    })
  }
  return byPair
}

/** Every pair of projects that exchanged something, newest first. */
export function exchanges(bots: Bot[]): Exchange[] {
  return [...itemsOf(bots).values()]
    .map(({ pair, items }) => {
      const sorted = [...items].sort((x, y) => x.ms - y.ms)
      // Whoever spoke first opens the exchange; an answer is never first, it follows what it answers.
      const first = sorted[0]
      const a = first.kind === 'answered' ? pair.find((p) => p !== first.from) ?? pair[0] : first.from
      const b = pair.find((p) => p !== a) ?? pair[1]
      const sent = sorted.filter((i) => i.kind === 'sent').length
      const received = sorted.filter((i) => i.kind === 'received').length
      return { a, b, items: sorted, lastMs: sorted[sorted.length - 1].ms, waiting: Math.max(0, sent - received) }
    })
    .sort((x, y) => y.lastMs - x.lastMs)
}
