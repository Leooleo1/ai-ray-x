import type { CSSProperties } from 'react'
import type { Agent } from '../agents'

/** Draws an open-bot-avatar definition flat: body in the agent color, two pill eyes.
 *  Idle: a slow breath and blinks on a rhythm of its own. Thinking: a small hop
 *  and eyes that look around. Both stop under prefers-reduced-motion. */
const EYE_WIDTH = 4
const EYE_HEIGHT = 11
const BLINK_OFFSETS: Record<string, string> = { raio: '0s', dieta: '-1.9s', memo: '-3.4s' }

export type AvatarState = 'idle' | 'thinking' | 'still'

interface AvatarProps {
  agent: Agent
  size: number
  state?: AvatarState
}

export function Avatar({ agent, size, state = 'idle' }: AvatarProps) {
  const { body, eyes } = agent.avatar
  const width = EYE_WIDTH * eyes.scale
  const height = EYE_HEIGHT * eyes.scale
  const style = { '--piscar-atraso': BLINK_OFFSETS[agent.id] ?? '0s' } as CSSProperties
  return (
    <svg className={`avatar ${state}`} style={style} width={size} height={size} viewBox="0 0 100 100" role="img" aria-label={agent.name}>
      <g className="avatar-tudo">
        <g className="avatar-corpo">
          <path d={body.path} fill={agent.color} fillRule="nonzero" />
        </g>
        <g className="avatar-olhos">
          {[-1, 1].map((side) => (
            <rect
              key={side}
              x={eyes.x + (side * eyes.spacing) / 2 - width / 2}
              y={eyes.y - height / 2}
              width={width}
              height={height}
              rx={width / 2}
              fill="#16161c"
            />
          ))}
        </g>
      </g>
    </svg>
  )
}
