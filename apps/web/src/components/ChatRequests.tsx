import type { ChatRequest } from '../api'
import { AGENT_LABEL } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { daysLabel } from './RoutinesEditor'

const two = (n: number) => String(n).padStart(2, '0')

/** One request in a sentence, with the bots and channels by the names the person knows. */
function describe(request: ChatRequest, bots: Bots): { line: string; note?: string } {
  const { input } = request
  const botName = (id = '') => {
    const bot = bots.state.bots.find((b) => b.id === id)
    return bot ? `${bot.persona?.name || AGENT_LABEL[bot.agent]} · ${bot.name}` : id
  }
  const fill = (template: string, values: Record<string, string>) => Object.entries(values).reduce((text, [key, value]) => text.replace(`{${key}}`, value), template)
  switch (request.kind) {
    case 'add_bot':
      return { line: fill(t.requestAddBot, { agent: AGENT_LABEL[input.agent as keyof typeof AGENT_LABEL] ?? input.agent ?? '', project: input.project ?? '' }) }
    case 'add_routine':
      return { line: fill(t.requestAddRoutine, { bot: botName(input.bot), days: daysLabel(input.days ?? []), time: `${two(input.hour ?? 0)}:${two(input.minute ?? 0)}`, text: input.text ?? '' }) }
    case 'add_task':
      return { line: fill(t.requestAddTask, { bot: botName(input.bot), title: input.title ?? '' }) }
    case 'new_channel':
      return { line: fill(t.requestNewChannel, { name: input.name ?? '', members: (input.members ?? []).map((m) => botName(m)).join(', ') }) }
    case 'channel_post': {
      const channel = bots.state.channels.find((c) => c.id === input.channel)
      return { line: fill(t.requestPost, { channel: channel?.name ?? input.channel ?? '', text: input.text ?? '' }), note: t.requestPostCost }
    }
  }
}

/** Changes an AI asked for from a chat, each waiting for Approve or Discard. */
export function ChatRequests({ bots }: { bots: Bots }) {
  const requests = bots.state.requests ?? []
  if (requests.length === 0) return null
  return (
    <section className="mesa-secao pedidos">
      <h2>
        {t.requestsTitle}
        <small>{requests.length}</small>
      </h2>
      <p className="pedidos-nota">{t.requestsNote}</p>
      <ul className="pedidos-lista">
        {requests.map((request) => {
          const { line, note } = describe(request, bots)
          return (
            <li key={request.id}>
              <span className="pedidos-texto">
                <b>{line}</b>
                {[request.why, note].filter(Boolean).map((extra) => (
                  <small key={extra}>{extra}</small>
                ))}
              </span>
              <span className="quadro-acoes">
                <button type="button" className="cheio" onClick={() => void bots.act('request_approve', { request: request.id })}>
                  {t.requestApprove}
                </button>
                <button type="button" className="link" onClick={() => void bots.act('request_discard', { request: request.id })}>
                  {t.requestDiscard}
                </button>
              </span>
            </li>
          )
        })}
      </ul>
      {bots.error && <p className="nota failed quebra">{bots.error}</p>}
    </section>
  )
}
