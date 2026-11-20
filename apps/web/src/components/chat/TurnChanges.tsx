import { useState } from 'react'
import { type DraftFile, type TurnChanges as Changes, fetchTurnDiff } from '../../api'
import { t } from '../../i18n'
import { FileDiff } from './DraftCard'

/** One file's part of a unified diff, keyed by its path, with what kind of change it is. */
function split(patch: string): Map<string, Pick<DraftFile, 'diff' | 'status' | 'binary'>> {
  const parts = new Map<string, Pick<DraftFile, 'diff' | 'status' | 'binary'>>()
  for (const block of patch.split(/^(?=diff --git )/m)) {
    const path = /^diff --git a\/(.+?) b\//.exec(block)?.[1]
    if (!path) continue
    const status = /^new file mode/m.test(block) ? 'added' : /^deleted file mode/m.test(block) ? 'deleted' : 'modified'
    const body = block.split('\n').filter((line) => !/^(diff --git|index |new file|deleted file|--- |\+\+\+ )/.test(line)).join('\n')
    parts.set(path, { diff: body.trim(), status, binary: /^Binary files/m.test(block) })
  }
  return parts
}

interface TurnChangesProps {
  bot: string
  thread: string
  ms: number
  changes: Changes
  undone: boolean
  busy: boolean
  onUndo: () => void
}

/** What a turn of an AI with full access changed in the project: its files, their diff on a tap, and an undo. */
export function TurnChanges({ bot, thread, ms, changes, undone, busy, onUndo }: TurnChangesProps) {
  const [parts, setParts] = useState<ReturnType<typeof split> | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [sure, setSure] = useState(false)
  const added = changes.files.reduce((sum, f) => sum + (f.added ?? 0), 0)
  const removed = changes.files.reduce((sum, f) => sum + (f.removed ?? 0), 0)
  const load = () => {
    if (parts) return setParts(null)
    fetchTurnDiff(bot, thread, ms)
      .then(({ patch }) => setParts(split(patch)))
      .catch((failure: unknown) => setError(failure instanceof Error ? failure.message : String(failure)))
  }
  const count = changes.files.length === 1 ? t.draftOne : t.draftFiles.replace('{n}', String(changes.files.length))
  return (
    <div className={`rascunho turno ${undone ? 'desfeito' : ''}`}>
      <header>
        <b>{t.turnTitle}</b>
        <small>{`${count} · +${added} −${removed}`}</small>
      </header>
      {parts ? (
        <ul>
          {changes.files.map((file) => {
            const part = parts.get(file.path)
            const draftFile: DraftFile = { path: file.path, status: part?.status ?? 'modified', added: file.added ?? 0, removed: file.removed ?? 0, diff: part?.diff ?? '', binary: part?.binary ?? file.added === null, truncated: false }
            return <FileDiff key={file.path} file={draftFile} open={changes.files.length <= 2} />
          })}
        </ul>
      ) : (
        <ul className="turno-arquivos">
          {changes.files.map((file) => (
            <li key={file.path}>
              <code>{file.path}</code>
              <small>{file.added === null ? t.binaryFile : `+${file.added} −${file.removed ?? 0}`}</small>
            </li>
          ))}
        </ul>
      )}
      {error && <p className="nota failed quebra">{error}</p>}
      <footer>
        <button type="button" className="link" onClick={load}>
          {parts ? t.turnHideDiff : t.turnShowDiff}
        </button>
        {undone ? (
          <span className="nota">{t.turnUndone}</span>
        ) : (
          <button
            type="button"
            className={`link ${sure ? 'perigo' : ''}`}
            disabled={busy}
            onBlur={() => setSure(false)}
            onClick={() => {
              if (!sure) return setSure(true)
              setSure(false)
              onUndo()
            }}
          >
            {sure ? t.turnUndoSure : t.turnUndo}
          </button>
        )}
        {changes.head_moved && !undone && <span className="nota">{t.turnCommitted}</span>}
      </footer>
    </div>
  )
}
