import type { BotAgent } from '../api'
import { AGENT_LABEL } from '../avatars'
import './agent-logo.css'

const KNOWN = new Set<string>(['claude', 'codex', 'cursor', 'antigravity', 'opencode'])

interface AgentLogoProps {
  agent: string
  size?: number
  /** Said aloud when the logo stands alone; decorative beside the AI's name. */
  labelled?: boolean
}

/** The AI's own mark (Claude, OpenAI for Codex, Cursor, Antigravity, opencode). */
export function AgentLogo({ agent, size = 14, labelled = false }: AgentLogoProps) {
  if (!KNOWN.has(agent)) return null
  return (
    <img
      className="ia-logo"
      src={`/ia/${agent}.svg`}
      width={size}
      height={size}
      alt={labelled ? AGENT_LABEL[agent as BotAgent] : ''}
      aria-hidden={labelled ? undefined : true}
      draggable={false}
    />
  )
}
