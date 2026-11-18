// Ready-made bots. The instructions go to the CLI in English; the bot still
// answers in the person's language.

import type { BotAgent, Persona } from '../api'
import { locale } from '../i18n'

export interface Preset {
  id: string
  shape: string
  color: string
  name: string
  role: string
  instructions: string
}

const pt = locale === 'pt-BR'

export const PRESETS: Preset[] = [
  {
    id: 'partner',
    shape: 'monitor',
    color: '#e8754a',
    name: pt ? 'Parceiro' : 'Partner',
    role: pt ? 'conversa sobre o projeto e propõe mudanças' : 'talks about the project and proposes changes',
    instructions: 'Be a pragmatic partner: answer what was asked, point to the files involved, and propose small, safe changes.',
  },
  {
    id: 'reviewer',
    shape: 'magnifier',
    color: '#6f95f2',
    name: pt ? 'Revisor' : 'Reviewer',
    role: pt ? 'acha bugs e riscos antes de publicar' : 'finds bugs and risks before you ship',
    instructions: 'Act as a careful code reviewer: look first for bugs, edge cases and risky changes; cite file and line; keep proposed fixes small.',
  },
  {
    id: 'explainer',
    shape: 'book',
    color: '#d9c29a',
    name: pt ? 'Explicador' : 'Explainer',
    role: pt ? 'explica o projeto em linguagem simples' : 'explains the project in plain words',
    instructions: 'Explain for someone who does not code: what each part does and why, in plain words, defining any technical term you must use.',
  },
  {
    id: 'guardian',
    shape: 'shield',
    color: '#5fd08a',
    name: pt ? 'Guardião' : 'Guardian',
    role: pt ? 'cuida de segredos, acessos e dados' : 'watches secrets, access and data',
    instructions:
      'Focus on security: hardcoded secrets, authentication and permissions, input validation, data exposure. Never print a secret value; refer to it by file and line only.',
  },
]

/** What the person is building in the Bot step. */
export interface BotDesign {
  preset: string
  agent: BotAgent
  name: string
  shape: string
  color: string
  instructions: string
}

export const CUSTOM = 'custom'

export function designFrom(preset: Preset, agent: BotAgent): BotDesign {
  return { preset: preset.id, agent, name: preset.name, shape: preset.shape, color: preset.color, instructions: preset.instructions }
}

export function personaOf(design: BotDesign, fallbackName: string): Persona {
  return {
    preset: design.preset,
    name: design.name.trim() || fallbackName,
    instructions: design.instructions.trim(),
    color: design.color,
    shape: design.shape,
  }
}
