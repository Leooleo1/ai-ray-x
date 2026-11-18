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
