import { useEffect, useState } from 'react'
import { type Bot, type BotMessage, botShotImage } from '../../api'
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
