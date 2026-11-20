import { type ChangeEvent, useEffect, useRef, useState } from 'react'
import { type Profile, fetchProfile, saveProfile } from '../api'
import { Frame } from './parts'
import { o } from './texts'

/** The photo is shrunk to this square before it leaves the browser. */
const PHOTO_PX = 256
const PHOTO_QUALITY = 0.88
const NAME_MAX = 60

const messageOf = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure))

/** A picked image, cropped to its centre square and shrunk, as a JPEG data URL. */
function shrink(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const image = new Image()
    image.onload = () => {
      const side = Math.min(image.width, image.height)
      const canvas = document.createElement('canvas')
      canvas.width = PHOTO_PX
      canvas.height = PHOTO_PX
      canvas.getContext('2d')?.drawImage(image, (image.width - side) / 2, (image.height - side) / 2, side, side, 0, 0, PHOTO_PX, PHOTO_PX)
      URL.revokeObjectURL(url)
      resolve(canvas.toDataURL('image/jpeg', PHOTO_QUALITY))
    }
    image.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error(o.photoUnreadable))
    }
    image.src = url
  })
}

const initials = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => word[0])
    .slice(0, 2)
    .join('')
    .toUpperCase()

interface StepProfileProps {
  onBack: () => void
  onNext: () => void
}

/** Who is using raiox: a name and, if they like, a photo (from the computer or their GitHub). */
export function StepProfile({ onBack, onNext }: StepProfileProps) {
  const [profile, setProfile] = useState<Profile | null>(null)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const picker = useRef<HTMLInputElement>(null)
  useEffect(() => {
    fetchProfile()
      .then((loaded) => {
        setProfile(loaded)
        setName(loaded.name ?? '')
      })
      .catch((failure: unknown) => setError(messageOf(failure)))
  }, [])
  const run = async (change: () => Promise<Profile>) => {
    setBusy(true)
    setError(null)
    try {
      setProfile(await change())
    } catch (failure: unknown) {
      setError(messageOf(failure))
    } finally {
      setBusy(false)
    }
  }
  const pick = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (file) await run(async () => saveProfile({ photo: await shrink(file) }))
  }
  const next = async () => {
    if (name.trim() !== (profile?.name ?? '')) await run(() => saveProfile({ name: name.trim() }))
    onNext()
  }
  return (
    <Frame
      actions={
        <>
          <button type="button" className="bv-btn" onClick={onBack}>
            {o.back}
          </button>
          <button type="button" className="bv-btn cheio bv-principal" disabled={busy} onClick={() => void next()}>
            {o.continue}
          </button>
        </>
      }
    >
      <div className="bv-perfil">
        <button type="button" className="bv-foto" aria-label={o.photoPick} disabled={busy} onClick={() => picker.current?.click()}>
          {profile?.photo ? <img src={profile.photo} alt="" /> : <span>{initials(name) || '?'}</span>}
        </button>
        <input ref={picker} type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(event) => void pick(event)} />
        <div className="bv-perfil-campos">
          <label className="bv-campo">
            <span>{o.profileName}</span>
            <input value={name} maxLength={NAME_MAX} placeholder={o.profileNameHint} onChange={(event) => setName(event.target.value)} />
          </label>
          <div className="bv-perfil-foto-acoes">
            <button type="button" className="bv-btn" disabled={busy} onClick={() => picker.current?.click()}>
              {o.photoPick}
            </button>
            {profile?.github && (
              <button type="button" className="bv-btn" disabled={busy} onClick={() => void run(() => saveProfile({ from_github: true }))}>
                {o.photoGitHub.replace('{login}', profile.github)}
              </button>
            )}
            {profile?.photo && (
              <button type="button" className="acao-texto" disabled={busy} onClick={() => void run(() => saveProfile({ photo: null }))}>
                {o.photoRemove}
              </button>
            )}
          </div>
          <small className="bv-nota">{o.profileNote}</small>
        </div>
      </div>
      {error && <p className="nota failed quebra">{error}</p>}
    </Frame>
  )
}
