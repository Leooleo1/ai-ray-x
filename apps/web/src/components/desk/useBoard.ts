import { useEffect, useState } from 'react'
import { type TaskBoardState, fetchTasks, taskAction } from '../../api'

/** How often the board looks at its tasks (they move when a turn ends). */
const POLL_MS = 3000

export type TaskActionName = Parameters<typeof taskAction>[0]
export type ActOnBoard = (action: TaskActionName, body: Record<string, unknown>) => Promise<boolean>

export interface BoardHandle {
  board: TaskBoardState | null
  error: string | null
  act: ActOnBoard
}

const messageOf = (error: unknown): string => (error instanceof Error ? error.message : String(error))

/** The team's tasks, read again every few seconds, and the actions that move them. */
export function useBoard(): BoardHandle {
  const [board, setBoard] = useState<TaskBoardState | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    const load = () =>
      fetchTasks()
        .then((next) => alive && setBoard(next))
        .catch((failure: unknown) => alive && setError(messageOf(failure)))
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [])
  const act: ActOnBoard = async (action, body) => {
    setError(null)
    try {
      setBoard(await taskAction(action, body))
      return true
    } catch (failure: unknown) {
      setError(messageOf(failure))
      return false
    }
  }
  return { board, error, act }
}
