import { useState } from 'react'
import type { BotMessage, DraftFile } from '../../api'
import { t } from '../../i18n'

export type DraftState = 'open' | 'applied' | 'undone' | 'discarded'

/** Files with more lines than this start folded, so a big draft does not flood the chat. */
const OPEN_LINES = 40
const STATUS: Record<DraftFile['status'], string> = { modified: t.statusModified, added: t.statusAdded, deleted: t.statusDeleted }

interface DraftCardProps {
  message: BotMessage
  state: DraftState
  busy: boolean
  onAct: (action: 'apply' | 'discard' | 'undo') => void
}

/** What the AI changed in its draft copy, file by file, and the way to apply, discard or undo it. */
export function DraftCard({ message, state, busy, onAct }: DraftCardProps) {
  const files = message.data.files ?? []
  const blocked = message.data.blocked ?? []
  const added = files.reduce((sum, f) => sum + f.added, 0)
  const removed = files.reduce((sum, f) => sum + f.removed, 0)
  if (files.length === 0) return <p className="nota rascunho-vazio">{t.draftEmpty}</p>
  return (
    <div className="rascunho">
      <header>
        <b>{t.draftTitle}</b>
        <small>{`${files.length === 1 ? t.draftOne : t.draftFiles.replace('{n}', String(files.length))} · +${added} −${removed}`}</small>
      </header>
      <ul>
        {files.map((file) => (
          <FileDiff key={file.path} file={file} open={files.length <= 2 && file.diff.split('\n').length <= OPEN_LINES} />
        ))}
      </ul>
      {blocked.length > 0 && <p className="nota">{t.draftBlocked.replace('{list}', blocked.join(', '))}</p>}
      <footer>
        {state === 'open' && (
          <>
            <button type="button" className="cheio" disabled={busy} onClick={() => onAct('apply')}>
              {t.applyDraft}
            </button>
            <button type="button" className="link" disabled={busy} onClick={() => onAct('discard')}>
              {t.discardDraft}
            </button>
            <span className="nota">{t.draftHint}</span>
          </>
        )}
        {state === 'applied' && (
          <>
            <span className="estado">
              <i className="ponto ok" />
              {t.draftApplied}
            </span>
            <button type="button" className="link" disabled={busy} onClick={() => onAct('undo')}>
              {t.undoDraft}
            </button>
          </>
        )}
        {state === 'undone' && <span className="nota">{t.draftUndone}</span>}
        {state === 'discarded' && <span className="nota">{t.draftDiscarded}</span>}
      </footer>
    </div>
  )
}

/** One file: its status, name and size of the change; the diff unfolds on a tap. */
function FileDiff({ file, open }: { file: DraftFile; open: boolean }) {
  const [shown, setShown] = useState(open)
  return (
    <li className="rascunho-arquivo">
      <button type="button" aria-expanded={shown} onClick={() => setShown(!shown)}>
        <span className={`rascunho-status ${file.status}`}>{STATUS[file.status]}</span>
        <code>{file.path}</code>
        <small>{file.binary ? t.binaryFile : `+${file.added} −${file.removed}`}</small>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d={shown ? 'M6 15l6-6 6 6' : 'M6 9l6 6 6-6'} />
        </svg>
      </button>
      {shown && !file.binary && (
        <pre className="diff">
          {file.diff.split('\n').map((line, i) => (
            <span key={i} className={line.startsWith('+') && !line.startsWith('+++') ? 'mais' : line.startsWith('-') && !line.startsWith('---') ? 'menos' : line.startsWith('@@') ? 'trecho' : ''}>
              {`${line}\n`}
            </span>
          ))}
          {file.truncated && <span className="trecho">{t.diffTruncated}</span>}
        </pre>
      )}
    </li>
  )
}
