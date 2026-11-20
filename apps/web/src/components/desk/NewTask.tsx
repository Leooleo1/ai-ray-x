import { type FormEvent, useState } from 'react'
import type { Bot, TaskBoardState } from '../../api'
import { faceForBot } from '../../avatars'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { SelectChip } from '../SelectChip'
import type { ActOnBoard } from './useBoard'

const TITLE_MAX = 120
const two = (n: number) => String(n).padStart(2, '0')
const nameOf = (bot: Bot) => bot.persona?.name || bot.name

interface NewTaskProps {
  bots: Bot[]
  board: TaskBoardState | null
  act: ActOnBoard
}

/** The desk's top line: a task for one of the AIs, and whether they pick tasks up by themselves. */
export function NewTask({ bots, board, act }: NewTaskProps) {
  const [title, setTitle] = useState('')
  const [bot, setBot] = useState('')
  const [isolated, setIsolated] = useState(false)
  const chosen = bot || bots[0]?.id || ''
  const add = async (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() && chosen && (await act('add', { bot: chosen, title, detail: '', isolated }))) setTitle('')
  }
  const note = board?.window ? t.boardAutoWindow.replace('{a}', two(board.window.start)).replace('{b}', two(board.window.end)) : t.boardAutoNote
  return (
    <div className="mesa-topo">
      {bots.length > 0 && (
        <form className="mesa-nova" onSubmit={(event) => void add(event)}>
          <input value={title} maxLength={TITLE_MAX} placeholder={t.boardNewTitle} aria-label={t.boardNewTitle} onChange={(event) => setTitle(event.target.value)} />
          <SelectChip
            className="mesa-nova-bot"
            label={t.boardBotLabel}
            value={chosen}
            options={bots.map((b) => ({
              value: b.id,
              label: nameOf(b),
              meta: b.name !== nameOf(b) ? b.name : undefined,
              icon: <Avatar agent={faceForBot(b)} size={18} state="still" />,
            }))}
            onChange={setBot}
          />
          <button type="button" className={`mesa-copia ${isolated ? 'ligada' : ''}`} aria-pressed={isolated} title={t.boardCopyHint} onClick={() => setIsolated(!isolated)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="8" y="8" width="12" height="12" rx="2.5" />
              <path d="M16 8V6.5A2.5 2.5 0 0 0 13.5 4h-7A2.5 2.5 0 0 0 4 6.5v7A2.5 2.5 0 0 0 6.5 16H8" />
            </svg>
            {t.boardCopy}
          </button>
          <button type="submit" className="mesa-cheio" disabled={!title.trim()}>
            {t.boardAdd}
          </button>
        </form>
      )}
      {board && (
        <label className="permissao mesa-auto">
          <button type="button" className={`config-chave ${board.auto ? 'ligada' : ''}`} role="switch" aria-checked={board.auto} onClick={() => void act('auto', { on: !board.auto })}>
            <i />
          </button>
          <span>
            <b>{t.boardAuto}</b>
            <small>{note}</small>
          </span>
        </label>
      )}
    </div>
  )
}
