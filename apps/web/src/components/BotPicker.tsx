import type { Bot, BotAgent, BotsState } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import { Avatar } from './Avatar'

interface Row {
  key: string
  project: string
  name: string
  agent: BotAgent
  requests: number | null
  lastMs: number | null
  bot: Bot | undefined
}

/** Active bots first, then the projects with Claude Code or Codex usage that could have one. */
function rows(state: BotsState): Row[] {
  const owner = (agent: BotAgent, project: string) => state.bots.find((b) => b.agent === agent && b.dir.endsWith(`/${project}`))
  const fromUsage = state.candidates.map((c) => ({
    key: `${c.agent}:${c.project}`,
    project: c.project,
    name: c.name,
    agent: c.agent,
    requests: c.requests,
    lastMs: c.last_ms,
    bot: owner(c.agent, c.project),
  }))
  const listed = new Set(fromUsage.flatMap((r) => (r.bot ? [r.bot.id] : [])))
  const onlyActive = state.bots
    .filter((b) => !listed.has(b.id))
    .map((b) => ({ key: b.id, project: b.dir, name: b.name, agent: b.agent, requests: null, lastMs: null, bot: b }))
  return [...onlyActive, ...fromUsage].sort((a, b) => Number(Boolean(b.bot)) - Number(Boolean(a.bot)))
}

const day = (ms: number) => new Date(ms).toLocaleDateString(locale, { day: '2-digit', month: 'short' }).replace('.', '').replace(' de ', ' ')

interface BotPickerProps {
  bots: Bots
  onOpen: (id: string) => void
}

export function BotPicker({ bots, onOpen }: BotPickerProps) {
  const list = rows(bots.state)
  const activate = async (row: Row) => {
    const before = new Set(bots.state.bots.map((b) => b.id))
    const next = await bots.act('activate', { project: row.project, agent: row.agent })
    const created = next?.bots.find((b) => !before.has(b.id))
    if (created) onOpen(created.id)
  }
  return (
    <main className="chat">
      <section className="cartao bots-lista">
        <p className="bots-regra">{t.botsRule}</p>
        {list.length === 0 && <p className="vazio">{t.noCandidates}</p>}
        <ul className="bots-itens">
          {list.map((row) => (
            <li key={row.key} className="bot-linha">
              <Avatar agent={faceForBot(row.bot ?? { agent: row.agent, persona: null })} size={28} state="still" />
              <span className="bot-nome">
                <b>{row.name}</b>
                <small>{row.project}</small>
              </span>
              <span className="bot-uso">
                {AGENT_LABEL[row.agent]}
                {row.requests !== null && ` · ${compact(row.requests)} ${t.requestsShort}`}
                {row.lastMs !== null && ` · ${day(row.lastMs)}`}
              </span>
              <span className="acao">
                {row.bot ? (
                  <>
                    <button className="vidro-btn" onClick={() => row.bot && onOpen(row.bot.id)}>
                      {t.composeBot} {AGENT_LABEL[row.agent]}
                    </button>
                    <button className="link" disabled={row.bot.busy} onClick={() => row.bot && void bots.act('deactivate', { id: row.bot.id })}>
                      {t.deactivate}
                    </button>
                  </>
                ) : (
                  <button className="vidro-btn" onClick={() => void activate(row)}>
                    {t.activate}
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
        {bots.error && <p className="nota failed">{bots.error}</p>}
      </section>
    </main>
  )
}
