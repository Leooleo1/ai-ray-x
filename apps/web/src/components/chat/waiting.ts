// What waits for the person in a bot's conversations, shared by the chat and the desk.

import type { Bot, BotMessage } from '../../api'
import { t } from '../../i18n'

/** Messages that leave the answer before them current: its proposal and commands still stand. */
export const ASIDE: BotMessage['kind'][] = ['failed', 'sent', 'stopped', 'house', 'remembered', 'review', 'review_asked']

/** The latest answer, when nothing came after it: the commands it asks for can still run. */
export function latestAnswer(messages: BotMessage[]): number {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i]
    if (ASIDE.includes(m.kind)) continue
    return m.author === 'bot' && m.kind === 'answer' ? i : -1
  }
  return -1
}

/** The answer whose proposal still waits for approval, if any. */
export function pendingProposal(messages: BotMessage[]): number {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i]
    if (ASIDE.includes(m.kind)) continue
    if (m.kind === 'approve') return -1
    return m.kind === 'answer' && m.data.reply?.proposal ? i : -1
  }
  return -1
}

/** Drafts with changes that were neither applied nor discarded yet. */
export function openDrafts(messages: BotMessage[]): BotMessage[] {
  const settled = new Set(messages.filter((m) => m.kind === 'draft_applied' || m.kind === 'draft_discarded' || m.kind === 'draft_undone').map((m) => m.data.draft))
  return messages.filter((m) => m.kind === 'draft' && m.data.draft && (m.data.files?.length ?? 0) > 0 && !settled.has(m.data.draft))
}

export type WaitingKind = 'proposal' | 'draft' | 'command'

export interface Waiting {
  bot: Bot
  thread: string
  title: string
  kind: WaitingKind
  /** The message to open. */
  ms: number
  text: string
}

/** Everything waiting for the person across one bot's conversations. */
export function waitingOf(bot: Bot): Waiting[] {
  const conversations = [{ id: 'main', title: t.mainConversation, messages: bot.messages }, ...bot.threads.map((x) => ({ id: x.id, title: x.title, messages: x.messages }))]
  return conversations.flatMap(({ id, title, messages }) => {
    const found: Omit<Waiting, 'bot' | 'thread' | 'title'>[] = []
    const proposal = pendingProposal(messages)
    if (proposal >= 0) found.push({ kind: 'proposal', ms: messages[proposal].ms, text: messages[proposal].data.reply?.proposal ?? '' })
    const latest = latestAnswer(messages)
    for (const request of latest >= 0 ? (messages[latest].data.reply?.run ?? []) : []) {
      if (!request.refused) found.push({ kind: 'command', ms: messages[latest].ms, text: request.command })
    }
    for (const draft of openDrafts(messages)) {
      found.push({ kind: 'draft', ms: draft.ms, text: (draft.data.files ?? []).map((f) => f.path).join(', ') })
    }
    return found.map((item) => ({ ...item, bot, thread: id, title }))
  })
}
