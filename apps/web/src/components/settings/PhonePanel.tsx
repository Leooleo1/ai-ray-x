import { useEffect, useState } from 'react'
import { type Pairing, type RemoteState, type TunnelConfig, fetchRemote, forgetDevice, newPairing, setTunnel } from '../../api'
import { t } from '../../i18n'
import { Segmented } from '../Segmented'
import './phone.css'

/** How often the tab asks how the tunnel is doing, and how often the QR countdown ticks. */
const POLL_MS = 2000
const TICK_MS = 1000
const MINUTE_MS = 60_000
const HOUR_MS = 60 * MINUTE_MS

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

function ago(ms: number, now: number): string {
  const gone = now - ms
  if (gone < MINUTE_MS) return t.phoneNow
  if (gone < HOUR_MS) return t.phoneMinutes.replace('{n}', String(Math.floor(gone / MINUTE_MS)))
  if (gone < 24 * HOUR_MS) return t.phoneHours.replace('{n}', String(Math.floor(gone / HOUR_MS)))
  return shortDate(ms)
}

const shortDate = (ms: number) => new Date(ms).toLocaleDateString(undefined, { day: '2-digit', month: '2-digit' })

function countdown(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** Settings › Phone: the tunnel, pairing a device by QR, and the paired devices. */
export function PhonePanel() {
  const [remote, setRemote] = useState<RemoteState | null>(null)
  const [draft, setDraft] = useState<TunnelConfig | null>(null)
  const [pairing, setPairing] = useState<Pairing | null>(null)
  const [forgetting, setForgetting] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    let alive = true
    const load = () =>
      fetchRemote()
        .then((next) => {
          if (!alive) return
          setRemote(next)
          setDraft((typed) => typed ?? next.tunnel.config)
        })
        .catch((failure: unknown) => alive && setError(messageOf(failure)))
    void load()
    const poll = window.setInterval(() => void load(), POLL_MS)
    const tick = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => {
      alive = false
      window.clearInterval(poll)
      window.clearInterval(tick)
    }
  }, [])
  const act = async (call: () => Promise<RemoteState>) => {
    setError(null)
    try {
      setRemote(await call())
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const pair = async () => {
    setError(null)
    try {
      setPairing(await newPairing())
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  if (!remote || !draft) return error ? <p className="nota failed quebra">{error}</p> : <p className="cfg-vazio">{t.probing}</p>
  const { tunnel, devices } = remote
  const on = tunnel.config.on
  const live = tunnel.running && tunnel.host !== null
  const change = (field: keyof TunnelConfig, value: string) => setDraft({ ...draft, [field]: value })
  const toggle = () => {
    if (on) setPairing(null)
    void act(() => setTunnel({ ...draft, on: !on }))
  }
  const status = live
    ? { tone: 'ok', text: t.phoneLive.replace('{h}', tunnel.host ?? '') }
    : on && tunnel.exit_code !== null
      ? { tone: 'alerta', text: t.phoneStopped.replace('{n}', String(tunnel.exit_code)) }
      : { tone: '', text: on ? t.phoneStarting : t.phoneOff }
  const left = pairing ? pairing.expires_ms - now : 0
  const fields: { key: keyof TunnelConfig; label: string; hint: string }[] = [
    { key: 'tunnel', label: t.phoneTunnel, hint: 'raiox' },
    { key: 'hostname', label: t.phoneHostname, hint: 'raiox.seudominio.com' },
    { key: 'team', label: t.phoneTeam, hint: 'equipe.cloudflareaccess.com' },
    { key: 'aud', label: t.phoneAud, hint: '4f2c…' },
  ]
  return (
    <div className="cfg-grade celular">
      <article className={`cfg-ia ${live ? '' : 'apagada'}`}>
        <header>
          <span className="cfg-nome">
            <b>{t.phoneAccess}</b>
            <small>{t.phoneAccessHint}</small>
          </span>
          <button type="button" className={`config-chave ${on ? 'ligada' : ''}`} role="switch" aria-checked={on} aria-label={t.phoneAccess} disabled={!tunnel.installed} onClick={toggle}>
            <i />
            {on ? t.phoneOn : t.phoneTurnOn}
          </button>
        </header>
        <p className={`cfg-estado ${status.tone}`}>
          <i className={`ponto ${status.tone}`} />
          <span>{status.text}</span>
        </p>
        {!tunnel.installed && <p className="cfg-faz">{t.phoneMissing}</p>}
        <fieldset className="celular-modo" disabled={on}>
          <Segmented
            label={t.phoneAccess}
            options={[
              { id: 'quick', label: t.phoneQuick },
              { id: 'named', label: t.phoneNamed },
            ]}
            value={draft.mode}
            onChange={(mode) => setDraft({ ...draft, mode })}
          />
          <p className="cfg-faz">{draft.mode === 'quick' ? t.phoneQuickHint : t.phoneNamedHint}</p>
          {draft.mode === 'named' &&
            fields.map((field) => (
              <label key={field.key} className="ajustes-campo">
                <span>{field.label}</span>
                <input value={String(draft[field.key])} placeholder={field.hint} spellCheck={false} autoCapitalize="none" onChange={(event) => change(field.key, event.target.value.trim())} />
              </label>
            ))}
        </fieldset>
        {tunnel.log && <pre className="celular-log">{tunnel.log}</pre>}
      </article>
      <article className={`cfg-ia ${live ? '' : 'apagada'}`}>
        <header>
          <span className="cfg-nome">
            <b>{t.phonePair}</b>
            <small>{live ? t.phonePairHint : t.phonePairNeedsOn}</small>
          </span>
        </header>
        {pairing && live && left > 0 ? (
          <div className="celular-qr">
            <img src={pairing.qr} alt={t.phonePair} width={232} height={232} />
            <span className="cfg-faz">{t.phonePairTyped}</span>
            <span className="celular-codigo">{pairing.code}</span>
            <span className="cfg-faz">{t.phonePairExpires.replace('{t}', countdown(left))}</span>
          </div>
        ) : (
          pairing && live && <span className="cfg-faz">{t.phonePairExpired}</span>
        )}
        <span className="acao">
          <button type="button" className="cheio" disabled={!live} onClick={() => void pair()}>
            {pairing ? t.phonePairNew : t.phonePairButton}
          </button>
        </span>
      </article>
      <article className="cfg-ia">
        <header>
          <span className="cfg-nome">
            <b>{t.phoneDevices}</b>
            <small>{devices.length === 0 ? t.phoneNoDevices : String(devices.length)}</small>
          </span>
        </header>
        {devices.length > 0 && (
          <ul className="celular-aparelhos">
            {devices.map((device) => (
              <li key={device.id}>
                <span>
                  <b>{device.name}</b>
                  <small title={t.phonePaired.replace('{d}', shortDate(device.created_ms))}>{[t.phoneSeen.replace('{t}', ago(device.seen_ms, now)), device.notifies ? t.phoneNotifies : ''].filter(Boolean).join(' · ')}</small>
                </span>
                <button
                  type="button"
                  className={`vidro-btn ${forgetting === device.id ? 'confirmar' : ''}`}
                  onBlur={() => setForgetting(null)}
                  onClick={() => (forgetting === device.id ? void act(() => forgetDevice(device.id)) : setForgetting(device.id))}
                >
                  {forgetting === device.id ? t.phoneForgetSure : t.phoneForget}
                </button>
              </li>
            ))}
          </ul>
        )}
      </article>
      {error && <p className="nota failed quebra">{error}</p>}
    </div>
  )
}
