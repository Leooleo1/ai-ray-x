import { useEffect, useState } from 'react'
import { type Connection, type SetupState, installCli, signInCli } from '../api'
import { compact } from '../i18n'
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

export function StepAis({ setup, loading, onCheck, onRefresh, onBack, onNext, onFree }: StepAisProps) {
  const [signingIn, setSigningIn] = useState<Record<string, number>>({})
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [free, setFree] = useState(false)
  // Every CLI that can be a bot: installed ones as they are, the others with Install.
  const ais = (setup?.ais ?? []).filter((ai) => ai.bots)
  const installs = setup?.installs ?? {}
  const read = ais.reduce((sum, ai) => sum + ai.requests, 0)
  const ready = ais.some(canRunBots)
  const opencode = ais.find((ai) => ai.id === 'opencode')
  const installing = Object.values(installs).some((job) => job.state === 'running')
  const waiting = Object.entries(signingIn).some(([id, since]) => Date.now() - since < SIGN_IN_WAIT_MS && ais.find((ai) => ai.id === id)?.logged_in !== true)

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
  const signIn = (id: string) => act(id, async () => {
    await signInCli(id)
    setSigningIn((before) => ({ ...before, [id]: Date.now() }))
  })
  const startFree = () => {
    setFree(true)
    if (opencode && !opencode.installed && installs.opencode?.state !== 'running') void install('opencode')
  }

  return (
    <Frame
      actions={
        <>
          <button className="bv-btn link" onClick={onBack}>
            {o.back}
          </button>
          <button className="bv-btn vidro" disabled={loading} onClick={onCheck}>
            {o.checkAgain}
          </button>
          <button className="bv-btn cheio bv-principal" disabled={!setup} onClick={onNext}>
            {ready ? o.continue : o.noBotsYet}
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
          return (
            <li key={ai.id} className="bv-ia">
              <i className={`ponto ${usable ? 'ok' : ai.installed ? 'alerta' : ''}`} />
              <b>{ai.name}</b>
              <span className="bv-mono">{ai.version?.match(/\d+(\.\d+)+/)?.[0] ?? ''}</span>
              <span className={`bv-estado ${usable ? 'ok' : ''}`}>{job?.state === 'running' ? o.installing : status(ai)}</span>
              <span className="bv-direita">
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
                {ai.installed && ai.logged_in !== false && ai.requests > 0 && <span className="bv-mono">{`${compact(ai.requests)} ${o.requestsRead}`}</span>}
              </span>
              {job?.state === 'running' && <span className="bv-mono bv-ia-nota">{lastLine(job.log)}</span>}
              {signingIn[ai.id] && ai.logged_in === false && <span className="bv-ia-nota">{o.signInOpened}</span>}
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
