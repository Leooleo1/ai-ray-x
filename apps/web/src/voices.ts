// How each agent talks. The answers are data from the server; the voice is here.
// Raio reads like a report, Dieta like a nutritionist, Memo like an archivist.

import type { RoomMessage } from './api'
import type { Outcome } from './api'
import { suggestionTitle } from './diet'
import { describeOutcome } from './outcome'
import { compact, locale, percent } from './i18n'

const pt = locale === 'pt-BR'
const HEAVY = 400_000

type Data = Record<string, unknown>

const num = (d: Data, key: string): number => Number(d[key] ?? 0)

const ASK: Record<string, [string, string]> = {
  raio: ['Raio, como está a sessão?', 'Raio, how is the session?'],
  dieta: ['Dieta, o que dá para cortar?', 'Dieta, what can we cut?'],
  memo: ['Memo, o que guardamos?', 'Memo, what did we keep?'],
}

export function askLabel(agent: string): string {
  const [ptText, enText] = ASK[agent] ?? ['', '']
  return pt ? ptText : enText
}

function raio(d: Data): string {
  if (!d.found) return pt ? 'Ainda não achei nenhuma sessão deste projeto nos registros.' : 'No session of this project in the logs yet.'
  const ctx = num(d, 'context_tokens')
  const top = (d.top as { name: string; tokens: number }[] | undefined) ?? []
  const heavy = top.map((item) => `${item.name} (${compact(item.tokens)})`).join(', ')
  const verdict =
    ctx >= HEAVY
      ? pt ? 'Passou de 400 mil: feche o passo atual e rode /compact.' : 'Over 400K: finish the current step and run /compact.'
      : pt ? 'Está leve, pode seguir.' : 'Light enough, carry on.'
  return pt
    ? `Laudo da sessão: ${compact(ctx)} de contexto, ${compact(num(d, 'startup_tokens'))} vieram da partida, ${num(d, 'requests')} pedidos desde a última compactação. Mais pesado: ${heavy}. ${verdict}`
    : `Session report: ${compact(ctx)} of context, ${compact(num(d, 'startup_tokens'))} from startup, ${num(d, 'requests')} requests since the last compaction. Heaviest: ${heavy}. ${verdict}`
}

function dieta(d: Data): string {
  const list = (d.suggestions as { id: string; title: string; share: number; confidence: string }[] | undefined) ?? []
  const first = list[0]
  if (!first) return pt ? 'Nada para cortar agora. Prato limpo.' : 'Nothing to cut right now. Clean plate.'
  const bound = first.confidence === 'upper bound' ? (pt ? 'até ' : 'up to ') : '~'
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

export function speak(message: RoomMessage): string {
  const d = message.data
  switch (message.kind) {
    case 'ask':
      return typeof d.text === 'string' && d.text ? d.text : askLabel(String(d.agent))
    case 'answer':
      return message.author === 'raio' ? raio(d) : message.author === 'dieta' ? dieta(d) : memo(d)
    case 'applied':
      return pt ? `Apliquei ${d.key}=${d.value} neste projeto. Para voltar, é só desfazer.` : `Applied ${d.key}=${d.value} in this project. Undo brings it back.`
    case 'undone':
      return pt ? `Desfiz ${d.key}; voltou ao valor de antes.` : `Undid ${d.key}; the previous value is back.`
    case 'kept':
      return pt ? `Arquivei a lição no ai-memory (${d.kept}).` : `Kept the lesson in ai-memory (${d.kept}).`
    case 'kept_failed':
      return pt ? `Não consegui arquivar no ai-memory: ${d.error}.` : `Could not keep it in ai-memory: ${d.error}.`
  }
}
