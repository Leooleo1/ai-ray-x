import { useState } from 'react'
import type { Bot, BotAgent, BotsState } from '../api'
import { AGENT_LABEL, faceForBot, faceOf } from '../avatars'
import type { Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import { PRESETS } from '../onboarding/presets'
import { Avatar } from './Avatar'

/** Projects offered before searching; the search reaches all of them. */
const FIRST_ROWS = 12
const NEUTRAL_FACE = faceOf(PRESETS[0].shape, '#9a9aa6', PRESETS[0].name)

interface Row {
  project: string
  name: string
  usage: Partial<Record<BotAgent, number>>
  lastMs: number | null
  bots: Bot[]
}

const inProject = (bot: Bot, project: string) => bot.dir.endsWith(`/${project}`)

/** One row per project folder: where the person used an AI CLI, plus any folder that already has a bot. */
function rows(state: BotsState): Row[] {
  const byProject = new Map<string, Row>()
  for (const c of state.candidates) {
    const row = byProject.get(c.project) ?? { project: c.project, name: c.name, usage: {}, lastMs: c.last_ms, bots: [] }
    byProject.set(c.project, { ...row, usage: { ...row.usage, [c.agent]: c.requests }, lastMs: Math.max(row.lastMs ?? 0, c.last_ms) })
  }
  const all = [...byProject.values()].map((row) => ({ ...row, bots: state.bots.filter((b) => inProject(b, row.project)) }))
  const listed = new Set(all.flatMap((row) => row.bots.map((b) => b.id)))
  const outside = state.bots.filter((b) => !listed.has(b.id)).map((b) => ({ project: b.dir, name: b.name, usage: {}, lastMs: null, bots: [b] }))
  return [...outside, ...all]
}

const day = (ms: number) => new Date(ms).toLocaleDateString(locale, { day: '2-digit', month: 'short' }).replace('.', '').replace(' de ', ' ')
const usageLine = (row: Row) =>
  [...(Object.entries(row.usage) as [BotAgent, number][]).map(([agent, n]) => `${AGENT_LABEL[agent]} ${compact(n)}`), row.lastMs ? day(row.lastMs) : ''].filter(Boolean).join(' · ')

interface BotPickerProps {
  bots: Bots
  onOpen: (id: string) => void
  onAskMany: () => void
  /** Opens a channel, or the new-channel form with `novo`. */
  onChannel: (id: string) => void
}

/** The projects with an AI on top, to open; below, every other project, to turn on with one tap. */
export function BotPicker({ bots, onOpen, onAskMany, onChannel }: BotPickerProps) {
  const [query, setQuery] = useState('')
  // The project whose AI choice is open, when more than one CLI is on.
  const [choosing, setChoosing] = useState<string | null>(null)
  const all = rows(bots.state)
  const agents = bots.state.agents.map((a) => a.id)
  const mine = all.filter((row) => row.bots.length > 0)
  const needle = query.trim().toLowerCase()
  const others = all.filter((row) => row.bots.length === 0 && (!needle || `${row.name} ${row.project}`.toLowerCase().includes(needle)))
  const shown = needle ? others : others.slice(0, FIRST_ROWS)
  const activate = async (row: Row, agent: BotAgent) => {
    setChoosing(null)
    const before = new Set(bots.state.bots.map((b) => b.id))
    const next = await bots.act('activate', { project: row.project, agent })
    const created = next?.bots.find((b) => !before.has(b.id))
    if (created) onOpen(created.id)
  }
  const turnOn = (row: Row) => (agents.length === 1 ? void activate(row, agents[0]) : setChoosing(choosing === row.project ? null : row.project))
  return (
    <main className="chat">
      <section className="cartao projetos-pagina">
        {mine.length > 0 && (
          <>
            {/* On a phone the menu has no room for it, so Ask several lives here too. */}
            <button type="button" className="projetos-varios so-celular" onClick={onAskMany}>
              {t.askMany}
            </button>
            <h2 className="projetos-secao">{t.yourProjects}</h2>
            <ul className="projetos-lista">
              {mine.map((row) => {
                const latest = row.bots[0]
                return (
                  <li key={row.project} className="projeto-linha">
                    <Avatar agent={faceForBot(latest)} size={32} state="still" />
                    <span className="projeto-nome">
                      <b>{row.name}</b>
                      <small>{row.bots.map((b) => AGENT_LABEL[b.agent]).join(' · ')}</small>
                    </span>
                    <button type="button" className="projeto-acao cheio" onClick={() => onOpen(latest.id)}>
                      {t.openProject}
                    </button>
                  </li>
                )
              })}
            </ul>
          </>
        )}
        {mine.length > 0 && (
          <>
            {/* On a phone the menu has no room for the channels either, so they live here too. */}
            <h2 className="projetos-secao so-celular">{t.channels}</h2>
            <ul className="projetos-lista so-celular">
              {(bots.state.channels ?? []).map((channel) => (
                <li key={channel.id} className="projeto-linha">
                  <span className="projeto-cerquilha" aria-hidden="true">
                    #
                  </span>
                  <span className="projeto-nome">
                    <b>{channel.name}</b>
                    <small>{channel.members.map((m) => `@${m.handle}`).join(' ')}</small>
                  </span>
                  <button type="button" className="projeto-acao cheio" onClick={() => onChannel(channel.id)}>
                    {t.openProject}
                  </button>
                </li>
              ))}
              <li className="projeto-linha">
                <span className="projeto-cerquilha" aria-hidden="true">
                  #
                </span>
                <span className="projeto-nome">
                  <b>{t.channelNewNav}</b>
                  <small>{t.channelNewHint}</small>
                </span>
                <button type="button" className="projeto-acao" onClick={() => onChannel('novo')}>
                  {t.channelCreate}
                </button>
              </li>
            </ul>
          </>
        )}
        <h2 className="projetos-secao">
          {t.addProject}
          <small>{t.addProjectHint}</small>
        </h2>
        <input className="projetos-busca" value={query} placeholder={t.searchProjects} aria-label={t.searchProjects} onChange={(event) => setQuery(event.target.value)} />
        {agents.length === 0 && <p className="nota">{t.noAgentsOn}</p>}
        {shown.length === 0 && <p className="nota">{t.noCandidates}</p>}
        <ul className="projetos-lista">
          {shown.map((row) => (
            <li key={row.project} className="projeto-linha">
              <Avatar agent={NEUTRAL_FACE} size={32} state="still" />
              <span className="projeto-nome">
                <b>{row.name}</b>
                <small>{[row.project, usageLine(row)].filter(Boolean).join(' · ')}</small>
              </span>
              {choosing === row.project ? (
                <span className="projeto-ias" role="group" aria-label={t.pickAi}>
                  {agents.map((agent) => (
                    <button key={agent} type="button" className="projeto-acao" onClick={() => void activate(row, agent)}>
                      {AGENT_LABEL[agent]}
                    </button>
                  ))}
                </span>
              ) : (
                <button type="button" className="projeto-acao" disabled={agents.length === 0} onClick={() => turnOn(row)}>
                  {t.activate}
                </button>
              )}
            </li>
          ))}
        </ul>
        {!needle && others.length > shown.length && <p className="nota">{t.moreProjects.replace('{n}', String(others.length - shown.length))}</p>}
        {bots.error && <p className="nota failed">{bots.error}</p>}
      </section>
    </main>
  )
}
