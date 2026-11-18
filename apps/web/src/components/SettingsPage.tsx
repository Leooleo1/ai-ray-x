import { useCallback, useEffect, useState } from 'react'
import { type BotAgent, type Connection, type GitHubState, type SettingsState, fetchGitHub, fetchSettings, saveSettings } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, t } from '../i18n'
import { Command } from '../onboarding/parts'
import { Avatar } from './Avatar'
import { Choice, modelName } from './BotSettings'
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
  return (
    <main className="config">
      <section className="cartao config-bloco">
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
        <section className="cartao config-bloco">
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
                <span className="config-cli">{AGENT_LABEL[agent] ?? c.name}</span>
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
            )
          })}
        </section>
      )}

      <section className="cartao config-bloco">
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

      <section className="cartao config-bloco">
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
              <Avatar agent={faceForBot(bot)} size={28} state="still" />
              <b>{bot.name}</b>
              <span className="config-mono">
                {[bot.persona?.name, AGENT_LABEL[bot.agent], bot.model ? modelName(bot.model) : '', bot.effort ?? ''].filter(Boolean).join(' · ')}
              </span>
              <span className="acao">
                <button type="button" className="vidro-btn" onClick={() => onOpenBot(bot.id)}>
                  {t.talk}
                </button>
                <button type="button" className="link" disabled={bot.busy} onClick={() => void bots.act('deactivate', { id: bot.id })}>
                  {t.deactivate}
                </button>
              </span>
            </li>
          ))}
        </ul>
      </section>

      {(error ?? bots.error) && <p className="nota failed quebra">{error ?? bots.error}</p>}
      <button type="button" className="lado-link config-refazer" onClick={onRedoSetup}>
        {t.redoSetup}
      </button>
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
  const status = statusOf(c)
  const what = [c.xray ? t.canXray : '', c.bots ? t.canBots : ''].filter(Boolean)
  const usable = c.installed && c.logged_in !== false && (c.bots || c.xray)
  return (
    <li className="config-conexao">
      <i className={`ponto ${status.tone}`} />
      <b>{c.name}</b>
      <span className="config-mono">{c.version ?? ''}</span>
      <span className={`config-estado ${status.tone}`}>{status.text}</span>
      <span className="config-mono config-pode">
        {what.length > 0 ? what.join(' · ') : t.botsLater}
        {c.installed && !c.xray && c.bots ? ` · ${t.xrayLater}` : ''}
        {c.requests > 0 ? ` · ${compact(c.requests)}` : ''}
      </span>
      {usable ? (
        <button type="button" className={`config-chave ${c.enabled ? 'ligada' : ''}`} role="switch" aria-checked={c.enabled} aria-label={c.name} onClick={() => onToggle(!c.enabled)}>
          <i />
          {c.enabled ? t.turnedOn : t.turnOn}
        </button>
      ) : (
        <span />
      )}
      {!c.installed && <Command label={t.installLabel} command={c.install} />}
      {c.installed && c.logged_in === false && <Command label={t.signInLabel} command={c.login} />}
    </li>
  )
}
