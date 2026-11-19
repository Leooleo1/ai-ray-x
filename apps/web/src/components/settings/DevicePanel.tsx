import { useEffect, useState } from 'react'
import { type PushState, fetchPush, subscribePush, testPush, unsubscribePush } from '../../api'
import { t } from '../../i18n'
import './phone.css'

type Support = 'ready' | 'install' | 'blocked' | 'unsupported'

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** Opened from the home screen icon (an installed app), not a browser tab. */
const STANDALONE = window.matchMedia('(display-mode: standalone)').matches || (navigator as Navigator & { standalone?: boolean }).standalone === true
const IOS = /iPhone|iPad/.test(navigator.userAgent)

function support(): Support {
  if (!('serviceWorker' in navigator) || !('PushManager' in window) || !('Notification' in window)) return IOS && !STANDALONE ? 'install' : 'unsupported'
  return Notification.permission === 'denied' ? 'blocked' : 'ready'
}

/** raiox's public key (base64url) as the bytes the browser subscribes with. */
function keyBytes(text: string): Uint8Array<ArrayBuffer> {
  const base64 = text.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (text.length % 4)) % 4)
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))
}

/** Settings › Phone, on a paired device: installing raiox and its notifications here. */
export function DevicePanel() {
  const [push, setPush] = useState<PushState | null>(null)
  const [can, setCan] = useState<Support>(support)
  const [busy, setBusy] = useState(false)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    fetchPush()
      .then(setPush)
      .catch((failure: unknown) => setError(messageOf(failure)))
  }, [])
  const run = async (work: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await work()
    } catch (failure: unknown) {
      setError(messageOf(failure))
    } finally {
      setBusy(false)
    }
  }
  const turnOn = () =>
    run(async () => {
      if (!push) return
      if ((await Notification.requestPermission()) !== 'granted') {
        setCan('blocked')
        return
      }
      const registration = await navigator.serviceWorker.ready
      const subscription = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: keyBytes(push.public_key) })
      setPush(await subscribePush(subscription.toJSON()))
      await testPush()
      setSent(true)
    })
  const turnOff = () =>
    run(async () => {
      const registration = await navigator.serviceWorker.ready
      await (await registration.pushManager.getSubscription())?.unsubscribe()
      setPush(await unsubscribePush())
      setSent(false)
    })
  const sendTest = () =>
    run(async () => {
      await testPush()
      setSent(true)
    })
  const on = push?.on === true
  const status = on
    ? { tone: 'ok', text: sent ? t.deviceTestSent : t.deviceNotifyOn }
    : can === 'blocked'
      ? { tone: 'alerta', text: t.deviceBlocked }
      : { tone: '', text: can === 'ready' ? t.deviceNotifyOff : t.deviceNoNotify }
  return (
    <div className="cfg-grade celular">
      <article className="cfg-ia">
        <header>
          <span className="cfg-nome">
            <b>{t.deviceTitle}</b>
            <small>{t.deviceHint}</small>
          </span>
          <button
            type="button"
            className={`config-chave ${on ? 'ligada' : ''}`}
            role="switch"
            aria-checked={on}
            aria-label={t.deviceNotify}
            disabled={busy || !push || (can !== 'ready' && !on)}
            onClick={() => void (on ? turnOff() : turnOn())}
          >
            <i />
            {t.deviceNotify}
          </button>
        </header>
        <p className={`cfg-estado ${status.tone}`}>
          <i className={`ponto ${status.tone}`} />
          <span>{status.text}</span>
        </p>
        {can === 'install' && <p className="cfg-faz">{t.deviceInstallIos}</p>}
        {can !== 'install' && !STANDALONE && <p className="cfg-faz">{IOS ? t.deviceInstallIos : t.deviceInstall}</p>}
        {on && (
          <span className="acao">
            <button type="button" className="vidro-btn" disabled={busy} onClick={() => void sendTest()}>
              {t.deviceTest}
            </button>
          </span>
        )}
        {error && <p className="nota failed quebra">{error}</p>}
      </article>
    </div>
  )
}
