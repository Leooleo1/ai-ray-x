import { useEffect, useState } from 'react'
import { type Bot, type Delivery, botShotImage } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import { compact, t } from '../../i18n'
import { Avatar } from '../Avatar'
import type { ChatNav } from '../chat/nav'
import { hourOf, toneOf } from './days'

/** A screenshot as a thumbnail; a click shows it at the list's width. */
function ShotThumb({ bot, name, place }: { bot: Bot; name: string; place: string }) {
  const [image, setImage] = useState<string | null>(null)
  const [wide, setWide] = useState(false)
  useEffect(() => {
    let alive = true
    botShotImage(bot.id, name)
      .then((loaded) => alive && setImage(loaded.image))
      .catch(() => alive && setImage(null))
    return () => {
      alive = false
    }
  }, [bot.id, name])
  if (!image) return <span className="entregas-print vazio">{t.shotLoading}</span>
  return (
    <button type="button" className={`entregas-print ${wide ? 'largo' : ''}`} aria-label={wide ? t.shotSmaller : t.shotLarger} onClick={() => setWide(!wide)}>
      <img src={image} alt={place} />
    </button>
  )
}

/** What the line says: a screenshot's page, the files of a draft, or the delivery's own title. */
function titleOf(item: Delivery): string {
  return item.kind === 'shot' ? t.deliveryShotOf.replace('{target}', item.title) : item.title
}

interface DeliveryRowProps {
  item: Delivery
  bot: Bot
  nav: ChatNav
}

/** One delivery on the day's rail: when, what kind, by whom, what it was, and the way to its conversation. */
export function DeliveryRow({ item, bot, nav }: DeliveryRowProps) {
  const files = item.files && item.files.length > 0 && item.kind !== 'draft' ? item.files : []
  const excerpt = (item.kind === 'routine' || item.kind === 'audit') && item.detail ? item.detail : ''
  return (
    <li className={`entregas-item ${toneOf(item)}`}>
      <time className="entregas-hora">{hourOf(item.ms)}</time>
      <i className="entregas-no" aria-hidden="true" />
      <div className="entregas-corpo">
        <div className="entregas-linha">
          <span className="entregas-tipo">{t.deliveryKinds[item.kind]}</span>
          <span className="entregas-quem">
            <Avatar agent={faceForBot(bot)} size={16} state="still" />
            <span>{bot.name}</span>
            <small>{AGENT_LABEL[bot.agent]}</small>
          </span>
          {item.undone && <span className="entregas-cifra entregas-desfeito">{t.deliveryUndone}</span>}
          {item.tokens ? <span className="entregas-cifra entregas-custo">{`${compact(item.tokens)} tokens`}</span> : null}
          <button type="button" className="acao-texto entregas-abrir" onClick={() => nav.openMessage(item.bot, item.thread, item.ms)}>
            {t.deliveryOpen}
          </button>
        </div>
        <p className="entregas-titulo">{titleOf(item)}</p>
        {files.length > 0 && <p className="entregas-arquivos">{files.join('   ')}</p>}
        {excerpt && <p className="entregas-trecho">{excerpt}</p>}
        {item.kind === 'shot' && item.detail && <ShotThumb bot={bot} name={item.detail} place={item.title} />}
      </div>
    </li>
  )
}
