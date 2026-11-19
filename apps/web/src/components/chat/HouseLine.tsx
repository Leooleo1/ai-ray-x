import { AGENTS, type AgentId } from '../../agents'
import { useEffect, useState } from 'react'
import { type Bot, type BotMessage, type ProjectDiet, type Report, botDiet } from '../../api'
import { suggestionTitle } from '../../diet'
import { compact, percent, t } from '../../i18n'
import { type Data, houseAsk, houseText } from '../../voices'
import { Action } from '../Action'
import { Avatar } from '../Avatar'
import { CompactionBreakdown } from '../Breakdown'

export const HOUSE: AgentId[] = ['raio', 'dieta', 'memo']

export const isHouse = (author: string): author is AgentId => (HOUSE as string[]).includes(author)

interface HouseAskProps {
  onAsk: (agent: AgentId) => void
}

/** Raio, Dieta and Memo, one tap away above the composer; they answer from the logs, without AI. */
export function HouseAsk({ onAsk }: HouseAskProps) {
  return (
    <div className="casa-perguntas" role="group" aria-label={t.houseLabel}>
      {HOUSE.map((id) => (
        <button key={id} type="button" className="casa-chip" onClick={() => onAsk(id)}>
          <Avatar agent={AGENTS[id]} size={16} state="still" />
          <b>{AGENTS[id].name}</b>
          <span>{houseAsk(id)}</span>
        </button>
      ))}
    </div>
  )
}

interface HouseLineProps {
  bot: Bot
  message: BotMessage
  /** The conversation the answer sits in, for Raio's count of what it read. */
  messages: BotMessage[]
  report: Report
  onChanged: () => void
}

/** An answer from a house agent: its face and voice, and what can be done with it right there. */
export function HouseLine({ bot, message, messages, report, onChanged }: HouseLineProps) {
  const agent = message.author as AgentId
  const face = AGENTS[agent]
  const data = message.data as unknown as Data
  return (
    <div className="linha de-agente casa-linha" id={`msg-${message.ms}`}>
      <Avatar agent={face} size={28} state="still" />
      <div className="casa">
        <span className="casa-quem">
          <b style={{ color: face.color }}>{face.name}</b>
          <small>
            <span className="casa-papel">{`${face.role} · `}</span>
            {t.houseFree}
          </small>
        </span>
        <p>{houseText(agent, data)}</p>
        {agent === 'raio' && <ConversationReads messages={messages} until={message.ms} />}
        {agent === 'dieta' && <HouseCuts bot={bot} data={data} report={report} onChanged={onChanged} />}
      </div>
    </div>
  )
}

/** What this conversation's answers read so far, the number every next question starts from. */
function ConversationReads({ messages, until }: { messages: BotMessage[]; until: number }) {
  const replies = messages.filter((m) => m.ms <= until && m.data.reply).map((m) => m.data.reply?.read_tokens ?? 0)
  if (replies.length === 0) return null
  const total = replies.reduce((sum, n) => sum + n, 0)
  const last = replies[replies.length - 1]
  return <small className="casa-meta">{t.conversationReads.replace('{n}', String(replies.length)).replace('{total}', compact(total)).replace('{last}', compact(last))}</small>
}

interface SuggestionData {
  id: string
}

/** Dieta's cuts that have a setting, as computed now, each with its own Apply in the bot's project. */
function HouseCuts({ bot, data, report, onChanged }: { bot: Bot; data: Data; report: Report; onChanged: () => void }) {
  const named = (data.suggestions as SuggestionData[] | undefined) ?? []
  const applicable = (data.applicable as string[] | undefined) ?? []
  const [diet, setDiet] = useState<ProjectDiet | null>(null)
  // Again after each apply or undo, which reloads the report.
  useEffect(() => {
    botDiet(bot.id).then(setDiet).catch(() => setDiet(null))
  }, [bot.id, report.changes.length])
  // The buttons act on the project's suggestion as computed now, never on numbers saved in an old message.
  const items = named.filter((s) => applicable.includes(s.id)).flatMap((s) => diet?.suggestions.filter((now) => now.id === s.id) ?? [])
  if (!diet || items.length === 0) return null
  const here = `${bot.dir}/`
  const scope = diet.scope.own ? t.dietOwn.replace('{n}', compact(diet.scope.calls)) : t.dietAll.replace('{n}', compact(diet.scope.calls)).replace('{m}', compact(diet.scope.minimum))
  return (
    <div className="aprovacoes">
      <small className="casa-meta">{scope}</small>
      {items.map((s) => {
        const change = report.changes.find((c) => c.suggestion === s.id && c.undone_ms === null && c.file.startsWith(here))
        return (
          <div key={s.id} className="aprovacao">
            <div className="aprovacao-topo">
              <b>{suggestionTitle(s.id, s.title)}</b>
              <span className="num">{`~${percent(s.saving_share)}`}</span>
            </div>
            {s.id === 'compact-earlier' && <CompactionBreakdown model={diet.compaction} />}
            <Action
              id={s.id}
              value={s.value}
              source="claude"
              change={change}
              outcome={report.outcomes.find((o) => o.change_id === change?.id)}
              onChanged={onChanged}
              bot={bot.id}
              project={bot.name}
            />
          </div>
        )
      })}
    </div>
  )
}
