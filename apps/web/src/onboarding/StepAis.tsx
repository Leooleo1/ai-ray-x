import type { Connection, SetupState } from '../api'
import { compact } from '../i18n'
import { Command, Frame } from './parts'
import { o } from './texts'

interface StepAisProps {
  setup: SetupState | null
  loading: boolean
  onCheck: () => void
  onBack: () => void
  onNext: () => void
}

function status(ai: Connection): string {
  if (!ai.installed) return o.notInstalled
  if (ai.logged_in === false) return o.notLogged
  return [o.connected, ai.method, ai.plan].filter(Boolean).join(' · ')
}

export function StepAis({ setup, loading, onCheck, onBack, onNext }: StepAisProps) {
  // Claude Code and Codex always; the other CLIs only when they are on this machine.
  const ais = (setup?.ais ?? []).filter((ai) => ai.installed || ai.id === 'claude' || ai.id === 'codex')
  const read = ais.reduce((sum, ai) => sum + ai.requests, 0)
  const ready = ais.some((ai) => ai.bots && ai.logged_in === true)
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
      <ul className="bv-lista">
        {ais.map((ai) => (
          <li key={ai.id} className="bv-ia">
            <i className={`ponto ${ai.logged_in ? 'ok' : ai.installed ? 'alerta' : ''}`} />
            <b>{ai.name}</b>
            <span className="bv-mono">{ai.version?.match(/\d+(\.\d+)+/)?.[0] ?? ''}</span>
            <span className={`bv-estado ${ai.logged_in ? 'ok' : ''}`}>{status(ai)}</span>
            <span className="bv-mono bv-direita">{ai.requests > 0 ? `${compact(ai.requests)} ${o.requestsRead}` : ''}</span>
            {!ai.installed && <Command label={o.installWith} command={ai.install} />}
            {ai.installed && ai.logged_in === false && <Command label={o.loginWith} command={ai.login} />}
          </li>
        ))}
      </ul>
    </Frame>
  )
}
