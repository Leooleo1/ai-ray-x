// The person's own AI as bots in their projects: shared state for the sidebar and the chat.

import { useCallback, useEffect, useState } from 'react'
import { type BotAction, type BotsState, botAction, fetchBots } from './api'

/** While a bot answers, the panel asks for news this often. */
const POLL_MS = 1500

export interface Bots {
  state: BotsState
  error: string | null
  /** Runs one action; resolves with the new state, or null when it failed (see `error`). */
  act: (action: BotAction, body: Record<string, unknown>) => Promise<BotsState | null>
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
      return next
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure))
      return null
    }
  }, [])

  return { state, error, act }
}
