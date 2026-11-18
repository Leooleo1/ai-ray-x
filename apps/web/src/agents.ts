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
  color: string
  avatar: AvatarDefinition
}

const pt = locale === 'pt-BR'

export const AGENTS: Record<AgentId, Agent> = {
  raio: {
    id: 'raio',
    name: 'Raio',
    role: pt ? 'lê os registros e mostra o que pesa' : 'reads the logs and shows what weighs',
    color: '#b4a3f5',
    avatar: raio as AvatarDefinition,
  },
  dieta: {
    id: 'dieta',
    name: 'Dieta',
    role: pt ? 'sugere cortes, nunca aplica sozinha' : 'suggests cuts, never applies them',
    color: '#5fd08a',
    avatar: dieta as AvatarDefinition,
  },
  memo: {
    id: 'memo',
    name: 'Memo',
    role: pt ? 'guarda as lições no ai-memory' : 'keeps the lessons in ai-memory',
    color: '#d9c29a',
    avatar: memo as AvatarDefinition,
  },
}
