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
/** What applying does, shown in the Apply tooltip, with the point Dieta computed. */
export function explainApply(id: string, value?: string | null): string[] {
  if (id !== 'compact-earlier') return []
  const cap = compact(Number(value ?? 400_000))
  return locale === 'pt-BR'
    ? [
        `Grava CLAUDE_CODE_AUTO_COMPACT_WINDOW=${value ?? '400000'} no .claude/settings.local.json deste projeto (só seu, fora do git).`,
        `Nas próximas sessões daqui, o Claude Code resume a conversa ao chegar em ${cap} tokens, em vez de perto de 1 milhão.`,
        'O preço já está na conta: cada resumo lê o contexto inteiro uma vez, e depois dele o Claude relê o que saiu do resumo.',
        'No máximo um resumo a cada 100 pedidos, para não perder detalhe toda hora. Faz backup antes; "desfazer" volta o valor anterior.',
      ]
    : [
        `Writes CLAUDE_CODE_AUTO_COMPACT_WINDOW=${value ?? '400000'} to this project's .claude/settings.local.json (yours only, kept out of git).`,
        `In the next sessions here, Claude Code summarizes the conversation at ${cap} tokens instead of near 1M.`,
        'The cost is already counted: each summary reads the whole context once, and afterwards Claude re-reads what left the summary.',
        'At most one summary per 100 requests, so detail is not lost all the time. Backed up first; "undo" restores the previous value.',
      ]
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
  value: string | null
}

function evidencePt(s: Suggestion, source: SourceReport): string {
  const e = source.efficiency
  const shell = source.overview.source === 'claude' ? 'Bash' : 'exec'
  switch (s.id) {
    case 'compact-earlier': {
      const m = e.compaction
      const best = m.best
      if (!best) return s.evidence
      const per100 = (n: number) => n.toLocaleString(locale, { maximumFractionDigits: 1 })
      return `compactando em ${compact(best.cap)}, cada pedido relê ${compact(best.avg_context)} em média, em vez de ${compact(m.now_avg_context)}; ${per100(best.compactions_per_100)} resumo a cada 100 pedidos (hoje ${per100(m.now_compactions_per_100)}), cada um lendo o contexto inteiro uma vez; depois de cada resumo recomeça em ${compact(m.floor)} e relê uns ${compact(m.refill)} a mais`
    }
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
      value: s.value ?? null,
    }
  })
}
