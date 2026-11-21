// The person's own AI as bots in their projects: shared state for the sidebar and the chat.

import { t } from './i18n'
import { notify, type NoticeKind } from './notify'
import { useCallback, useEffect, useRef, useState } from 'react'
import { type BotAction, type BotsState, botAction, fetchBots, fetchBotsStatus } from './api'
import { mergeBotsStatus } from './botsStatus'

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
  const current = useRef(initial)
  const generation = useRef(0)
  const pending = useRef(0)
  const publish = useCallback((next: BotsState) => {
    generation.current += 1
    current.current = next
    setState(next)
  }, [])
  useEffect(() => publish(initial), [initial, publish])

  const busy = state.bots.some((bot) => bot.busy) || state.channels.some((channel) => channel.working)
  useEffect(() => {
    if (!busy) return undefined
    let cancelled = false
    let timer: number
    const poll = async () => {
      const version = generation.current
      const stillCurrent = () => !cancelled && pending.current === 0 && generation.current === version
      if (stillCurrent()) {
        try {
          const status = await fetchBotsStatus()
          if (stillCurrent()) {
            const next = mergeBotsStatus(current.current, status) ?? await fetchBots()
            if (stillCurrent()) publish(next)
          }
        } catch (failure: unknown) {
          if (stillCurrent()) setError(failure instanceof Error ? failure.message : String(failure))
        }
      }
      if (!cancelled) timer = window.setTimeout(() => void poll(), POLL_MS)
    }
    timer = window.setTimeout(() => void poll(), POLL_MS)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [busy, publish])

  const act = useCallback(async (action: BotAction, body: Record<string, unknown>) => {
    generation.current += 1
    pending.current += 1
    setError(null)
    try {
      const next = await botAction(action, body)
      publish(next)
      const note = noticeFor(action, body)
      if (note) notify(note.text, note.kind)
      return next
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure))
      return null
    } finally {
      pending.current -= 1
    }
  }, [publish])

  const refresh = useCallback(() => {
    const version = ++generation.current
    pending.current += 1
    fetchBots()
      .then((next) => { if (generation.current === version) publish(next) })
      .catch((failure: unknown) => { if (generation.current === version) setError(failure instanceof Error ? failure.message : String(failure)) })
      .finally(() => { pending.current -= 1 })
  }, [publish])

  return { state, error, act, refresh }
}
