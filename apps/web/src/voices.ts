// How the house agents talk in a project's chat. The answers are data from the
// server, computed without AI; the voice is here. Raio reads like a report,
// Dieta like a nutritionist, Memo like an archivist.

import type { AgentId } from './agents'
import type { Outcome } from './api'
import { suggestionTitle } from './diet'
import { describeOutcome } from './outcome'
import { compact, locale, percent, t } from './i18n'

const pt = locale === 'pt-BR'
const HEAVY = 400_000

export type Data = Record<string, unknown>

const num = (d: Data, key: string): number => Number(d[key] ?? 0)

const ASK: Record<AgentId, [string, string]> = {
  raio: ['quanto pesa?', 'how heavy?'],
  dieta: ['o que cortar?', 'what to cut?'],
  memo: ['o que guardamos?', 'what did we keep?'],
}

/** The short question each house agent answers, as its chip says it. */
export function houseAsk(agent: AgentId): string {
  const [ptText, enText] = ASK[agent]
  return pt ? ptText : enText
}

function raio(d: Data): string {
  if (!d.found) return pt ? 'Ainda não achei nenhuma sessão do Claude Code neste projeto.' : 'No Claude Code session in this project yet.'
  const ctx = num(d, 'context_tokens')
  const top = (d.top as { name: string; tokens: number }[] | undefined) ?? []
  const named: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }
  const heavy = top.map((item) => `${named[item.name] ?? item.name} (${compact(item.tokens)})`).join(', ')
  const verdict =
    ctx >= HEAVY
      ? pt ? 'Passou de 400 mil: feche o passo atual e rode /compact.' : 'Over 400K: finish the current step and run /compact.'
      : pt ? 'Está leve, pode seguir.' : 'Light enough, carry on.'
  return pt
    ? `Laudo da última sessão neste projeto: ${compact(ctx)} de contexto, ${compact(num(d, 'startup_tokens'))} vieram da partida, ${num(d, 'requests')} pedidos desde a última compactação. Mais pesado: ${heavy}. ${verdict}`
    : `Report on this project's latest session: ${compact(ctx)} of context, ${compact(num(d, 'startup_tokens'))} from startup, ${num(d, 'requests')} requests since the last compaction. Heaviest: ${heavy}. ${verdict}`
}

function dieta(d: Data): string {
  const list = (d.suggestions as { id: string; title: string; share: number; confidence: string }[] | undefined) ?? []
  const first = list[0]
  if (!first) return pt ? 'Nada para cortar agora. Prato limpo.' : 'Nothing to cut right now. Clean plate.'
  // Old messages said "up to" (a ceiling); the estimate is now net, so every saving reads as ~.
  const bound = '~'
  const applicable = ((d.applicable as string[] | undefined) ?? []).includes(first.id)
  const how = applicable
    ? pt ? 'Esse dá para aplicar com um clique.' : 'This one can be applied with a click.'
    : pt ? 'Esse é hábito: não tem ajuste para aplicar.' : 'This one is a habit: there is no setting to apply.'
  return pt
    ? `Prato do dia: ${list.length} cortes. O maior é ${suggestionTitle(first.id, first.title).toLowerCase()}, ${bound}${percent(first.share)} menos contexto relido. ${how}`
    : `Today's plate: ${list.length} cuts. The biggest is ${first.title.toLowerCase()}, ${bound}${percent(first.share)} less context re-read. ${how}`
}

const MEMORY_STATE: Record<string, [string, string]> = {
  known: ['o ai-memory já conhece este projeto.', 'ai-memory already knows this project.'],
  new_project: ['o ai-memory ainda não conhece este projeto; a primeira lição cria o registro.', 'ai-memory does not know this project yet; the first lesson creates it.'],
  missing: ['não encontrei o ai-memory nesta máquina.', 'ai-memory is not installed here.'],
  unreachable: ['o ai-memory não respondeu agora.', 'ai-memory did not answer right now.'],
}

function memo(d: Data): string {
  const lessons = (d.lessons as { suggestion: string; undone: boolean; outcome?: Outcome; kept_now?: boolean }[] | undefined) ?? []
  const [statePt, stateEn] = MEMORY_STATE[String(d.ai_memory)] ?? ['', '']
  if (lessons.length === 0) {
    return pt ? `Arquivo vazio por enquanto. Quando você aplicar uma sugestão, eu guardo a lição; ${statePt}` : `Nothing archived yet. When you apply a suggestion, I keep the lesson; ${stateEn}`
  }
  const last = lessons[lessons.length - 1]
  const lastTitle = suggestionTitle(last.suggestion, last.suggestion).toLowerCase()
  const result = last.outcome ? ` ${pt ? 'Resultado' : 'Result'}: ${describeOutcome(last.outcome)}.` : ''
  const kept = last.kept_now ? (pt ? ' Guardei esse resultado no ai-memory agora.' : ' I just kept this result in ai-memory.') : ''
  return pt
    ? `Arquivei ${lessons.length} ${lessons.length === 1 ? 'lição' : 'lições'} neste projeto; a última foi ${lastTitle}${last.undone ? ', depois desfeita' : ''}.${result}${kept} Quanto ao resto, ${statePt}`
    : `I archived ${lessons.length} lesson${lessons.length === 1 ? '' : 's'} here; the last was ${lastTitle}${last.undone ? ', later undone' : ''}.${result}${kept} Also, ${stateEn}`
}

/** What a house agent says, from the data it computed about the project. */
export function houseText(agent: AgentId, data: Data): string {
  return agent === 'raio' ? raio(data) : agent === 'dieta' ? dieta(data) : memo(data)
}
