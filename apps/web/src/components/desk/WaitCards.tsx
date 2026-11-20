import { useState } from 'react'
import type { ChatRequest } from '../../api'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import type { ChatNav } from '../chat/nav'
import type { Waiting } from '../chat/waiting'
import { describeRequest } from './requests'
import { Who } from './Who'

interface ApprovalCardProps {
  item: Waiting
  bots: Bots
  nav: ChatNav
}

/** Proposals can start a draft here. Open leads to the original message for every kind. */
export function ApprovalCard({ item, bots, nav }: ApprovalCardProps) {
  const [approving, setApproving] = useState(false)
  const proposal = item.kind === 'proposal'
  const { bot, thread, ms } = item
  const note = proposal && bot.persona?.name && bot.persona.name !== bot.name ? `${bot.name} · ${item.title}` : item.title
  const approve = async () => {
    setApproving(true)
    await bots.act('approve', { id: bot.id, thread, answer_ms: ms })
    setApproving(false)
  }

  return (
    <article className="mesa-cartao waiting">
      <span className="mesa-tipo">
        <span>{t.waitingKinds[item.kind]}</span>
      </span>
      {item.text && (proposal ? <p className="mesa-proposta">{item.text}</p> : <code className="mesa-trecho">{item.text}</code>)}
      <footer className="mesa-pe">
        <Who bot={bot} note={note} />
        <button type="button" className="acao-texto" onClick={() => nav.openMessage(bot.id, thread, ms)}>
          {t.deskOpen}
        </button>
      </footer>
      {proposal && (
        <div className="mesa-acoes">
          <button type="button" className="mesa-cheio" disabled={bot.busy || approving} onClick={() => void approve()}>
            {t.approveEdit}
          </button>
          <small className="mesa-apoio">{t.draftScope}</small>
        </div>
      )}
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
