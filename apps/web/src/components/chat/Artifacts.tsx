import { useEffect, useState } from 'react'
import { type BotArtifact, fetchArtifact, IS_REMOTE } from '../../api'
import { t } from '../../i18n'

function Artifact({ bot, file }: { bot: string; file: BotArtifact }) {
  const [url, setUrl] = useState<string | null>(null)
  const [missing, setMissing] = useState(false)
  const [previewFailed, setPreviewFailed] = useState(false)
  const name = file.name || t.artifactFile
  const id = file.id
  useEffect(() => {
    if (!id || IS_REMOTE) return undefined
    let active = true
    let objectUrl: string | undefined
    fetchArtifact(bot, id)
      .then(({ bytes, mime }) => {
        if (!active) return
        const data = Uint8Array.from(atob(bytes), (byte) => byte.charCodeAt(0))
        objectUrl = URL.createObjectURL(new Blob([data], { type: mime }))
        setUrl(objectUrl)
      })
      .catch(() => { if (active) setMissing(true) })
    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [bot, id])
  return (
    <figure className="chat-arquivo">
      <figcaption>
        <span>{name}</span>
        {url && <a className="acao-texto" href={url} download={name}>{t.artifactDownload}</a>}
      </figcaption>
      {url && file.mime?.startsWith('image/') && !previewFailed && <img src={url} alt={name} onError={() => setPreviewFailed(true)} />}
      {(!url || previewFailed) && (
        <small role="status">{previewFailed ? t.artifactPreviewUnavailable : !id || missing ? t.artifactUnavailable : IS_REMOTE ? t.artifactDesktop : t.artifactLoading}</small>
      )}
    </figure>
  )
}

export function Artifacts({ bot, files }: { bot: string; files: BotArtifact[] }) {
  return files.map((file, index) => <Artifact key={file.id ?? index} bot={bot} file={file} />)
}
