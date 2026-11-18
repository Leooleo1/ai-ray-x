import type { Bot } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import { Avatar } from '../components/Avatar'
import { Frame } from './parts'
import { o } from './texts'

interface StepDoneProps {
  bots: Bot[]
  onFinish: (firstBot: string | null) => void
}

export function StepDone({ bots, onFinish }: StepDoneProps) {
  return (
    <Frame
      actions={
        <button className="bv-btn cheio bv-principal" onClick={() => onFinish(bots[0]?.id ?? null)}>
          {o.openPanel}
        </button>
      }
    >
      <ul className="bv-lista">
        {bots.map((bot) => (
          <li key={bot.id} className="bv-repo">
            <Avatar agent={faceForBot(bot)} size={28} state="idle" />
            <b>{bot.name}</b>
            <span className="bv-mono">{`${bot.persona?.name ?? ''} · ${AGENT_LABEL[bot.agent]}`}</span>
            <button className="bv-btn vidro bv-direita" onClick={() => onFinish(bot.id)}>
              {o.talk}
            </button>
          </li>
        ))}
      </ul>
    </Frame>
  )
}
