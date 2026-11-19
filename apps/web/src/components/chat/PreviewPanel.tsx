import { useEffect, useState } from 'react'
import { type Bot, type Preview, botPreview, botPreviewShot } from '../../api'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import './preview.css'

/** How often the panel asks how the app is doing while it is open. */
const POLL_MS = 2000
const SIZES = { desktop: { width: 1280, height: 800 }, phone: { width: 390, height: 844 } }
type Size = keyof typeof SIZES

interface PreviewPanelProps {
  bot: Bot
  bots: Bots
  onClose: () => void
}

/** The project's app inside the panel: its dev command, the page, its output and screenshots. */
export function PreviewPanel({ bot, bots, onClose }: PreviewPanelProps) {
  const [preview, setPreview] = useState<Preview | null>(null)
  const [command, setCommand] = useState('')
  const [tab, setTab] = useState<'page' | 'simulator'>('page')
  const [size, setSize] = useState<Size>('desktop')
  const [shot, setShot] = useState<string | null>(null)
  const [shooting, setShooting] = useState(false)
  const [failed, setFailed] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    const load = () =>
      botPreview(bot.id)
        .then((next) => {
          if (!alive) return
          setPreview(next)
          setCommand((typed) => typed || next.command || next.suggestion || '')
        })
        .catch((error: unknown) => alive && setFailed(error instanceof Error ? error.message : String(error)))
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [bot.id])
  const take = async () => {
    setShooting(true)
    setFailed(null)
    try {
      const { width, height } = SIZES[size]
      const taken = await botPreviewShot(bot.id, tab, preview?.url ?? '', width, height)
      setShot(taken.image)
    } catch (error: unknown) {
      setFailed(error instanceof Error ? error.message : String(error))
    } finally {
      setShooting(false)
    }
  }
  const run = () => void bots.act('preview_start', { id: bot.id, command })
  const stop = () => void bots.act('preview_stop', { id: bot.id })
  return (
    <aside className="ver" aria-label={t.previewTitle}>
      <header className="ver-topo">
        <b>{t.previewTitle}</b>
        {preview?.simulator && (
          <div className="ver-abas" role="tablist">
            <button type="button" role="tab" aria-selected={tab === 'page'} className={tab === 'page' ? 'ativa' : ''} onClick={() => setTab('page')}>
              {t.previewBrowser}
            </button>
            <button type="button" role="tab" aria-selected={tab === 'simulator'} className={tab === 'simulator' ? 'ativa' : ''} onClick={() => setTab('simulator')}>
              {`${t.previewSimulator} · ${preview.simulator}`}
            </button>
          </div>
        )}
        <button type="button" className="voltar" onClick={onClose}>
          {t.close}
        </button>
      </header>
      <div className="ver-corpo">
        {tab === 'page' && (
          <>
            {!preview?.running ? (
              <form
                className="ver-comando"
                onSubmit={(event) => {
                  event.preventDefault()
                  run()
                }}
              >
                <label>
                  <span>{t.previewCommand}</span>
                  <input value={command} placeholder={t.previewNoSuggestion} onChange={(event) => setCommand(event.target.value)} />
                </label>
                <button type="submit" className="cheio" disabled={!command.trim()}>
                  {t.previewRun}
                </button>
                <small>{preview?.command && preview.exit_code !== null ? t.previewExited.replace('{n}', String(preview.exit_code)) : t.previewRunNote}</small>
              </form>
            ) : (
              <div className="ver-barra">
                {preview.url ? (
                  <a href={preview.url} target="_blank" rel="noreferrer">
                    {preview.url}
                  </a>
                ) : (
                  <span className="ver-esperando">{t.previewWaiting}</span>
                )}
                <span className="ver-tamanhos" role="group">
                  {(Object.keys(SIZES) as Size[]).map((key) => (
                    <button key={key} type="button" aria-pressed={size === key} className={size === key ? 'ativo' : ''} onClick={() => setSize(key)}>
                      {key === 'desktop' ? t.previewDesktop : t.previewPhone}
                    </button>
                  ))}
                </span>
                <button type="button" className="vidro-btn" disabled={!preview.url || shooting} onClick={() => void take()}>
                  {shooting ? t.previewShooting : t.previewShot}
                </button>
                <button type="button" className="vidro-btn" onClick={stop}>
                  {t.previewStop}
                </button>
              </div>
            )}
            {preview?.running && preview.url && (
              <div className={`ver-moldura ${size}`}>
                <iframe title={t.previewTitle} src={preview.url} />
              </div>
            )}
            {preview?.log && (
              <details className="ver-log" open={!preview.url}>
                <summary>{t.previewLog}</summary>
                <pre>{preview.log}</pre>
              </details>
            )}
          </>
        )}
        {tab === 'simulator' && (
          <div className="ver-barra">
            <span className="ver-esperando">{preview?.simulator}</span>
            <button type="button" className="vidro-btn" disabled={shooting} onClick={() => void take()}>
              {shooting ? t.previewShooting : t.previewSimShot}
            </button>
          </div>
        )}
        {shot && <img className={`ver-print ${tab === 'simulator' ? 'simulador' : ''}`} src={shot} alt={t.previewShot} />}
        {(failed ?? bots.error) && <p className="nota failed quebra">{failed ?? bots.error}</p>}
      </div>
    </aside>
  )
}
