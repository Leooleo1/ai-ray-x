import type { Bot } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import { compact, t } from '../../i18n'
import { Avatar } from '../Avatar'
import { modelName } from '../BotSettings'
import { when } from '../chat/listParts'
import type { ChatNav } from '../chat/nav'

const WEEK_MS = 7 * 86_400_000

const messagesOf = (bot: Bot) => [...bot.messages, ...bot.threads.flatMap((x) => x.messages)]

/** What this agent read and wrote through raiox in the last seven days, across its conversations. */
function weekTokens(bot: Bot, now: number): number {
  return messagesOf(bot)
    .filter((m) => m.author === 'bot' && m.ms >= now - WEEK_MS)
    .reduce((sum, m) => sum + (m.data.reply?.read_tokens ?? 0) + (m.data.reply?.output_tokens ?? 0), 0)
}

const lastActivity = (bot: Bot): number | undefined => Math.max(0, ...messagesOf(bot).map((m) => m.ms)) || undefined

function routinesLabel(count: number): string {
  if (count === 0) return ''
  return count === 1 ? t.deskRoutine : t.deskRoutines.replace('{n}', String(count))
}

interface TeamProps {
  bots: Bot[]
  nav: ChatNav
  now: number
}

/** The whole team, by project then by AI: who each one is, where it works, and how much it worked this week. */
export function Team({ bots, nav, now }: TeamProps) {
  const team = [...bots].sort((a, b) => a.name.localeCompare(b.name) || a.agent.localeCompare(b.agent))
  return (
    <section className="mesa-equipe" aria-label={t.deskTeam}>
      <header className="mesa-raia-topo">
        <h3>{t.deskTeam}</h3>
        <span className="mesa-raia-conta" aria-hidden="true">
          {team.length}
        </span>
      </header>
      <ul>
        {team.map((bot) => {
          const week = weekTokens(bot, now)
          return (
            <li key={bot.id}>
              <button type="button" onClick={() => nav.openBot(bot.id)}>
                <Avatar agent={faceForBot(bot)} size={28} state={bot.busy ? 'thinking' : 'still'} />
                <span className="mesa-equipe-quem">
                  <b>{bot.persona?.name ?? AGENT_LABEL[bot.agent]}</b>
                  <small className={bot.persona?.role ? '' : 'sem-cargo'}>{bot.persona?.role || t.deskNoRole}</small>
                </span>
                <span className="mesa-equipe-onde">
                  <b>{bot.name}</b>
                  <small>{[AGENT_LABEL[bot.agent], bot.model ? modelName(bot.model) : ''].filter(Boolean).join(' · ')}</small>
                </span>
                <span className="mesa-equipe-estado">
                  <span className={`mesa-estado ${bot.busy ? 'ativo' : ''}`}>
                    <i className="ponto" />
                    {bot.busy ? t.deskWorkingNow : [t.deskIdleSince, when(lastActivity(bot))].filter(Boolean).join(' ')}
                  </span>
                  <small>{routinesLabel(bot.routines.length)}</small>
                </span>
                <span className="mesa-cifra">{week ? t.deskWeek.replace('{n}', compact(week)) : ''}</span>
              </button>
            </li>
          )
        })}
      </ul>
    </section>
  )
}
