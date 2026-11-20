import { useState } from 'react'
import { type Change, type Outcome, undoChange } from '../../api'
import { suggestionTitle } from '../../diet'
import { compact, locale, percent, t } from '../../i18n'
import { settingLabel } from '../Action'
import { PixelBar } from './PixelField'

const MIN_REQUESTS = 200
const MIN_SESSIONS = 2

interface LedgerCardProps {
  changes: Change[]
  outcomes: Outcome[]
  targetDir: string
  botByDir: Map<string, string>
  highlighted: boolean
  onChanged: () => void
}

function projectNameOf(file: string): string {
  const parts = file.split('/.claude/')[0]?.split('/') ?? []
  return parts.pop() || file
}

function projectDirOf(file: string): string {
  return file.split('/.claude/')[0] ?? ''
}

/** Bento 5: Full-width Memo ledger with pixel progress and quiet 2-click undo. */
export function LedgerCard({ changes, outcomes, targetDir, botByDir, highlighted, onChanged }: LedgerCardProps) {
  const inTarget = (c: Change) => c.file.startsWith(`${targetDir}/`)
  const hasMultipleProjects = new Set(changes.map((c) => projectDirOf(c.file))).size > 1
  const [scope, setScope] = useState<'project' | 'all'>(() => (changes.some(inTarget) ? 'project' : 'all'))
  const [confirmUndoId, setConfirmUndoId] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const visible = [...changes]
    .filter((c) => (scope === 'project' && hasMultipleProjects ? inTarget(c) : true))
    .sort((a, b) => b.applied_ms - a.applied_ms)

  const doUndo = async (change: Change) => {
    setBusyId(change.id)
    setError(null)
    try {
      const botId = botByDir.get(projectDirOf(change.file))
      await undoChange(change.id, botId)
      setConfirmUndoId(null)
      onChanged()
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyId(null)
    }
  }

  return (
    <section id="efi-historico" className={`cartao efi-historico ${highlighted ? 'efi-foco' : ''}`}>
      <header className="efi-card-topo">
        <h2>
          {t.efiLedgerTitle}
          <small>{t.efiLedgerHint}</small>
        </h2>
        {hasMultipleProjects && (
          <div className="efi-acoes">
            <button type="button" className={`acao-texto ${scope === 'project' ? 'ativo' : ''}`} onClick={() => setScope('project')}>
              {t.efiLedgerThisProject}
            </button>
            <button type="button" className={`acao-texto ${scope === 'all' ? 'ativo' : ''}`} onClick={() => setScope('all')}>
              {t.efiLedgerAllProjects}
            </button>
          </div>
        )}
      </header>

      {error && <div className="nota failed">{error}</div>}

      {visible.length === 0 ? (
        <p className="efi-vazio">{t.efiLedgerEmpty}</p>
      ) : (
        <ul className="efi-ledger-lista">
          {visible.map((change) => {
            const undone = change.undone_ms !== null
            const outcome = outcomes.find((o) => o.change_id === change.id)
            const appliedDate = new Date(change.applied_ms).toLocaleDateString(locale, { day: '2-digit', month: 'short' })
            const reqProgress = outcome ? Math.min(1, outcome.after.requests / MIN_REQUESTS) : 0
            const sesProgress = outcome ? Math.min(1, outcome.after.sessions / MIN_SESSIONS) : 0
            const progress = Math.min(1, (reqProgress + sesProgress) / 2)

            return (
              <li key={change.id} className={`efi-ledger-item ${undone ? 'desfeito' : ''}`}>
                <div className="efi-ledger-info">
                  <div className="efi-ledger-titulo">
                    <i className={`ponto ${undone ? '' : 'ok'}`} aria-hidden="true" />
                    <span>{suggestionTitle(change.suggestion, change.suggestion)}</span>
                    <span className="efi-meta">{settingLabel(change)}</span>
                  </div>
                  <span className="efi-ledger-sub" title={`${change.key}=${change.after} · ${change.file}`}>
                    {`${projectNameOf(change.file)} · ${appliedDate}${undone ? ` · ${t.efiLedgerUndone}` : ''}`}
                  </span>
                </div>

                <div className="efi-ledger-medicao">
                  {outcome ? (
                    outcome.conclusive ? (
                      <div className="efi-acoes">
                        <span className={`efi-meta ${outcome.context_change <= 0 ? 'ok' : 'alerta'}`}>
                          {`${t.efiLedgerMeasured}: ${outcome.context_change > 0 ? '+' : ''}${percent(outcome.context_change)}`}
                        </span>
                        <span className="efi-meta">
                          {`${compact(outcome.before.avg_context)} → ${compact(outcome.after.avg_context)}`}
                        </span>
                      </div>
                    ) : (
                      <>
                        <div className="efi-acoes">
                          <span className="efi-meta">{t.efiLedgerMeasuring}</span>
                          <span className="efi-meta">
                            {t.efiLedgerProgress
                              .replace('{req}', String(Math.min(outcome.after.requests, MIN_REQUESTS)))
                              .replace('{reqMax}', String(MIN_REQUESTS))
                              .replace('{ses}', String(Math.min(outcome.after.sessions, MIN_SESSIONS)))
                              .replace('{sesMax}', String(MIN_SESSIONS))}
                          </span>
                        </div>
                        <PixelBar ratio={progress} color="var(--cobalto)" cols={24} rows={1} />
                      </>
                    )
                  ) : (
                    <span className="efi-meta">{undone ? t.efiLedgerUndone : t.applied}</span>
                  )}
                </div>

                <div className="efi-ledger-acoes">
                  {change.lesson && <span className="efi-meta ok">{t.efiLedgerLessonKept}</span>}
                  {!undone &&
                    (confirmUndoId === change.id ? (
                      <>
                        <button type="button" className="acao-texto confirmar" disabled={busyId === change.id} onClick={() => void doUndo(change)}>
                          {t.efiLedgerUndoSure}
                        </button>
                        <button type="button" className="acao-texto" disabled={busyId === change.id} onClick={() => setConfirmUndoId(null)}>
                          {t.cancel}
                        </button>
                      </>
                    ) : (
                      <button type="button" className="acao-texto" onClick={() => setConfirmUndoId(change.id)}>
                        {t.undo}
                      </button>
                    ))}
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}
