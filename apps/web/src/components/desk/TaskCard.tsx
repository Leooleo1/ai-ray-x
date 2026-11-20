import { useState } from 'react'
import type { Bot, Task } from '../../api'
import { compact, t } from '../../i18n'
import type { ActOnBoard } from './useBoard'
import { Who } from './Who'

interface TaskCardProps {
  task: Task
  owner: Bot | undefined
  /** Another bot that left it (a bot's own follow-up says nothing more). */
  from: Bot | undefined
  act: ActOnBoard
  onOpen: () => void
}

const nameOf = (bot: Bot) => bot.persona?.name || bot.name

/** A task on the board, with the one step that moves it on and the quieter ones. */
export function TaskCard({ task, owner, from, act, onOpen }: TaskCardProps) {
  const [confirming, setConfirming] = useState(false)
  const { status } = task
  const remove = () => {
    if (!confirming) return setConfirming(true)
    setConfirming(false)
    void act('delete', { id: task.id })
  }
  return (
    <article className={`mesa-cartao ${status}`}>
      {status === 'waiting' && (
        <span className="mesa-tipo">
          <span>{t.deskTaskToFinish}</span>
        </span>
      )}
      <p className="mesa-titulo">{task.title}</p>
      <Who
        bot={owner}
        note={from ? t.boardFromBot.replace('{who}', nameOf(from)) : undefined}
        figure={task.tokens > 0 ? `${compact(task.tokens)} tokens` : undefined}
        working={status === 'doing'}
      />
      <footer className="mesa-acoes">
        {status === 'todo' && (
          <button type="button" className="mesa-cheio" onClick={() => void act('start', { id: task.id })}>
            {t.boardStart}
          </button>
        )}
        {status === 'waiting' && (
          <button type="button" className="mesa-cheio" onClick={() => void act('done', { id: task.id })}>
            {t.boardFinish}
          </button>
        )}
        {status === 'doing' && <small className="mesa-trabalhando">{t.boardWorking}</small>}
        {status !== 'todo' && (
          <button type="button" className="acao-texto" onClick={onOpen}>
            {t.boardOpen}
          </button>
        )}
        {status === 'waiting' && (
          <button type="button" className="acao-texto" onClick={() => void act('start', { id: task.id })}>
            {t.boardAgain}
          </button>
        )}
        {status === 'done' && (
          <button type="button" className="acao-texto" onClick={() => void act('reopen', { id: task.id })}>
            {t.boardReopen}
          </button>
        )}
        {(status === 'todo' || status === 'done') && (
          <button type="button" className={`acao-texto ${confirming ? 'confirmar' : ''}`} onBlur={() => setConfirming(false)} onClick={remove}>
            {confirming ? t.boardDeleteSure : t.boardDelete}
          </button>
        )}
      </footer>
    </article>
  )
}
