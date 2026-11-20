import type { Bot } from '../../api'
import { faceForBot } from '../../avatars'
import { Avatar } from '../Avatar'

interface WhoProps {
  bot: Bot | undefined
  /** Said after the name, when there is more to say (who left the task, which conversation). */
  note?: string
  /** A number at the right edge, in mono: what it cost. */
  figure?: string
  working?: boolean
}

/** Whose card this is: the AI's face and name, and what it cost at the right. */
export function Who({ bot, note, figure, working = false }: WhoProps) {
  const name = bot ? bot.persona?.name || bot.name : '?'
  return (
    <span className="mesa-quem">
      {bot && <Avatar agent={faceForBot(bot)} size={18} state={working ? 'thinking' : 'still'} />}
      <small>{note ? `${name} · ${note}` : name}</small>
      {figure && <span className="mesa-cifra">{figure}</span>}
    </span>
  )
}
