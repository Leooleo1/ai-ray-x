import { useEffect, useState } from 'react'
import { type Bot, type BotMessage, botShotImage, botUploadImage } from '../../api'
import { t } from '../../i18n'

/** A screenshot the AI asked for: where it was taken, and the image, wider on a tap. */
export function ShotBlock({ bot, message }: { bot: Bot; message: BotMessage }) {
  const [image, setImage] = useState<string | null>(null)
  const [wide, setWide] = useState(false)
  const name = message.data.name ?? ''
  useEffect(() => {
    let alive = true
    botShotImage(bot.id, name)
      .then((loaded) => alive && setImage(loaded.image))
      .catch(() => alive && setImage(null))
    return () => {
      alive = false
    }
  }, [bot.id, name])
  return (
    <figure className={`print-ia ${wide ? 'largo' : ''}`}>
      <figcaption>{t.shotTaken.replace('{p}', message.data.place ?? message.data.target ?? '')}</figcaption>
      {image ? (
        <button type="button" onClick={() => setWide(!wide)} aria-label={wide ? t.shotSmaller : t.shotLarger}>
          <img src={image} alt={message.data.place ?? ''} />
        </button>
      ) : (
        <span className="print-ia-carregando">{t.shotLoading}</span>
      )}
    </figure>
  )
}

/** One image attached by the person: thumbnail until tapped, then shown larger. */
function UploadThumb({ bot, name }: { bot: Bot; name: string }) {
  const [image, setImage] = useState<string | null>(null)
  const [wide, setWide] = useState(false)
  useEffect(() => {
    let alive = true
    botUploadImage(bot.id, name)
      .then((loaded) => alive && setImage(loaded.image))
      .catch(() => alive && setImage(null))
    return () => {
      alive = false
    }
  }, [bot.id, name])
  return (
    <figure className={`print-ia minha-imagem ${wide ? 'largo' : ''}`}>
      {image ? (
        <button type="button" onClick={() => setWide(!wide)} aria-label={wide ? t.shotSmaller : t.shotLarger}>
          <img src={image} alt={t.imageAttached} />
        </button>
      ) : (
        <span className="print-ia-carregando">{t.shotLoading}</span>
      )}
    </figure>
  )
}

/** Images the person sent with a message: thumbnails, each opening larger on a tap. */
export function UploadedImages({ bot, names }: { bot: Bot; names: string[] }) {
  if (names.length === 0) return null
  return (
    <div className="minha-imagens">
      {names.map((name) => (
        <UploadThumb key={name} bot={bot} name={name} />
      ))}
    </div>
  )
}
