import { useEffect, useRef } from 'react'
import type { AvatarDefinition } from 'open-bot-avatar'
import { mountAvatar, type MountedAvatar } from 'open-bot-avatar/dom'
import type { Face } from '../avatars'

/** Live avatar from the open-bot-avatar engine (MIT, Dominik Scholz): volume,
 *  gaze, blinks and moods. `still` draws one static frame, for history rows.
 *  The engine honors prefers-reduced-motion on its own. */
export type AvatarState = 'idle' | 'thinking' | 'happy' | 'still'

const EYE_COLOR = '#16161c'
/** Volume per finish: solid is the engine's natural depth, flat has none. */
const DEPTH: Record<string, number> = { solid: 5, flat: 0, pixel: 3 }

interface AvatarProps {
  agent: Face
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
      depth: DEPTH[agent.finish ?? 'solid'] ?? 5,
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

  return <div ref={host} className={`avatar ${agent.finish === 'pixel' ? 'pixel' : ''}`} role="img" aria-label={agent.name} style={{ width: size, height: size }} />
}
