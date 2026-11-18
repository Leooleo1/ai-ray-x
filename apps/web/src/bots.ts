// The person's own AI as bots in their projects: shared state for the sidebar and the chat.

import type { AvatarDefinition } from 'open-bot-avatar'
import { useCallback, useEffect, useState } from 'react'
import face from '../../../assets/agents/bot.json'
import { type BotAction, type BotAgent, type BotsState, botAction, fetchBots } from './api'

/** While a bot answers, the panel asks for news this often. */
const POLL_MS = 1500

export const AGENT_LABEL: Record<BotAgent, string> = { claude: 'Claude Code', codex: 'Codex' }

export interface Face {
  name: string
  color: string
  avatar: AvatarDefinition
}

/** One monitor-shaped avatar for every bot, in the color of its CLI. */
export const BOT_FACES: Record<BotAgent, Face> = {
  claude: { name: 'Claude Code', color: '#e8754a', avatar: face as AvatarDefinition },
  codex: { name: 'Codex', color: '#6f95f2', avatar: face as AvatarDefinition },
}

export interface Bots {
  state: BotsState
  error: string | null
  /** Runs one action; resolves with the new state, or null when it failed (see `error`). */
  act: (action: BotAction, body: Record<string, string>) => Promise<BotsState | null>
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

  const act = useCallback(async (action: BotAction, body: Record<string, string>) => {
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
