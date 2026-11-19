import { useState } from 'react'
import type { BotMessage, RunRequest } from '../../api'
import { t } from '../../i18n'

const TERMINAL_ICON = 'M4 6h16v12H4zM7 10l3 2-3 2M12 15h5'

interface RunRequestCardProps {
  request: RunRequest
  /** The latest answer, so the command can still run. */
  live: boolean
  busy: boolean
  onRun: (always: boolean) => void
}

/** A command the AI asks to run: the exact words, and Run or Run and always allow. */
export function RunRequestCard({ request, live, busy, onRun }: RunRequestCardProps) {
  if (!live && !request.refused) return null
  return (
    <div className="comando">
      <span className="comando-rotulo">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d={TERMINAL_ICON} />
        </svg>
        {t.runAsk}
      </span>
      <code className="comando-texto">{request.command}</code>
      {request.refused ? (
        <p className="nota">{t.runRefusedLabel.replace('{why}', t.runRefused[request.refused])}</p>
      ) : (
        <span className="acao">
          <button type="button" className="cheio" disabled={busy} onClick={() => onRun(false)}>
            {t.runOnce}
          </button>
          <button type="button" className="link" disabled={busy} onClick={() => onRun(true)}>
            {t.runAlways}
          </button>
          <span className="nota">{t.runWhere}</span>
        </span>
      )}
    </div>
  )
}

/** What a command did: how it ended and, on a tap, its output. */
export function RanBlock({ message }: { message: BotMessage }) {
  const [open, setOpen] = useState(false)
  const ran = message.data.ran
  if (!ran) return null
  const end = ran.timed_out ? t.ranTimedOut : ran.stopped ? t.ranStopped : ran.code === null ? t.ranNotStarted : t.ranCode.replace('{n}', String(ran.code))
  const seconds = `${(ran.duration_ms / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 })} s`
  const lines = ran.output ? ran.output.split('\n').length : 0
  return (
    <div className={`rodou ${ran.code === 0 ? 'ok' : 'falhou'}`}>
      <span className="rodou-linha">
        <i className="ponto" aria-hidden="true" />
        <span>{t.ranLine}</span>
        <code>{ran.command}</code>
        <small>{[end, seconds, message.data.allowed ? t.ranAllowed : ''].filter(Boolean).join(' · ')}</small>
      </span>
      {lines > 0 && (
        <button type="button" className="ver-mais" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? t.hideOutput : t.showOutput.replace('{n}', String(lines))}
        </button>
      )}
      {open && (
        <pre className="rodou-saida">
          {ran.cut && `${t.ranCut}\n`}
          {ran.output}
        </pre>
      )}
    </div>
  )
}
