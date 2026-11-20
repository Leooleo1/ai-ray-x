import type { BotAgent, CliInstall, CliTestResult, Connection, GitHubState } from '../../api'
import { AgentLogo } from '../AgentLogo'
import { compact, locale, t } from '../../i18n'
import { Command } from '../../onboarding/parts'
import { modelName } from '../BotSettings'
import { DefaultPicker } from './DefaultPicker'
import '../controls.css'

/** Model choices when no bot is open to ask: Claude Code's own aliases. */
const CLAUDE_ALIASES = ['opus', 'sonnet', 'haiku', 'fable']
const EFFORTS: Partial<Record<BotAgent, string[]>> = {
  claude: ['low', 'medium', 'high', 'xhigh', 'max'],
  codex: ['low', 'medium', 'high', 'xhigh'],
}
/** CLIs with a mark of their own; the others keep two letters. */
const AGENT_IDS = new Set(['claude', 'codex', 'cursor', 'opencode', 'antigravity'])
const MONOGRAMS: Record<string, string> = { copilot: 'CP' }
const lastLine = (log: string) => log.trim().split('\n').pop() ?? ''
const formatMs = (ms: number) => (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1).replace('.', locale === 'pt-BR' ? ',' : '.')} s`)

type Tone = 'ok' | 'alerta' | ''

function statusOf(c: Connection, install?: CliInstall): { text: string; tone: Tone } {
  if (!c.installed && install?.state === 'running') return { text: t.installing, tone: 'alerta' }
  if (!c.installed) return { text: t.notFound, tone: '' }
  if (c.logged_in === false) return { text: t.notSignedIn, tone: 'alerta' }
  if (c.logged_in === null) return { text: t.unknownSignIn, tone: 'ok' }
  return { text: [t.connected, c.method, c.plan].filter(Boolean).join(' · '), tone: 'ok' }
}

export interface Defaults {
  model: string | null
  effort: string | null
}

interface ConnectionTileProps {
  connection: Connection
  /** Its install from the panel, when one was started. */
  install?: CliInstall
  /** Sign-in flow opened for this CLI while not yet signed in. */
  signingIn?: 'browser' | 'terminal'
  testState?: { testing: boolean; result?: CliTestResult }
  onInstall?: () => void
  onSignIn?: () => void
  onTest?: () => void
  defaults: Defaults
  onToggle: (on: boolean) => void
  onDefaults: (next: Defaults) => void
}

/** One AI CLI: whether it is here and signed in, what raiox read from it, and the model and effort its bots start on. */
export function ConnectionTile({ connection: c, install, signingIn, testState, onInstall, onSignIn, onTest, defaults, onToggle, onDefaults }: ConnectionTileProps) {
  const status = statusOf(c, install)
  const usable = c.installed && c.logged_in !== false && (c.bots || c.xray)
  const lit = usable && c.enabled
  const how = !c.installed ? { label: t.installLabel, command: c.install } : c.logged_in === false ? { label: t.signInLabel, command: c.login } : null
  const does = [c.xray ? t.canXray : '', c.bots ? t.canBots : ''].filter(Boolean).join(' · ')
  // What is still missing for this CLI, when it has no requests to show instead.
  const missing = !does ? t.botsLater : c.installed && !c.xray ? t.xrayLater : ''
  const agent = c.id as BotAgent
  const efforts = EFFORTS[agent] ?? []
  return (
    <article className={`cfg-ia ${lit ? 'ligada' : ''} ${usable ? '' : 'apagada'}`}>
      <header>
        <span className="cfg-monograma">
          {AGENT_IDS.has(c.id) ? <AgentLogo agent={c.id} size={18} /> : (MONOGRAMS[c.id] ?? c.name.slice(0, 2).toUpperCase())}
        </span>
        <span className="cfg-nome">
          <b>{c.name}</b>
          <small>{[c.version ?? '—', does].filter(Boolean).join(' · ')}</small>
        </span>
        {usable && (
          <button type="button" className={`config-chave ${c.enabled ? 'ligada' : ''}`} role="switch" aria-checked={c.enabled} aria-label={c.name} onClick={() => onToggle(!c.enabled)}>
            <i />
            {c.enabled ? t.turnedOn : t.turnOn}
          </button>
        )}
      </header>
      <p className={`cfg-estado ${status.tone}`}>
        <i className={`ponto ${status.tone}`} />
        <span>{status.text}</span>
      </p>
      {c.requests > 0 ? (
        <p className="cfg-numero">
          <b>{compact(c.requests)}</b>
          <small>{t.requestsRead}</small>
        </p>
      ) : (
        missing && <p className="cfg-faz">{missing}</p>
      )}
      {install?.state === 'running' && <p className="cfg-faz cfg-instalando">{lastLine(install.log) || t.installing}</p>}
      {install?.state === 'failed' && !c.installed && <p className="cfg-faz falhou">{t.installStopped}</p>}
      {signingIn && <p className="cfg-faz">{signingIn === 'browser' ? t.signInBrowser : t.signInOpened}</p>}
      {testState?.result && (
        <p className={`cfg-teste ${testState.result.ok ? 'ok' : 'falhou'}`}>
          {testState.result.ok
            ? `ok · ${formatMs(testState.result.duration_ms)}`
            : `${testState.result.error || 'erro'}${testState.result.duration_ms > 0 ? ` · ${formatMs(testState.result.duration_ms)}` : ''}`}
        </p>
      )}
      <footer>
        {!c.installed && onInstall && install?.state !== 'running' && (
          <button type="button" className="cfg-btn cheio pequeno" onClick={onInstall}>
            {t.installAction}
          </button>
        )}
        {c.installed && c.logged_in === false && onSignIn && (
          <button type="button" className="cfg-btn cheio pequeno" onClick={onSignIn}>
            {t.signInAction}
          </button>
        )}
        {how && (install?.state === 'failed' || !onInstall || c.installed) && <Command label={how.label} command={how.command} />}
        {usable && onTest && (
          <button type="button" className="acao-texto" disabled={testState?.testing} onClick={onTest}>
            {testState?.testing ? t.testing : t.testNow}
          </button>
        )}
        {lit && c.bots && (
          <>
            <small className="cfg-rotulo">{t.startsWith}</small>
            <DefaultPicker
              label={t.modelLabel}
              value={defaults.model}
              options={agent === 'claude' ? CLAUDE_ALIASES : []}
              free={agent !== 'claude'}
              show={modelName}
              onPick={(model) => onDefaults({ ...defaults, model })}
            />
            {efforts.length > 0 && (
              <DefaultPicker label={t.effortLabel} value={defaults.effort} options={efforts} free={false} show={(e) => e} onPick={(effort) => onDefaults({ ...defaults, effort })} />
            )}
          </>
        )}
      </footer>
    </article>
  )
}

interface GitHubTileProps {
  github: GitHubState | null
  onCheck: () => void
}

/** GitHub through the person's own `gh`: who is signed in and how many repositories it sees. */
export function GitHubTile({ github, onCheck }: GitHubTileProps) {
  const ok = github?.logged_in === true
  return (
    <article className={`cfg-ia ${ok ? '' : 'apagada'}`} id="cfg-github">
      <header>
        <span className="cfg-monograma">
          <img src="/marcas/github.svg" width={17} height={17} alt="" />
        </span>
        <span className="cfg-nome">
          <b>{t.githubTitle}</b>
          <small>gh</small>
        </span>
        <button type="button" className="cfg-btn vidro pequeno" disabled={github === null} onClick={onCheck}>
          {t.checkAgain}
        </button>
      </header>
      <p className={`cfg-estado ${ok ? 'ok' : github ? 'alerta' : ''}`}>
        <i className={`ponto ${ok ? 'ok' : github ? 'alerta' : ''}`} />
        <span>{github === null ? t.probing : ok ? `${t.githubAs} ${github.login}` : github.installed ? t.githubSignedOut : t.githubNoGh}</span>
      </p>
      {ok && (
        <p className="cfg-numero">
          <b>{github.repos.length}</b>
          <small>{t.githubRepos}</small>
        </p>
      )}
      {github && !ok && (
        <footer className="cfg-comandos">
          {!github.installed && <Command label={t.installLabel} command={github.install} />}
          <Command label={t.signInLabel} command={github.connect} />
        </footer>
      )}
    </article>
  )
}
