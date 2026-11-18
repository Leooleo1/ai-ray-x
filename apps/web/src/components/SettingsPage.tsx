import { useCallback, useEffect, useState } from 'react'
import { type BotAgent, type Connection, type GitHubState, type SettingsState, fetchGitHub, fetchSettings, saveSettings } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, t } from '../i18n'
import { Command } from '../onboarding/parts'
import { Avatar } from './Avatar'
import { Choice, modelName } from './BotSettings'
import { WiresEditor } from './WiresEditor'
import './settings-page.css'

/** Model choices per CLI when no bot is open to ask: Claude Code's own aliases. */
const CLAUDE_ALIASES = ['opus', 'sonnet', 'haiku', 'fable']
const EFFORTS: Partial<Record<BotAgent, string[]>> = {
  claude: ['low', 'medium', 'high', 'xhigh', 'max'],
  codex: ['low', 'medium', 'high', 'xhigh'],
}

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure))

interface SettingsPageProps {
  bots: Bots
  onOpenBot: (id: string) => void
  onAddBot: () => void
  onRedoSetup: () => void
}

/** Connections, bot defaults, GitHub and the bots: what the first-run setup set, to change one piece at a time. */
export function SettingsPage({ bots, onOpenBot, onAddBot, onRedoSetup }: SettingsPageProps) {
  const [settings, setSettings] = useState<SettingsState | null>(null)
  const [github, setGithub] = useState<GitHubState | null>(null)
  const [error, setError] = useState<string | null>(null)

  const loadGithub = useCallback(() => {
    setGithub(null)
    fetchGitHub().then(setGithub).catch((failure: unknown) => setError(message(failure)))
  }, [])
  useEffect(() => {
    fetchSettings().then(setSettings).catch((failure: unknown) => setError(message(failure)))
    loadGithub()
  }, [loadGithub])

  const save = async (change: Record<string, unknown>) => {
    setError(null)
    try {
      setSettings(await saveSettings(change))
      // The bots page and pickers read the turned-on CLIs from the bots state.
      bots.refresh()
    } catch (failure: unknown) {
      setError(message(failure))
    }
  }

  const botClis = (settings?.connections ?? []).filter((c) => c.bots && c.enabled)
  const sections = [
    { id: 'conexoes', label: t.connectionsTitle },
    ...(botClis.length > 0 ? [{ id: 'padroes', label: t.defaultsTitle }] : []),
    { id: 'github', label: t.githubTitle },
    { id: 'bots', label: t.botsTitle2 },
    ...(bots.state.bots.length > 0 ? [{ id: 'fios', label: t.wires }] : []),
  ]
  const [current, setCurrent] = useState('conexoes')
  const go = (id: string) => {
    setCurrent(id)
    document.getElementById(`config-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }
  return (
    <main className="config">
      <nav className="config-nav" aria-label={t.settingsTitle}>
        {sections.map((s) => (
          <button key={s.id} type="button" className={`config-secao ${current === s.id ? 'ativo' : ''}`} aria-current={current === s.id ? 'true' : undefined} onClick={() => go(s.id)}>
            {s.label}
          </button>
        ))}
        <button type="button" className="lado-link config-refazer" onClick={onRedoSetup}>
          {t.redoSetup}
        </button>
      </nav>
      <div className="config-conteudo">
      <section className="cartao config-bloco" id="config-conexoes">
        <header className="config-topo">
          <b>{t.connectionsTitle}</b>
          <small>{t.connectionsHint}</small>
        </header>
        {!settings && <p className="nota">{t.probing}</p>}
        <ul className="config-lista">
          {settings?.connections.map((c) => (
            <ConnectionRow key={c.id} connection={c} onToggle={(on) => void save({ enabled: { [c.id]: on } })} />
          ))}
        </ul>
      </section>

      {botClis.length > 0 && (
        <section className="cartao config-bloco" id="config-padroes">
          <header className="config-topo">
            <b>{t.defaultsTitle}</b>
            <small>{t.defaultsHint}</small>
          </header>
          {botClis.map((c) => {
            const agent = c.id as BotAgent
            const chosen = settings?.defaults[c.id] ?? { model: null, effort: null }
            const efforts = EFFORTS[agent] ?? []
            return (
              <div key={c.id} className="config-padrao">
                <span className="config-rotulo">
                  <b>{AGENT_LABEL[agent] ?? c.name}</b>
                  <small>{chosen.model || chosen.effort ? [chosen.model, chosen.effort].filter(Boolean).join(' · ') : t.cliDefault}</small>
                </span>
                <div className="config-controles">
                <Choice
                  label={t.modelLabel}
                  current={chosen.model}
                  fallback={null}
                  options={agent === 'claude' ? CLAUDE_ALIASES : []}
                  free={agent !== 'claude'}
                  busy={false}
                  show={modelName}
                  onPick={(model) => void save({ defaults: { [c.id]: { model: model ?? '', effort: chosen.effort ?? '' } } })}
                />
                {efforts.length > 0 && (
                  <Choice
                    label={t.effortLabel}
                    current={chosen.effort}
                    fallback={null}
                    options={efforts}
                    busy={false}
                    show={(e) => e}
                    onPick={(effort) => void save({ defaults: { [c.id]: { model: chosen.model ?? '', effort: effort ?? '' } } })}
                  />
                )}
                </div>
              </div>
            )
          })}
        </section>
      )}

      <section className="cartao config-bloco" id="config-github">
        <header className="config-topo">
          <b>{t.githubTitle}</b>
          <button type="button" className="voltar" onClick={loadGithub}>
            {t.checkAgain}
          </button>
        </header>
        {!github && <p className="nota">{t.probing}</p>}
        {github?.logged_in && (
          <p className="config-linha">
            <i className="ponto ok" />
            {`${t.githubAs} ${github.login} · ${github.repos.length} ${t.githubRepos}`}
          </p>
        )}
        {github && !github.logged_in && (
          <div className="config-comandos">
            <p className="config-linha">
              <i className="ponto alerta" />
              {github.installed ? t.githubSignedOut : t.githubNoGh}
            </p>
            {!github.installed && <Command label={t.installLabel} command={github.install} />}
            <Command label={t.signInLabel} command={github.connect} />
          </div>
        )}
      </section>

      <section className="cartao config-bloco" id="config-bots">
        <header className="config-topo">
          <b>{t.botsTitle2}</b>
          <button type="button" className="voltar" onClick={onAddBot}>
            {t.addBot}
          </button>
        </header>
        {bots.state.bots.length === 0 && <p className="nota">{t.noBotsYet}</p>}
        <ul className="config-lista">
          {bots.state.bots.map((bot) => (
            <li key={bot.id} className="config-bot">
              <button type="button" className="config-bot-abrir" onClick={() => onOpenBot(bot.id)}>
              <Avatar agent={faceForBot(bot)} size={28} state="still" />
              <b>{bot.name}</b>
              <span className="config-mono">
                {[bot.persona?.name, AGENT_LABEL[bot.agent], bot.model ? modelName(bot.model) : '', bot.effort ?? ''].filter(Boolean).join(' · ')}
              </span>
              </button>
              <span className="acao">
                <button type="button" className="link" disabled={bot.busy} onClick={() => void bots.act('deactivate', { id: bot.id })}>
                  {t.deactivate}
                </button>
              </span>
            </li>
          ))}
        </ul>
      </section>

      {bots.state.bots.length > 0 && (
        <section className="cartao config-bloco" id="config-fios">
          <header className="config-topo">
            <b>{t.wires}</b>
          </header>
          <p className="nota">{t.wiresHint}</p>
          <WiresEditor bots={bots} />
        </section>
      )}

      {(error ?? bots.error) && <p className="nota failed quebra">{error ?? bots.error}</p>}
      </div>
    </main>
  )
}

function statusOf(c: Connection): { text: string; tone: 'ok' | 'alerta' | '' } {
  if (!c.installed) return { text: t.notFound, tone: '' }
  if (c.logged_in === false) return { text: t.notSignedIn, tone: 'alerta' }
  if (c.logged_in === null) return { text: t.unknownSignIn, tone: 'ok' }
  return { text: [t.connected, c.method, c.plan].filter(Boolean).join(' · '), tone: 'ok' }
}

interface ConnectionRowProps {
  connection: Connection
  onToggle: (on: boolean) => void
}

function ConnectionRow({ connection: c, onToggle }: ConnectionRowProps) {
  const [howOpen, setHowOpen] = useState(false)
  const status = statusOf(c)
  const what = [c.xray ? t.canXray : '', c.bots ? t.canBots : ''].filter(Boolean)
  const usable = c.installed && c.logged_in !== false && (c.bots || c.xray)
  const how = !c.installed ? { label: t.installLabel, command: c.install, open: t.howInstall } : c.logged_in === false ? { label: t.signInLabel, command: c.login, open: t.howSignIn } : null
  const notes = [what.length > 0 ? what.join(' · ') : t.botsLater, c.installed && !c.xray && c.bots ? t.xrayLater : '', c.requests > 0 ? `${compact(c.requests)} ${t.requestsShort}` : '']
  return (
    <li className="config-conexao">
      <i className={`ponto ${status.tone}`} />
      <span className="config-rotulo">
        <b>{c.name}</b>
        <small>{c.version ?? '—'}</small>
      </span>
      <span className="config-rotulo">
        <span className={`config-estado ${status.tone}`}>{status.text}</span>
        <small>{notes.filter(Boolean).join(' · ')}</small>
      </span>
      {usable ? (
        <button type="button" className={`config-chave ${c.enabled ? 'ligada' : ''}`} role="switch" aria-checked={c.enabled} aria-label={c.name} onClick={() => onToggle(!c.enabled)}>
          <i />
          {c.enabled ? t.turnedOn : t.turnOn}
        </button>
      ) : how ? (
        <button type="button" className="config-como" aria-expanded={howOpen} onClick={() => setHowOpen(!howOpen)}>
          {how.open}
        </button>
      ) : (
        <span />
      )}
      {how && howOpen && <Command label={how.label} command={how.command} />}
    </li>
  )
}
