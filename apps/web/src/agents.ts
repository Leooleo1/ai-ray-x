// raiox's own agents: geometry from the open-bot-avatar engine, colors from the panel palette.

import dieta from '../../../assets/agents/dieta.json'
import memo from '../../../assets/agents/memo.json'
import raio from '../../../assets/agents/raio.json'
import type { AvatarDefinition } from 'open-bot-avatar'
import { locale } from './i18n'

export type AgentId = 'raio' | 'dieta' | 'memo'

export interface Agent {
  id: AgentId
  name: string
  role: string
  /** What it is about, in two or three words, under its name. */
  about: string
  color: string
  avatar: AvatarDefinition
  /** Its plush portrait, for the larger places (a tip's card). */
  art: string
}

const pt = locale === 'pt-BR'

export const AGENTS: Record<AgentId, Agent> = {
  raio: {
    id: 'raio',
    name: pt ? 'Raio' : 'Ray',
    role: pt ? 'lê os registros e mostra o que pesa' : 'reads the logs and shows what weighs',
    about: pt ? 'uso de tokens' : 'token use',
    color: '#b4a3f5',
    avatar: raio as AvatarDefinition,
    art: '/mascotes/ajudante-raio.webp',
  },
  dieta: {
    id: 'dieta',
    name: pt ? 'Dieta' : 'Diet',
    role: pt ? 'sugere cortes, nunca aplica sozinha' : 'suggests cuts, never applies them',
    about: pt ? 'cortes de custo' : 'cost cuts',
    color: '#5fd08a',
    avatar: dieta as AvatarDefinition,
    art: '/mascotes/ajudante-dieta.webp',
  },
  memo: {
    id: 'memo',
    name: 'Memo',
    role: pt ? 'guarda as lições no ai-memory' : 'keeps the lessons in ai-memory',
    about: pt ? 'memória' : 'memory',
    color: '#d9c29a',
    avatar: memo as AvatarDefinition,
    art: '/mascotes/ajudante-memo.webp',
  },
}
