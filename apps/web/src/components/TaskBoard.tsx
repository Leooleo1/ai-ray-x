import { type FormEvent, useEffect, useState } from 'react'
import { type Bot, type Task, type TaskBoardState, type TaskStatus, fetchTasks, taskAction } from '../api'
import { faceForBot } from '../avatars'
import { compact, t } from '../i18n'
import { Avatar } from './Avatar'
import type { ChatNav } from './chat/nav'
import { SelectChip } from './SelectChip'
import './board.css'

/** How often the board looks at its tasks (they move when a turn ends). */
const POLL_MS = 3000
const COLUMNS: { status: TaskStatus; label: () => string }[] = [
  { status: 'todo', label: () => t.boardTodo },
  { status: 'doing', label: () => t.boardDoing },
  { status: 'waiting', label: () => t.boardWaiting },
  { status: 'done', label: () => t.boardDone },
]

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

interface TaskBoardProps {
  bots: Bot[]
  nav: ChatNav
}

/** The team's board: tasks for the bots, by where they are, with what each cost. */
export function TaskBoard({ bots, nav }: TaskBoardProps) {
  const [board, setBoard] = useState<TaskBoardState | null>(null)
  const [title, setTitle] = useState('')
  const [bot, setBot] = useState('')
  const [deleting, setDeleting] = useState<string | null>(null)
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
  const act = async (action: Parameters<typeof taskAction>[0], body: Record<string, unknown>) => {
    setError(null)
    try {
      setBoard(await taskAction(action, body))
      return true
    } catch (failure: unknown) {
      setError(messageOf(failure))
      return false
    }
  }
  const chosen = bot || bots[0]?.id || ''
  const add = async (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() && chosen && (await act('add', { bot: chosen, title, detail: '' }))) setTitle('')
  }
  const botOf = (id: string) => bots.find((b) => b.id === id)
  const label = (b: Bot | undefined) => (b ? b.persona?.name || b.name : '?')
  const open = (task: Task, index: number) => {
    const thread = board?.threads[index]
    if (thread && task.answer_ms) nav.openMessage(task.bot, thread, task.answer_ms)
    else nav.openBot(task.bot)
  }
  if (!board) {
    return (
      <section className="mesa-secao quadro">
        <header className="quadro-topo">
          <h2>{t.boardTitle}</h2>
        </header>
        <p className="mesa-vazio">{error ?? t.wikiLoading}</p>
      </section>
    )
  }
  return (
    <section className="mesa-secao quadro">
      <header className="quadro-topo">
        <h2>{t.boardTitle}</h2>
        <label className="permissao quadro-auto">
          <button type="button" className={`config-chave ${board.auto ? 'ligada' : ''}`} role="switch" aria-checked={board.auto} onClick={() => void act('auto', { on: !board.auto })}>
            <i />
          </button>
          <span>
            <b>{t.boardAuto}</b>
            <small>
              {board.window
                ? t.boardAutoWindow.replace('{a}', String(board.window.start).padStart(2, '0')).replace('{b}', String(board.window.end).padStart(2, '0'))
                : t.boardAutoNote}
            </small>
          </span>
        </label>
      </header>
      {bots.length > 0 && (
        <form className="quadro-nova" onSubmit={(event) => void add(event)}>
          <input value={title} maxLength={120} placeholder={t.boardNewTitle} onChange={(event) => setTitle(event.target.value)} />
          <SelectChip
            className="quadro-bot"
            label={t.boardBotLabel}
            value={chosen}
            options={bots.map((b) => ({
              value: b.id,
              label: label(b),
              meta: b.name !== label(b) ? b.name : undefined,
              icon: <Avatar agent={faceForBot(b)} size={18} state="still" />,
            }))}
            onChange={setBot}
          />
          <button type="submit" className="cheio" disabled={!title.trim()}>
            {t.boardAdd}
          </button>
        </form>
      )}
      <div className="quadro-colunas">
        {COLUMNS.map((column) => {
          const items = board.tasks.map((task, index) => ({ task, index })).filter(({ task }) => task.status === column.status)
          return (
            <div key={column.status} className={`quadro-coluna ${column.status}`}>
              <h3>
                {column.label()}
                <small>{items.length}</small>
              </h3>
              {items.length === 0 && <p className="quadro-vazio">{t.boardEmpty}</p>}
              {items.map(({ task, index }) => {
                const owner = botOf(task.bot)
                // Who left it, when it is another bot (a bot's own follow-up says nothing more).
                const from = task.from !== 'person' && task.from !== task.bot ? botOf(task.from) : undefined
                return (
                  <article key={task.id} className="quadro-cartao">
                    <b>{task.title}</b>
                    <span className="quadro-quem">
                      {owner && <Avatar agent={faceForBot(owner)} size={18} state={task.status === 'doing' ? 'thinking' : 'still'} />}
                      <small>{[label(owner), task.tokens > 0 ? `${compact(task.tokens)} tokens` : '', from ? t.boardFromBot.replace('{who}', label(from)) : ''].filter(Boolean).join(' · ')}</small>
                    </span>
                    <span className="quadro-acoes">
                      {task.status === 'todo' && (
                        <button type="button" className="cheio" onClick={() => void act('start', { id: task.id })}>
                          {t.boardStart}
                        </button>
                      )}
                      {task.status === 'doing' && <small className="quadro-trabalhando">{t.boardWorking}</small>}
                      {task.status !== 'todo' && (
                        <button type="button" className="acao-texto" onClick={() => open(task, index)}>
                          {t.boardOpen}
                        </button>
                      )}
                      {task.status === 'waiting' && (
                        <>
                          <button type="button" className="cheio" onClick={() => void act('done', { id: task.id })}>
                            {t.boardFinish}
                          </button>
                          <button type="button" className="acao-texto" onClick={() => void act('start', { id: task.id })}>
                            {t.boardAgain}
                          </button>
                        </>
                      )}
                      {task.status === 'done' && (
                        <button type="button" className="acao-texto" onClick={() => void act('reopen', { id: task.id })}>
                          {t.boardReopen}
                        </button>
                      )}
                      {(task.status === 'todo' || task.status === 'done') && (
                        <button
                          type="button"
                          className={`acao-texto ${deleting === task.id ? 'confirmar' : ''}`}
                          onBlur={() => setDeleting(null)}
                          onClick={() => {
                            if (deleting !== task.id) setDeleting(task.id)
                            else {
                              setDeleting(null)
                              void act('delete', { id: task.id })
                            }
                          }}
                        >
                          {deleting === task.id ? t.boardDeleteSure : t.boardDelete}
                        </button>
                      )}
                    </span>
                  </article>
                )
              })}
            </div>
          )
        })}
      </div>
      {error && <p className="nota failed quebra">{error}</p>}
    </section>
  )
}
