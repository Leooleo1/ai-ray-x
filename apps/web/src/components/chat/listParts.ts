import { AGENT_LABEL } from '../../avatars'
// Small helpers the conversation lists share.

import type { BotMessage } from '../../api'
import { AGENTS } from '../../agents'
import { locale, t } from '../../i18n'
import { houseAsk } from '../../voices'
import { isHouse } from './HouseLine'

/** The last thing said, as one line for a list. */
export function preview(m: BotMessage | undefined): string {
  if (!m) return t.noMessagesYet
  if (isHouse(m.author)) return `${AGENTS[m.author].name} · ${houseAsk(m.author)}`
  if (m.kind === 'review') return `${t.reviewBy.replace('{a}', m.data.agent ? AGENT_LABEL[m.data.agent] : '')}: ${(m.data.reply?.text ?? '').split('\n')[0]}`
  if (m.kind === 'review_asked') return t.reviewAsked.replace('{a}', '').trim()
  if (m.kind === 'remembered') return t.rememberedLine
  if (m.kind === 'ran') return `${t.ranLine} ${m.data.ran?.command ?? ''}`
  const text = m.data.reply?.text ?? m.data.text ?? m.data.proposal ?? m.data.error ?? ''
  return text.replace(/[`*#>]/g, '').split('\n').find((l) => l.trim()) ?? ''
}

/** A time today, else a short date. */
export function when(ms: number | undefined): string {
  if (!ms) return ''
  const date = new Date(ms)
  if (date.toDateString() === new Date().toDateString()) return date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
  return date.toLocaleDateString(locale, { day: 'numeric', month: 'short' })
}

export const count = (n: number, [one, many]: [string, string]) => `${n} ${n === 1 ? one : many}`
