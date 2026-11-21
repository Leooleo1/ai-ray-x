import { useEffect, useState } from 'react'
import { type CliTestResult, type Connection, IS_REMOTE, type SetupState, installCli, signInCli, testCli } from '../api'
import { compact, locale } from '../i18n'
import { Command, Frame } from './parts'
import { canRunBots } from './presets'
import { o } from './texts'

/** How often the step looks again while an install or a sign-in goes on. */
const POLL_MS = 2500
/** How long it keeps looking after a sign-in window opened. */
const SIGN_IN_WAIT_MS = 5 * 60 * 1000

interface StepAisProps {
  setup: SetupState | null
  loading: boolean
  openProject?: boolean
  openingProject?: boolean
  onCheck: () => void
  /** Looks again without showing it is checking (while installs and sign-ins go on). */
  onRefresh: () => void
  onBack: () => void
  onNext: () => void
  /** The person chose to start free: opencode becomes the bots' CLI, on a free model. */
  onFree: () => void
}

function status(ai: Connection): string {
  if (!ai.installed) return o.notInstalled
  if (ai.logged_in === false) return o.notLogged
  return [o.connected, ai.method, ai.plan].filter(Boolean).join(' · ')
}

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))
const lastLine = (log: string) => log.trim().split('\n').pop() ?? ''
const formatMs = (ms: number) => (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1).replace('.', locale === 'pt-BR' ? ',' : '.')} s`)

export function StepAis({ setup, loading, openProject, openingProject, onCheck, onRefresh, onBack, onNext, onFree }: StepAisProps) {
  const [signingIn, setSigningIn] = useState<Record<string, { since: number; mode: 'browser' | 'terminal' }>>({})
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [tests, setTests] = useState<Record<string, { testing: boolean; result?: CliTestResult }>>({})
  const [free, setFree] = useState(false)
  // Every CLI that can be a bot: installed ones as they are, the others with Install.
  const ais = (setup?.ais ?? []).filter((ai) => ai.bots)
  const installs = setup?.installs ?? {}
  const read = ais.reduce((sum, ai) => sum + ai.requests, 0)
  const ready = ais.some(canRunBots)
  const opencode = ais.find((ai) => ai.id === 'opencode')
  const installing = Object.values(installs).some((job) => job.state === 'running')
  const waiting = Object.entries(signingIn).some(([id, info]) => Date.now() - info.since < SIGN_IN_WAIT_MS && ais.find((ai) => ai.id === id)?.logged_in !== true)

  useEffect(() => {
    if (!installing && !waiting && !(free && !opencode?.installed)) return
    const timer = window.setInterval(onRefresh, POLL_MS)
    return () => window.clearInterval(timer)
  }, [installing, waiting, free, opencode?.installed, onRefresh])

  // Starting free: once opencode is installed, it becomes the bots' CLI.
  useEffect(() => {
    if (free && opencode && canRunBots(opencode)) onFree()
  }, [free, opencode, onFree])

  const act = async (id: string, run: () => Promise<unknown>) => {
    setErrors(({ [id]: _, ...rest }) => rest)
    try {
      await run()
      onRefresh()
    } catch (failure: unknown) {
      setErrors((before) => ({ ...before, [id]: messageOf(failure) }))
    }
  }
  const install = (id: string) => act(id, () => installCli(id))
  const signIn = (id: string) =>
    act(id, async () => {
      const res = await signInCli(id)
      setSigningIn((before) => ({ ...before, [id]: { since: Date.now(), mode: res.mode ?? 'terminal' } }))
    })
  const runTest = async (id: string) => {
    setTests((before) => ({ ...before, [id]: { testing: true } }))
    try {
      const result = await testCli(id)
      setTests((before) => ({ ...before, [id]: { testing: false, result } }))
    } catch (failure: unknown) {
      setTests((before) => ({ ...before, [id]: { testing: false, result: { ok: false, duration_ms: 0, error: messageOf(failure) } } }))
    }
  }
  const startFree = () => {
    setFree(true)
    if (opencode && !opencode.installed && installs.opencode?.state !== 'running') void install('opencode')
  }

  return (
    <Frame
      actions={
        <>
          <button className="bv-btn link" disabled={openingProject} onClick={onBack}>
            {o.back}
          </button>
          <button className="bv-btn vidro" disabled={loading || openingProject} onClick={onCheck}>
            {o.checkAgain}
          </button>
          <button className="bv-btn cheio bv-principal" disabled={!setup || openingProject || (openProject && !ready)} onClick={onNext}>
            {openProject ? (openingProject ? o.quickOpening : o.quickOpenChat) : ready ? o.continue : o.noBotsYet}
          </button>
        </>
      }
    >
      <div className="bv-numero">
        <span>{setup ? compact(read) : '—'}</span>
        <small>{setup ? o.readSoFar : o.checking}</small>
      </div>
      {setup && opencode && (!ready || free) && (
        <div className="bv-gratis">
          <span>
            <b>{o.freeTitle}</b>
            <small>{free && canRunBots(opencode) ? o.freeReady : o.freeText}</small>
          </span>
          {!(free && canRunBots(opencode)) && (
            <button className="bv-btn cheio" disabled={free} onClick={startFree}>
              {free ? o.installing : o.freeAction}
            </button>
          )}
        </div>
      )}
      <ul className="bv-lista">
        {ais.map((ai) => {
          const job = installs[ai.id]
          const usable = canRunBots(ai)
          const test = tests[ai.id]
          const sign = signingIn[ai.id]
          return (
            <li key={ai.id} className="bv-ia">
              <i className={`ponto ${usable ? 'ok' : ai.installed ? 'alerta' : ''}`} />
              <b>{ai.name}</b>
              <span className="bv-mono">{ai.version?.match(/\d+(\.\d+)+/)?.[0] ?? ''}</span>
              <span className={`bv-estado ${usable ? 'ok' : ''}`}>{job?.state === 'running' ? o.installing : status(ai)}</span>
              <span className="bv-direita bv-ia-acoes">
                {!ai.installed && job?.state !== 'running' && (
                  <button className="bv-btn vidro bv-pequeno" onClick={() => void install(ai.id)}>
                    {o.installAction}
                  </button>
                )}
                {ai.installed && ai.logged_in === false && (
                  <button className="bv-btn vidro bv-pequeno" onClick={() => void signIn(ai.id)}>
                    {o.signInAction}
                  </button>
                )}
                {usable && !IS_REMOTE && (
                  <button className="bv-btn vidro bv-pequeno" disabled={test?.testing} onClick={() => void runTest(ai.id)}>
                    {test?.testing ? o.testing : o.testNow}
                  </button>
                )}
                {ai.installed && ai.logged_in !== false && ai.requests > 0 && <span className="bv-mono">{`${compact(ai.requests)} ${o.requestsRead}`}</span>}
              </span>
              {job?.state === 'running' && <span className="bv-mono bv-ia-nota">{lastLine(job.log)}</span>}
              {sign && ai.logged_in === false && <span className="bv-ia-nota">{sign.mode === 'browser' ? o.signInBrowser : o.signInOpened}</span>}
              {test?.result && (
                <span className={`bv-mono bv-ia-nota ${test.result.ok ? 'ok' : 'falhou'}`}>
                  {test.result.ok
                    ? `ok · ${formatMs(test.result.duration_ms)}`
                    : `${test.result.error || 'erro'}${test.result.duration_ms > 0 ? ` · ${formatMs(test.result.duration_ms)}` : ''}`}
                </span>
              )}
              {(job?.state === 'failed' || errors[ai.id]) && !ai.installed && (
                <>
                  <span className="bv-ia-nota falhou">{errors[ai.id] ?? o.installFailed}</span>
                  <Command label={o.installWith} command={ai.install} />
                </>
              )}
              {errors[ai.id] && ai.installed && <Command label={o.loginWith} command={ai.login} />}
            </li>
          )
        })}
      </ul>
    </Frame>
  )
}
