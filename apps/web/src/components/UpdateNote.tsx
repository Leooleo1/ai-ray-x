import { useEffect, useState } from 'react'
import { type AppVersion, IS_REMOTE, fetchAppVersion, updateApp } from '../api'
import { t } from '../i18n'
import './update.css'

/** How often the panel asks whether the app found a new version (the app itself checks every few hours). */
const POLL_MS = 5 * 60 * 1000

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** "raiox 0.1.3 is available · Update", at the right of the page title, in the desktop app only. */
export function UpdateNote() {
  const [app, setApp] = useState<AppVersion | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (IS_REMOTE) return
    let alive = true
    const load = () => fetchAppVersion().then((next) => alive && setApp(next)).catch(() => undefined)
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    // "Check for updates" in the profile menu found something: show it now.
    const now = () => void load()
    window.addEventListener('raiox:update', now)
    return () => {
      alive = false
      window.clearInterval(timer)
      window.removeEventListener('raiox:update', now)
    }
  }, [])
  if (!app?.available || !app.can_update) return null
  const update = async () => {
    setError(null)
    try {
      setApp(await updateApp())
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const installing = app.installing && !error
  return (
    <div className={`atualizacao ${installing ? 'instalando' : ''}`} role="status" aria-label={t.updateReady.replace('{v}', app.available)}>
      <span className="atu-icone" aria-hidden="true">
        <svg viewBox="0 0 24 24">
          <path d="M12 17V7M7.5 11.5 12 7l4.5 4.5" />
        </svg>
      </span>
      <span className="atu-texto">
        <small>{installing ? t.updating : error ?? t.updateNew}</small>
        <b>
          <span className="atu-de">{app.version}</span>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M5 12h13M13 7l5 5-5 5" />
          </svg>
          <span className="atu-versao">{app.available}</span>
        </b>
      </span>
      {installing ? (
        <span className="atu-progresso" aria-hidden="true">
          {Array.from({ length: 8 }, (_, i) => (
            <i key={i} style={{ animationDelay: `${i * 110}ms` }} />
          ))}
        </span>
      ) : (
        <button type="button" className="cheio" onClick={() => void update()}>
          {t.updateAction}
        </button>
      )}
    </div>
  )
}
