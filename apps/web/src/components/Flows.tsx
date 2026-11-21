import { Skeleton } from './Skeleton'
import { notify } from '../notify'
import { useCallback, useEffect, useRef, useState } from 'react'
import { type Flow, type FlowAction, fetchFlows, flowAct, projectBots, saveFlow } from '../api'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { triggerLine, stepMissing, triggerMissing } from './flows/describe'
import { type Draft, FlowCanvas } from './flows/FlowCanvas'
import { FlowIcon } from './flows/icons'
import './flows/fluxos.css'

/** Polls fast while a run moves, slowly otherwise. */
const LIVE_MS = 3_000
const IDLE_MS = 15_000
/** Edits are saved this long after the last keystroke. */
const SAVE_DELAY_MS = 700

const messageOf = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure))
const isLive = (flow: Flow | undefined) => flow?.run?.status === 'running' || flow?.run?.status === 'waiting'
const draftOf = (flow: Flow): Draft => ({ id: flow.id, name: flow.name, enabled: flow.enabled, trigger: flow.trigger, steps: flow.steps })
const complete = (draft: Draft) => draft.name.trim() !== '' && !(draft.trigger && triggerMissing(draft.trigger)) && draft.steps.every((step) => !stepMissing(step))

/** Workflows: a trigger and steps in a line, drawn on a board, saved as the person edits. */
interface FlowsProps {
  bots: Bots
  /** A workflow to open, picked elsewhere (the ⌘K palette). */
  pick?: { id: string; n: number } | null
}

export function Flows({ bots, pick }: FlowsProps) {
  const [flows, setFlows] = useState<Flow[] | null>(null)
  const [chosen, setChosen] = useState<string | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [error, setError] = useState<string | null>(null)
  const editing = useRef(false)
  useEffect(() => {
    if (pick) setChosen(pick.id)
  }, [pick])
  const load = useCallback(() => fetchFlows().then(({ flows: next }) => setFlows(next)).catch((failure: unknown) => setError(messageOf(failure))), [])
  const live = flows?.some(isLive) ?? false
  useEffect(() => {
    void load()
    const timer = window.setInterval(() => void load(), live ? LIVE_MS : IDLE_MS)
    return () => window.clearInterval(timer)
  }, [load, live])
  const flow = flows?.find((f) => f.id === chosen) ?? flows?.[0]
  // The board follows the saved workflow, except while the person is typing in it.
  useEffect(() => {
    if (flow && (!editing.current || draft?.id !== flow.id)) setDraft(draftOf(flow))
  }, [flow])
  useEffect(() => {
    if (!draft || !editing.current || !complete(draft)) return undefined
    const timer = window.setTimeout(() => {
      saveFlow(draft)
        .then(() => {
          editing.current = false
          setError(null)
          return load()
        })
        .catch((failure: unknown) => setError(messageOf(failure)))
    }, SAVE_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [draft, load])
  const edit = (next: Draft) => {
    editing.current = true
    setDraft(next)
  }
  const act = (action: FlowAction) => {
    if (!flow) return
    flowAct(action, flow.id)
      .then(({ flows: next }) => {
        setFlows(next)
        setError(null)
        if (action === 'delete') setChosen(null)
        const note = { run: t.noticeRunning, approve: t.noticeApproved, decline: t.noticeStopped, stop: t.noticeStopped, delete: t.noticeDeleted }[action]
        notify(note, action === 'run' || action === 'approve' ? 'ok' : 'undo')
      })
      .catch((failure: unknown) => setError(messageOf(failure)))
  }
  const create = () =>
    saveFlow({ name: t.flowUntitled, enabled: false, trigger: null, steps: [] })
      .then(({ id }) => {
        editing.current = false
        setChosen(id)
        bots.refresh()
        return load()
      })
      .catch((failure: unknown) => setError(messageOf(failure)))
  if (!flows) return <main className="fluxos">{error ? <p className="nota failed quebra">{error}</p> : <Skeleton rows={4} />}</main>
  const running = isLive(flow)
  const run = flow?.run ?? flow?.runs[0] ?? null
  const scheduled = draft?.trigger && draft.trigger.kind !== 'manual'
  return (
    <main className="fluxos">
      <aside className="fluxo-lista">
        <button type="button" className="fluxo-criar" onClick={() => void create()}>
          <FlowIcon name="plus" />
          {t.flowNew}
        </button>
        <ul>
          {flows.map((f) => (
            <li key={f.id}>
              <button type="button" className={f.id === flow?.id ? 'ativo' : ''} onClick={() => setChosen(f.id)}>
                <b>{f.name}</b>
                <small>
                  <i className={`fluxo-ponto ${f.run?.status === 'waiting' ? 'espera' : isLive(f) ? 'roda' : f.enabled && f.trigger && f.trigger.kind !== 'manual' ? 'ligado' : ''}`} />
                  {f.run?.status === 'waiting' ? t.flowWaitsYou : isLive(f) ? t.flowRunning : f.trigger ? triggerLine(f.trigger, projectBots(bots.state.bots)) : t.flowNoTrigger}
                </small>
              </button>
            </li>
          ))}
        </ul>
        {flows.length === 0 && <p className="fluxo-vazio">{t.flowNone}</p>}
      </aside>
      <section className="fluxo-palco">
        {draft && flow ? (
          <>
            <header className="fluxo-topo">
              <input className="fluxo-nome" value={draft.name} maxLength={60} aria-label={t.flowName} onChange={(event) => edit({ ...draft, name: event.target.value })} />
              {scheduled && (
                <label className="fluxo-ligar">
                  <span>{draft.enabled ? t.flowOn : t.flowOff}</span>
                  <button type="button" className={`config-chave ${draft.enabled ? 'ligada' : ''}`} role="switch" aria-checked={draft.enabled} onClick={() => edit({ ...draft, enabled: !draft.enabled })}>
                    <i />
                  </button>
                </label>
              )}
              {running ? (
                <button type="button" className="bv-btn" onClick={() => act('stop')}>
                  {t.flowStop}
                </button>
              ) : (
                <button type="button" className="bv-btn cheio" disabled={draft.steps.length === 0 || !complete(draft) || editing.current} onClick={() => act('run')}>
                  {t.flowRunNow}
                </button>
              )}
            </header>
            <FlowCanvas draft={draft} bots={projectBots(bots.state.bots)} channels={bots.state.channels} run={run} live={running} error={error} onChange={edit} onAction={act} />
          </>
        ) : (
          <div className="fluxo-tabuleiro">
            <div className="fluxo-comeco">
              <span className="fluxo-raio">
                <FlowIcon name="trigger" />
              </span>
              <b>{t.flowStartTitle}</b>
              <p>{t.flowStartText}</p>
              <button type="button" className="bv-btn cheio" onClick={() => void create()}>
                <FlowIcon name="plus" />
                {t.flowNew}
              </button>
            </div>
          </div>
        )}
      </section>
    </main>
  )
}
