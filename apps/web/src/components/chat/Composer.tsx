import { type ClipboardEvent, type DragEvent, type KeyboardEvent, type ReactNode, type RefObject, useEffect, useImperativeHandle, useRef, useState } from 'react'
import { t } from '../../i18n'
import { type AccessMode, nextMode } from '../../modes'
import { ModeChip } from './ModeChip'

/** The field grows with the text up to this many pixels, then scrolls. */
const MAX_HEIGHT = 168
const MAX_IMAGES = 4
const MAX_IMAGE_BYTES = 5 * 1024 * 1024
const ALLOWED_TYPES = new Set(['image/png', 'image/jpeg', 'image/webp', 'image/gif'])
const ACCEPT_IMAGES = 'image/png,image/jpeg,image/webp,image/gif'

interface PendingImage {
  id: string
  name: string
  dataUrl: string
}

// Module scope: switching to another bot and back keeps what was typed and attached.
const drafts = new Map<string, string>()
const draftImages = new Map<string, PendingImage[]>()
let nextImageId = 1

function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => (typeof reader.result === 'string' ? resolve(reader.result) : reject(new Error('unreadable image')))
    reader.onerror = () => reject(reader.error ?? new Error('unreadable image'))
    reader.readAsDataURL(file)
  })
}

/** Something "/" at the start of the field offers: a helper to ask, for instance. */
export interface SlashCommand {
  id: string
  label: string
  hint: string
  icon: ReactNode
  /** Runs at once; or, with `insert`, puts that text in the field to be completed. */
  run: () => void
  insert?: string
}

/** The commands whose name starts with what follows "/" (the whole draft, while it is one word). */
function slashMatches(draft: string, commands: SlashCommand[]): SlashCommand[] | null {
  const typed = /^\/(\S*)$/.exec(draft)
  if (!typed) return null
  const word = typed[1].toLowerCase()
  return commands.filter((c) => c.id.startsWith(word) || c.label.toLowerCase().startsWith(word))
}

interface ComposerProps {
  /** Whose draft this is (one per chat). */
  draftKey: string
  placeholder: string
  inputRef?: RefObject<HTMLTextAreaElement | null>
  appendRef?: RefObject<((text: string) => 'too-long' | 'sending' | null) | null>
  maxLength: number
  busy: boolean
  blocked?: boolean
  /** Lets the person attach up to 4 images by button, paste or drag-and-drop. */
  allowImages?: boolean
  /** Resolves true when the message went out, so the draft clears. */
  onSend: (text: string, images?: string[]) => Promise<boolean>
  onStop: () => void
  /** While the bot works in this conversation: sends a correction of its course. */
  onCorrect?: (text: string) => Promise<boolean>
  /** The bot's mode (allow, approve for me, auto, full), as a coding CLI shows it; Shift+Tab steps to the next. */
  mode?: { value: AccessMode; onPick: (mode: AccessMode) => void }
  /** Offered by typing "/" at the start of the field. */
  commands?: SlashCommand[]
}

