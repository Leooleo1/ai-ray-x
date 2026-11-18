import type { Bot, BotAgent, BotsState } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import { Avatar } from './Avatar'

interface Row {
  project: string
  name: string
  usage: Partial<Record<BotAgent, number>>
  lastMs: number | null
  bots: Bot[]
}

const inProject = (bot: Bot, project: string) => bot.dir.endsWith(`/${project}`)

/** One row per project: those with bots first, then those with Claude Code or Codex use. */
function rows(state: BotsState): Row[] {
  const byProject = new Map<string, Row>()
  for (const c of state.candidates) {
    const row = byProject.get(c.project) ?? { project: c.project, name: c.name, usage: {}, lastMs: c.last_ms, bots: [] }
    byProject.set(c.project, { ...row, usage: { ...row.usage, [c.agent]: c.requests }, lastMs: Math.max(row.lastMs ?? 0, c.last_ms) })
  }
  const all = [...byProject.values()].map((row) => ({ ...row, bots: state.bots.filter((b) => inProject(b, row.project)) }))
  const listed = new Set(all.flatMap((row) => row.bots.map((b) => b.id)))
  const outside = state.bots
    .filter((b) => !listed.has(b.id))
    .map((b) => ({ project: b.dir, name: b.name, usage: {}, lastMs: null, bots: [b] }))
  return [...outside, ...all].sort((a, b) => Number(b.bots.length > 0) - Number(a.bots.length > 0))
}

const day = (ms: number) => new Date(ms).toLocaleDateString(locale, { day: '2-digit', month: 'short' }).replace('.', '').replace(' de ', ' ')

interface BotPickerProps {
  bots: Bots
  onOpen: (id: string) => void
}

/** Each project with one cell per turned-on CLI: a lit cell opens that bot, an unlit one creates it. */
export function BotPicker({ bots, onOpen }: BotPickerProps) {
  const list = rows(bots.state)
  const agents = bots.state.agents
  const activate = async (row: Row, agent: BotAgent) => {
    const before = new Set(bots.state.bots.map((b) => b.id))
    const next = await bots.act('activate', { project: row.project, agent })
    const created = next?.bots.find((b) => !before.has(b.id))
    if (created) onOpen(created.id)
  }
  return (
    <main className="chat">
      <section className="cartao bots-lista">
        <p className="bots-regra">{t.botsRule}</p>
        {list.length === 0 && <p className="vazio">{t.noCandidates}</p>}
        <ul className="bots-itens">
          {list.map((row) => {
            const usage = (Object.entries(row.usage) as [BotAgent, number][]).map(([agent, n]) => `${AGENT_LABEL[agent]} ${compact(n)}`)
            const face = faceForBot(row.bots[0] ?? { agent: 'claude', persona: null })
            return (
              <li key={row.project} className="bot-linha">
                <Avatar agent={face} size={28} state="still" />
                <span className="bot-nome">
                  <b>{row.name}</b>
                  <small>{row.project}</small>
                </span>
                <span className="bot-uso">{[...usage, row.lastMs ? day(row.lastMs) : ''].filter(Boolean).join(' · ')}</span>
                <span className="bot-agentes" role="group" aria-label={row.name}>
                  {agents.map(({ id }) => {
                    const bot = row.bots.find((b) => b.agent === id)
                    return (
                      <button
                        key={id}
                        type="button"
                        className={bot ? 'ativo' : ''}
                        disabled={bot?.busy}
                        title={bot ? `${t.composeBot} ${AGENT_LABEL[id]}` : `${t.activate} ${AGENT_LABEL[id]}`}
                        onClick={() => (bot ? onOpen(bot.id) : void activate(row, id))}
                      >
                        {bot ? AGENT_LABEL[id] : `+ ${AGENT_LABEL[id]}`}
                      </button>
                    )
                  })}
                </span>
              </li>
            )
          })}
        </ul>
        {bots.error && <p className="nota failed">{bots.error}</p>}
      </section>
    </main>
  )
}
