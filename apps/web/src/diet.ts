// Dieta's suggestions in the reader's language. The backend sends English text;
// Portuguese is rebuilt here from the same numbers.

import type { SourceReport, Suggestion } from './api'
import { compact, locale } from './i18n'

interface Text {
  title: string
  how: string
}

const PT: Record<string, Text> = {
  'compact-earlier': {
    title: 'Compactar mais cedo',
    how: 'Diminuir a janela de compactação automática ou rodar /compact ao fim de cada tarefa',
  },
  'trim-tool-output': {
    title: 'Encurtar a saída das ferramentas',
    how: 'Limitar a saída do terminal, cortar com head/tail e buscar com Grep em vez de imprimir arquivos inteiros',
  },
  'lighter-startup': {
    title: 'Carregar menos na partida',
    how: 'Desligar plugins, skills e MCPs sem uso nos últimos 60 dias; manter regras e índice de memória curtos',
  },
  'screenshots-in-subagents': {
    title: 'Prints num subagente',
    how: 'Um subagente abre e julga os prints e devolve só o veredito; as imagens não entram na sessão principal',
  },
  'fresh-after-breaks': {
    title: 'Sessão nova depois de pausas longas',
    how: 'Depois de uma hora fora, abrir sessão nova com um resumo curto (ou um handoff) em vez de retomar uma enorme',
  },
}

/** What applying does, shown in the Apply tooltip. Only for suggestions with a setting. */
const EXPLAIN: Record<string, { pt: string[]; en: string[] }> = {
  'compact-earlier': {
    pt: [
      'Grava CLAUDE_CODE_AUTO_COMPACT_WINDOW=400000 no .claude/settings.local.json deste projeto (só seu, fora do git).',
      'Nas próximas sessões daqui, o Claude Code resume a conversa ao chegar em 400 mil tokens, em vez de perto de 1 milhão: cada pedido relê bem menos.',
      'O preço: ele resume com mais frequência, e detalhes antigos viram resumo mais cedo.',
      'Faz backup antes; "desfazer" volta o valor anterior. Outros projetos não mudam.',
    ],
    en: [
      'Writes CLAUDE_CODE_AUTO_COMPACT_WINDOW=400000 to this project\'s .claude/settings.local.json (yours only, kept out of git).',
      'In the next sessions here, Claude Code summarizes the conversation at 400K tokens instead of near 1M: every request re-reads much less.',
      'The cost: it summarizes more often, so older details become summary sooner.',
      'Backed up first; "undo" restores the previous value. Other projects do not change.',
    ],
  },
}

export function explainApply(id: string): string[] {
  const text = EXPLAIN[id]
  if (!text) return []
  return locale === 'pt-BR' ? text.pt : text.en
}

export function suggestionTitle(id: string, fallback: string): string {
  return locale === 'pt-BR' ? (PT[id]?.title ?? fallback) : fallback
}

export interface DietLine {
  id: string
  title: string
  evidence: string
  how: string
  saving: number
  share: number
  isUpperBound: boolean
}

function evidencePt(s: Suggestion, source: SourceReport): string {
  const e = source.efficiency
  const shell = source.overview.source === 'claude' ? 'Bash' : 'exec'
  switch (s.id) {
    case 'compact-earlier':
      return `compacta perto de ${compact(e.compaction_pre_median)} · metade dos pedidos passa de ${compact(e.context.median)}`
    case 'trim-tool-output': {
      const carried = e.carried.find((c) => c.name === shell)?.carried_tokens ?? 0
      return `a saída do ${shell} fica carregada: ${compact(carried)} relidos`
    }
    case 'lighter-startup': {
      const fixed = e.fixed.find((f) => f.kind === 'main')?.median_first_request ?? 0
      const listed = Number(e.snapshots.skill_listing_items ?? 0)
      const skills = listed > 0 ? ` · ${listed} skills listadas, ${e.skills_used.length} usadas` : ''
      return `${compact(fixed)} carregados antes de começar${skills}`
    }
    case 'screenshots-in-subagents':
      return `${compact(e.carried.reduce((sum, c) => sum + c.images, 0))} imagens entraram no contexto principal`
    default:
      return s.evidence
  }
}

export function dietLines(source: SourceReport): DietLine[] {
  const pt = locale === 'pt-BR'
  return source.suggestions.map((s) => {
    const text = pt ? PT[s.id] : undefined
    return {
      id: s.id,
      title: text?.title ?? s.title,
      evidence: pt ? evidencePt(s, source) : s.evidence,
      how: text?.how ?? s.how,
      saving: s.saving_tokens,
      share: s.saving_share,
      isUpperBound: s.confidence === 'upper bound',
    }
  })
}
