import { useEffect, useRef } from 'react'
import type { AvatarDefinition } from 'open-bot-avatar'
import { mountAvatar, type MountedAvatar } from 'open-bot-avatar/dom'
import type { Agent } from '../agents'

/** Live avatar from the open-bot-avatar engine (MIT, Dominik Scholz): volume,
 *  gaze, blinks and moods. `still` draws one static frame, for history rows.
 *  The engine honors prefers-reduced-motion on its own. */
export type AvatarState = 'idle' | 'thinking' | 'happy' | 'still'

const EYE_COLOR = '#16161c'
const DEPTH = 5

interface AvatarProps {
  agent: Agent
  size: number
  state?: AvatarState
  followPointer?: boolean
}

export function Avatar({ agent, size, state = 'idle', followPointer = false }: AvatarProps) {
  const host = useRef<HTMLDivElement>(null)
  const mounted = useRef<MountedAvatar | null>(null)
  const still = state === 'still'

  useEffect(() => {
    if (!host.current) return undefined
    mounted.current = mountAvatar(host.current, {
      definition: agent.avatar as AvatarDefinition,
      color: agent.color,
      eyeColor: EYE_COLOR,
      depth: DEPTH,
      still,
      followPointer,
      reducedMotion: 'system',
    })
    return () => {
      mounted.current?.destroy()
      mounted.current = null
    }
  }, [agent, still, followPointer])

  useEffect(() => {
    if (!still) mounted.current?.update({ mood: state })
  }, [state, still])

  return <div ref={host} className="avatar" role="img" aria-label={agent.name} style={{ width: size, height: size }} />
}
