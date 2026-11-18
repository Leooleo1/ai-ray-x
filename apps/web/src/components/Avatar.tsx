import type { Agent } from '../agents'

/** Draws an open-bot-avatar definition flat: body in the agent color, two pill eyes. */
const EYE_WIDTH = 4
const EYE_HEIGHT = 11

interface AvatarProps {
  agent: Agent
  size: number
}

export function Avatar({ agent, size }: AvatarProps) {
  const { body, eyes } = agent.avatar
  const width = EYE_WIDTH * eyes.scale
  const height = EYE_HEIGHT * eyes.scale
  return (
    <svg className="avatar" width={size} height={size} viewBox="0 0 100 100" role="img" aria-label={agent.name}>
      <path d={body.path} fill={agent.color} fillRule="nonzero" />
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
    </svg>
  )
}
