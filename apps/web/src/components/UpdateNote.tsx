import { useEffect, useState } from 'react'
import { type AppVersion, IS_REMOTE, fetchAppVersion, updateApp } from '../api'
import { t } from '../i18n'

/** How often the panel asks whether the app found a new version (the app itself checks every few hours). */
const POLL_MS = 5 * 60 * 1000

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** "raiox 0.1.1 is available · Update", above the profile, in the desktop app only. */
export function UpdateNote() {
  const [app, setApp] = useState<AppVersion | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (IS_REMOTE) return
    let alive = true
    const load = () => fetchAppVersion().then((next) => alive && setApp(next)).catch(() => undefined)
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
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
  return (
    <div className="atualizacao" role="status">
      <span>
        <b>{t.updateReady.replace('{v}', app.available)}</b>
        {(app.installing || error) && <small>{error ?? t.updating}</small>}
      </span>
      {!app.installing && (
        <button type="button" className="cheio" onClick={() => void update()}>
          {t.updateAction}
        </button>
      )}
    </div>
  )
}
