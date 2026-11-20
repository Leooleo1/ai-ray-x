import { useEffect, useState } from 'react'
import type { Bot, Task, TaskBoardState, TaskStatus } from '../api'
import type { Bots } from '../bots'
import { t } from '../i18n'
import type { ChatNav } from './chat/nav'
import { waitingOf } from './chat/waiting'
import { Lane } from './desk/Lane'
import { NewTask } from './desk/NewTask'
import { TaskCard } from './desk/TaskCard'
import { Team } from './desk/Team'
import { useBoard } from './desk/useBoard'
import { ApprovalCard, RequestCard } from './desk/WaitCards'
import { WorkCard } from './desk/WorkCard'
import './desk.css'
import './desk/mesa.css'

const TICK_MS = 1000
/** Finished tasks shown before "Show all". */
const SHOWN_DONE = 6

function useNow(): number {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => window.clearInterval(timer)
  }, [])
  return now
}

interface Placed {
  task: Task
  index: number
}

const byProject = (a: Bot, b: Bot) => a.name.localeCompare(b.name) || a.agent.localeCompare(b.agent)

/** The board's task a busy bot is doing now, when its turn is one. */
function taskOf(bot: Bot, board: TaskBoardState | null): Placed | undefined {
  const index = board?.tasks.findIndex((task, i) => task.status === 'doing' && task.bot === bot.id && board.threads[i] === bot.busy_thread) ?? -1
  return board && index >= 0 ? { task: board.tasks[index], index } : undefined
}

interface DeskProps {
  bots: Bots
  nav: ChatNav
}

/** The desk as one board: what waits to start, what the AIs do now, what waits for the person, and what is done. */
export function Desk({ bots, nav }: DeskProps) {
  const now = useNow()
  const { board, error, act } = useBoard()
  const [allDone, setAllDone] = useState(false)
  const all = bots.state.bots
  const botOf = (id: string) => all.find((b) => b.id === id)
  const placed = (status: TaskStatus): Placed[] => (board?.tasks ?? []).map((task, index) => ({ task, index })).filter(({ task }) => task.status === status)

  const working = all.filter((b) => b.busy).sort(byProject)
  const live = working.map((bot) => ({ bot, doing: taskOf(bot, board) }))
  const shownLive = new Set(live.flatMap(({ doing }) => (doing ? [doing.task.id] : [])))
  const doing = placed('doing').filter(({ task }) => !shownLive.has(task.id))
  const requests = bots.state.requests ?? []
  const approvals = all.flatMap(waitingOf).sort((a, b) => b.ms - a.ms)
  const waiting = placed('waiting')
  const done = placed('done').sort((a, b) => b.task.updated_ms - a.task.updated_ms)
  const doneShown = allDone ? done : done.slice(0, SHOWN_DONE)
  // Until the tasks first arrive, a lane says so instead of looking empty.
  const emptyOr = (text: string) => (board ? text : t.wikiLoading)

  const open = ({ task, index }: Placed) => {
    const thread = board?.threads[index]
    if (thread && task.answer_ms) nav.openMessage(task.bot, thread, task.answer_ms)
    else nav.openBot(task.bot)
  }
  const card = (item: Placed) => {
    const { task } = item
    const from = task.from !== 'person' && task.from !== task.bot ? botOf(task.from) : undefined
    return <TaskCard key={task.id} task={task} owner={botOf(task.bot)} from={from} act={act} onOpen={() => open(item)} />
  }

  return (
    <main className="mesa mesa-quadro">
      <NewTask bots={all} board={board} act={act} />
      {(error || bots.error) && <p className="nota failed quebra">{error ?? bots.error}</p>}
      <div className="mesa-raias">
        <Lane status="todo" title={t.boardTodo} count={placed('todo').length} empty={emptyOr(t.deskTodoEmpty)}>
          {placed('todo').map(card)}
        </Lane>
        <Lane status="doing" title={t.boardDoing} tone="cobalto" count={live.length + doing.length} empty={t.deskDoingEmpty}>
          {live.map(({ bot, doing: task }) => (
            <WorkCard key={bot.id} bot={bot} bots={bots} nav={nav} now={now} task={task?.task.title} />
          ))}
          {doing.map(card)}
        </Lane>
        <Lane status="waiting" title={t.boardWaiting} tone="brasa" count={requests.length + approvals.length + waiting.length} empty={t.deskWaitingEmpty}>
          {requests.map((request) => (
            <RequestCard key={request.id} request={request} bots={bots} />
          ))}
          {approvals.map((item) => (
            <ApprovalCard key={`${item.bot.id}:${item.thread}:${item.kind}:${item.ms}:${item.text}`} item={item} nav={nav} />
          ))}
          {waiting.map(card)}
        </Lane>
        <Lane status="done" title={t.boardDone} count={done.length} empty={emptyOr(t.deskDoneEmpty)}>
          {doneShown.map(card)}
          {done.length > SHOWN_DONE && (
            <button type="button" className="acao-texto mesa-todas" onClick={() => setAllDone(!allDone)}>
              {allDone ? t.deskFewerDone : t.deskAllDone.replace('{n}', String(done.length))}
            </button>
          )}
        </Lane>
      </div>
      <Team bots={all} nav={nav} now={now} />
    </main>
  )
}
