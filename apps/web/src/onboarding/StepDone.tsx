import type { Bot, BotAgent } from '../api'
import { AGENT_LABEL, type Face, faceForBot } from '../avatars'
import { Avatar } from '../components/Avatar'
import { Tour } from '../tour/Tour'
import { Frame } from './parts'
import { o } from './texts'

/** Faces shown together in the footer. */
const MAX_FACES = 3

interface StepDoneProps {
  bots: Bot[]
  face: Face
  agent: BotAgent
  onFinish: (firstBot: string | null) => void
}

/** The end: the mini tutorial with the person's bot, and the way into the panel. */
export function StepDone({ bots, face, agent, onFinish }: StepDoneProps) {
  return (
    <Frame
      actions={
        <>
          {bots.length > 0 && (
            <span className="bv-feitos">
              <span className="bv-rostos">
                {bots.slice(0, MAX_FACES).map((bot) => (
                  <Avatar key={bot.id} agent={faceForBot(bot)} size={22} state="still" />
                ))}
              </span>
              <span className="bv-mono">{bots.map((bot) => bot.name).join(', ')}</span>
            </span>
          )}
          <button className="bv-btn cheio bv-principal" onClick={() => onFinish(bots[0]?.id ?? null)}>
            {o.openPanel}
          </button>
        </>
      }
    >
      <Tour face={face} agent={AGENT_LABEL[agent]} />
    </Frame>
  )
}