/** One line that grows. Enter sends, Shift+Enter breaks the line; while the bot works, the button and Esc stop it. */
export function Composer({ draftKey, placeholder, inputRef, appendRef, maxLength, busy, blocked = false, allowImages = false, onSend, onStop, onCorrect, mode, commands = [] }: ComposerProps) {
  const [draft, setDraft] = useState(() => drafts.get(draftKey) ?? '')
  const [images, setImages] = useState<PendingImage[]>(() => (allowImages ? draftImages.get(draftKey) ?? [] : []))
  const [attachError, setAttachError] = useState<string | null>(null)
  const [dragging, setDragging] = useState(false)
  const [sending, setSending] = useState(false)
  const [slashActive, setSlashActive] = useState(0)
  const localField = useRef<HTMLTextAreaElement>(null)
  const field = inputRef ?? localField
  const picker = useRef<HTMLInputElement>(null)
  const dragDepth = useRef(0)

  useImperativeHandle(appendRef, () => (text) => {
    if (sending) return 'sending'
    const next = draft ? draft + '\n\n' + text : text
    if (maxLength > 0 && next.length > maxLength) return 'too-long'
    setDraft(next)
    return null
  })

  useEffect(() => {
    setDraft(drafts.get(draftKey) ?? '')
    setImages(allowImages ? draftImages.get(draftKey) ?? [] : [])
    setAttachError(null)
    setDragging(false)
    dragDepth.current = 0
    field.current?.focus()
  }, [draftKey, allowImages])
  useEffect(() => {
    drafts.set(draftKey, draft)
    const el = field.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`
  }, [draft, draftKey])
  useEffect(() => {
    if (allowImages) draftImages.set(draftKey, images)
  }, [allowImages, draftKey, images])

  // While the bot works here, what is typed corrects its course instead of asking anew.
  const correcting = busy && onCorrect !== undefined
  const canAttach = allowImages && !busy && !sending

  const addFiles = async (incoming: FileList | File[]) => {
    if (!canAttach) return
    const files = Array.from(incoming)
    if (files.length === 0) return
    let error: string | null = null
    const accepted: File[] = []
    let remaining = MAX_IMAGES - images.length
    for (const file of files) {
      if (!ALLOWED_TYPES.has(file.type)) {
        error = t.imageBadType
        continue
      }
      if (file.size === 0 || file.size > MAX_IMAGE_BYTES) {
        error = t.imageTooLarge
        continue
      }
      if (remaining <= 0) {
        error = t.imageTooMany.replace('{n}', String(MAX_IMAGES))
        continue
      }
      accepted.push(file)
      remaining -= 1
    }
    setAttachError(error)
    if (accepted.length === 0) return
    const loaded: PendingImage[] = []
    for (const file of accepted) {
      try {
        const dataUrl = await readAsDataUrl(file)
        loaded.push({ id: `img-${nextImageId++}`, name: file.name, dataUrl })
      } catch {
        setAttachError(t.imageBadType)
      }
    }
    if (loaded.length > 0) {
      setImages((current) => [...current, ...loaded].slice(0, MAX_IMAGES))
    }
  }

  const removeImage = (id: string) => {
    setImages((current) => current.filter((item) => item.id !== id))
    setAttachError(null)
  }

  const send = async () => {
    if (sending) return
    const text = draft.trim()
    if (correcting) {
      if (!text) return
      setSending(true)
      try {
        if (await onCorrect(text)) setDraft('')
      } finally {
        setSending(false)
      }
      return
    }
    if (busy || blocked) return
    const attached = allowImages ? images.map((item) => item.dataUrl) : []
    if (!text && attached.length === 0) return
    setSending(true)
    setAttachError(null)
    try {
      const ok = attached.length > 0 ? await onSend(text, attached) : await onSend(text)
      if (ok) {
        setDraft('')
        setImages([])
      }
    } finally {
      setSending(false)
    }
  }

  const slash = busy ? null : slashMatches(draft, commands)
  useEffect(() => setSlashActive(0), [draft])
  const runSlash = (command: SlashCommand | undefined) => {
    if (!command) return
    setDraft(command.insert ?? '')
    if (command.insert === undefined) command.run()
  }

  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (slash && slash.length > 0 && ['ArrowDown', 'ArrowUp', 'Enter', 'Tab'].includes(event.key) && !event.shiftKey) {
      event.preventDefault()
      if (event.key === 'ArrowDown') setSlashActive((i) => Math.min(i + 1, slash.length - 1))
      else if (event.key === 'ArrowUp') setSlashActive((i) => Math.max(i - 1, 0))
      else runSlash(slash[slashActive])
      return
    }
    if (slash && event.key === 'Escape') {
      event.preventDefault()
      setDraft('')
      return
    }
    if (event.key === 'Tab' && event.shiftKey && mode) {
      event.preventDefault()
      mode.onPick(nextMode(mode.value))
    } else if (event.key === 'Escape' && busy) {
      event.preventDefault()
      onStop()
    } else if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      void send()
    }
  }

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    if (!canAttach) return
    const pasted: File[] = []
    for (const item of Array.from(event.clipboardData.items)) {
      if (item.kind === 'file') {
        const file = item.getAsFile()
        if (file) pasted.push(file)
      }
    }
    if (pasted.length === 0) return
    event.preventDefault()
    void addFiles(pasted)
  }

  const onDragEnter = (event: DragEvent<HTMLFormElement>) => {
    if (!canAttach || !event.dataTransfer.types.includes('Files')) return
    event.preventDefault()
    dragDepth.current += 1
    setDragging(true)
  }

  const onDragOver = (event: DragEvent<HTMLFormElement>) => {
    if (!canAttach || !event.dataTransfer.types.includes('Files')) return
    event.preventDefault()
    event.dataTransfer.dropEffect = 'copy'
  }

  const onDragLeave = (event: DragEvent<HTMLFormElement>) => {
    if (!canAttach) return
    event.preventDefault()
    dragDepth.current = Math.max(0, dragDepth.current - 1)
    if (dragDepth.current === 0) setDragging(false)
  }

  const onDrop = (event: DragEvent<HTMLFormElement>) => {
    if (!canAttach) return
    event.preventDefault()
    dragDepth.current = 0
    setDragging(false)
    if (event.dataTransfer.files.length > 0) {
      void addFiles(event.dataTransfer.files)
    }
  }

  const hasContent = Boolean(draft.trim()) || (allowImages && images.length > 0)
  const nearLimit = maxLength > 0 && draft.length >= Math.floor(maxLength * 0.85)

  return (
    <form
      className={`compositor ${dragging ? 'arrastando' : ''}`}
      onDragEnter={onDragEnter}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      onSubmit={(event) => {
        event.preventDefault()
        void send()
      }}
    >
      {slash && slash.length > 0 && (
        <div className="comando-menu" role="listbox" aria-label={t.slashLabel}>
          {slash.map((command, i) => (
            <button
              key={command.id}
              type="button"
              role="option"
              aria-selected={i === slashActive}
              onPointerMove={() => setSlashActive(i)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => runSlash(command)}
            >
              {command.icon}
              <b>{`/${command.id}`}</b>
              <span>{command.hint}</span>
            </button>
          ))}
        </div>
      )}
      {allowImages && images.length > 0 && (
        <div className="compositor-anexos">
          {images.map((item) => (
            <div key={item.id} className="compositor-anexo">
              <img src={item.dataUrl} alt={item.name || t.imageAttached} />
              <button
                type="button"
                className="compositor-anexo-remover"
                onClick={() => removeImage(item.id)}
                aria-label={t.removeImage}
                title={t.removeImage}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
      )}
      {dragging && <p className="compositor-dica">{t.dropImages}</p>}
      {attachError && <p className="compositor-erro">{attachError}</p>}
      <div className="compositor-linha">
        {allowImages && !correcting && (
          <>
            <input
              ref={picker}
              type="file"
              accept={ACCEPT_IMAGES}
              multiple
              hidden
              onChange={(event) => {
                if (event.target.files) void addFiles(event.target.files)
                event.target.value = ''
              }}
            />
            <button
              type="button"
              className="anexar"
              disabled={!canAttach || images.length >= MAX_IMAGES}
              onClick={() => picker.current?.click()}
              aria-label={t.attachImages}
              title={t.attachImages}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M21.44 11.05l-8.49 8.49a5.5 5.5 0 0 1-7.78-7.78l8.49-8.49a3.5 3.5 0 0 1 4.95 4.95l-8.49 8.49a1.5 1.5 0 0 1-2.12-2.12l7.78-7.78" />
              </svg>
            </button>
          </>
        )}
        <textarea
          ref={field}
          rows={1}
          value={draft}
          maxLength={maxLength}
          placeholder={correcting ? t.correctPlaceholder : placeholder}
          aria-label={correcting ? t.correctPlaceholder : placeholder}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKey}
          onPaste={onPaste}
        />
        {nearLimit && (
          <small className={`compositor-contador ${draft.length >= maxLength ? 'limite' : ''}`}>
            {`${draft.length}/${maxLength}`}
          </small>
        )}
        {mode && <ModeChip mode={mode.value} onPick={mode.onPick} />}
        {correcting && draft.trim() ? (
          <button className="enviar corrigir" type="submit" disabled={sending} aria-label={t.correctSend} title={t.correctSend}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M9 14l-4-4 4-4M5 10h9a5 5 0 0 1 0 10h-3" />
            </svg>
          </button>
        ) : busy ? (
          <button className="enviar parar" type="button" onClick={onStop} aria-label={t.stop} title={`${t.stop} (Esc)`}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="7" y="7" width="10" height="10" rx="2" />
            </svg>
          </button>
        ) : (
          <button className="enviar" type="submit" disabled={!hasContent || sending || blocked} aria-label={t.send}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 19V5M5 12l7-7 7 7" />
            </svg>
          </button>
        )}
      </div>
    </form>
  )
}
