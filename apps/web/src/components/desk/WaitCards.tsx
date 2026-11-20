import type { ChatRequest } from '../../api'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import type { ChatNav } from '../chat/nav'
import type { Waiting } from '../chat/waiting'
import { describeRequest } from './requests'
import { Who } from './Who'

interface ApprovalCardProps {
  item: Waiting
  nav: ChatNav
}

/** A proposal, draft or command an AI waits on: it is approved in its conversation, so the card opens it. */
export function ApprovalCard({ item, nav }: ApprovalCardProps) {
  return (
    <article className="mesa-cartao waiting">
      <span className="mesa-tipo">
        <span>{t.waitingKinds[item.kind]}</span>
      </span>
      {item.text && <code className="mesa-trecho">{item.text}</code>}
      <footer className="mesa-pe">
        <Who bot={item.bot} note={item.title} />
        <button type="button" className="acao-texto" onClick={() => nav.openMessage(item.bot.id, item.thread, item.ms)}>
          {t.deskOpen}
        </button>
      </footer>
    </article>
  )
}

interface RequestCardProps {
  request: ChatRequest
  bots: Bots
}

/** A change an AI asked for from a chat; nothing happens until it is approved here. */
export function RequestCard({ request, bots }: RequestCardProps) {
  const { line, note } = describeRequest(request, bots)
  return (
    <article className="mesa-cartao waiting">
      <span className="mesa-tipo">
        <span>{t.deskRequestKind}</span>
      </span>
      <p className="mesa-titulo">{line}</p>
      {[request.why, note].filter(Boolean).map((extra) => (
        <small key={extra} className="mesa-apoio">
          {extra}
        </small>
      ))}
      <footer className="mesa-acoes">
        <button type="button" className="mesa-cheio" onClick={() => void bots.act('request_approve', { request: request.id })}>
          {t.requestApprove}
        </button>
        <button type="button" className="acao-texto" onClick={() => void bots.act('request_discard', { request: request.id })}>
          {t.requestDiscard}
        </button>
      </footer>
    </article>
  )
}
