// Avatar shapes and colors a bot can wear, and the faces built from them.
// Shapes come from open-bot-avatar (MIT, Dominik Scholz): the package's
// starter examples plus a few from its gallery, copied to assets/avatars.

import { type AvatarDefinition, getExample } from 'open-bot-avatar'
import monitor from '../../../assets/agents/bot.json'
import book from '../../../assets/avatars/book.json'
import brain from '../../../assets/avatars/brain.json'
import cat from '../../../assets/avatars/cat.json'
import ghost from '../../../assets/avatars/ghost.json'
import magnifier from '../../../assets/avatars/magnifier.json'
import rocket from '../../../assets/avatars/rocket.json'
import shield from '../../../assets/avatars/shield.json'
import star from '../../../assets/avatars/star.json'
import type { Bot, BotAgent, Finish } from './api'

export interface Face {
  name: string
  color: string
  avatar: AvatarDefinition
  /** Our own agents keep the default solid finish. */
  finish?: Finish
}

export const FINISHES: Finish[] = ['solid', 'flat', 'pixel']

/** Ready-made looks: shape, color and finish in one click. */
export const TEMPLATES: { shape: string; color: string; finish: Finish }[] = [
  { shape: 'monitor', color: '#e8754a', finish: 'solid' },
  { shape: 'monitor', color: '#6f95f2', finish: 'pixel' },
  { shape: 'chat', color: '#b4a3f5', finish: 'flat' },
  { shape: 'magnifier', color: '#6f95f2', finish: 'pixel' },
  { shape: 'shield', color: '#5fd08a', finish: 'solid' },
  { shape: 'rocket', color: '#ec7fb0', finish: 'pixel' },
  { shape: 'ghost', color: '#d9c29a', finish: 'flat' },
  { shape: 'cat', color: '#e8754a', finish: 'pixel' },
]

const fromPackage = (id: string): AvatarDefinition[] => {
  const found = getExample(id)
  return found ? [found] : []
}

const SHAPE_LIST: AvatarDefinition[] = [
  monitor as AvatarDefinition,
  ...fromPackage('chat'),
  magnifier as AvatarDefinition,
  book as AvatarDefinition,
  shield as AvatarDefinition,
  ...fromPackage('lightbulb'),
  brain as AvatarDefinition,
  rocket as AvatarDefinition,
  star as AvatarDefinition,
  ...fromPackage('sprout'),
  cat as AvatarDefinition,
  ghost as AvatarDefinition,
]

export const SHAPES: Record<string, AvatarDefinition> = Object.fromEntries(SHAPE_LIST.map((shape) => [shape.id, shape]))
export const SHAPE_IDS: string[] = SHAPE_LIST.map((shape) => shape.id)

/** Bot colors; each has a stage light engraved in public/luz/palco-<hex>.webp. */
export const COLORS: string[] = ['#e8754a', '#6f95f2', '#5fd08a', '#b4a3f5', '#d9c29a', '#ec7fb0']

export const DEFAULT_COLOR: Record<BotAgent, string> = { claude: '#e8754a', codex: '#6f95f2', cursor: '#d9c29a' }

export function stageLight(color: string): string {
  const known = COLORS.includes(color) ? color : COLORS[0]
  return `/luz/palco-${known.slice(1)}.webp`
}

// The same face object for the same look: Avatar remounts when its face changes identity.
const cache = new Map<string, Face>()

export function faceOf(shape: string, color: string, name: string, finish: Finish = 'solid'): Face {
  const key = `${shape}|${color}|${name}|${finish}`
  const known = cache.get(key)
  if (known) return known
  const face = { name, color, finish, avatar: SHAPES[shape] ?? (monitor as AvatarDefinition) }
  cache.set(key, face)
  return face
}

export const AGENT_LABEL: Record<BotAgent, string> = { claude: 'Claude Code', codex: 'Codex', cursor: 'Cursor' }

/** The name the person gave a bot, when it is not just its CLI's name. */
export function personaName(bot: { agent: BotAgent; persona: { name: string } | null }): string | undefined {
  const name = bot.persona?.name.trim()
  return name && name !== AGENT_LABEL[bot.agent] ? name : undefined
}

/** The bot's own look if the person gave it one, else the monitor in its CLI's color. */
export function faceForBot(bot: Pick<Bot, 'agent' | 'persona'>): Face {
  const persona = bot.persona
  return persona ? faceOf(persona.shape, persona.color, persona.name, persona.finish ?? 'solid') : faceOf('monitor', DEFAULT_COLOR[bot.agent], AGENT_LABEL[bot.agent])
}

/** Who speaks in a bot's chat: its name, or the CLI's. */
export function speakerOf(bot: Pick<Bot, 'agent' | 'persona'>): string {
  return bot.persona?.name ?? AGENT_LABEL[bot.agent]
}
