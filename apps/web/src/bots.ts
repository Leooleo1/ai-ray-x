// The person's own AI as bots in their projects: shared state for the sidebar and the chat.

import { t } from './i18n'
import { notify, type NoticeKind } from './notify'
import { useCallback, useEffect, useState } from 'react'
import { type BotAction, type BotsState, botAction, fetchBots } from './api'

/** While a bot answers, the panel asks for news this often. */
const POLL_MS = 1500

export interface Bots {
  state: BotsState
  error: string | null
  /** Runs one action; resolves with the new state, or null when it failed (see `error`). */
  act: (action: BotAction, body: Record<string, unknown>) => Promise<BotsState | null>
  /** Asks for the bots' state again (after settings change which CLIs are on). */
  refresh: () => void
}

/** Saved settings: one note, so a change never goes unconfirmed. */
const SAVED_ACTIONS = new Set(['configure', 'grants', 'routine_new', 'routine_toggle', 'routine_delete', 'routine_changed', 'link_toggle', 'limit', 'reviewer', 'auditor'])

/** What a finished action says at the foot of the panel; none for the ones that show their own result. */
function noticeFor(action: string, body: Record<string, unknown>): { text: string; kind: NoticeKind } | null {
  if (action === 'approve' || action === 'request_approve') return { text: t.noticeApproved, kind: 'ok' }
  if (action === 'request_discard') return { text: t.noticeDiscarded, kind: 'undo' }
  if (action === 'draft') {
    if (body.action === 'apply') return { text: t.noticeApplied, kind: 'ok' }
    if (body.action === 'discard') return { text: t.noticeDiscarded, kind: 'undo' }
    if (body.action === 'undo') return { text: t.noticeUndone, kind: 'undo' }
  }
  return SAVED_ACTIONS.has(action) ? { text: t.noticeSaved, kind: 'ok' } : null
}

export function useBots(initial: BotsState): Bots {
  const [state, setState] = useState(initial)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => setState(initial), [initial])

  const busy = state.bots.some((bot) => bot.busy)
  useEffect(() => {
    if (!busy) return undefined
    const timer = window.setInterval(() => {
      fetchBots()
        .then(setState)
        .catch((failure: unknown) => setError(failure instanceof Error ? failure.message : String(failure)))
    }, POLL_MS)
    return () => window.clearInterval(timer)
  }, [busy])

  const act = useCallback(async (action: BotAction, body: Record<string, unknown>) => {
    setError(null)
    try {
      const next = await botAction(action, body)
      setState(next)
      const note = noticeFor(action, body)
      if (note) notify(note.text, note.kind)
      return next
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure))
      return null
    }
  }, [])

  const refresh = useCallback(() => {
    fetchBots()
      .then(setState)
      .catch((failure: unknown) => setError(failure instanceof Error ? failure.message : String(failure)))
  }, [])

  return { state, error, act, refresh }
}
