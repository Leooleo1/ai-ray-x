import { type ReactNode, useState } from 'react'
import { o } from './texts'

const COPIED_MS = 1600

interface FrameProps {
  children: ReactNode
  actions: ReactNode
}

/** A step's body, which scrolls, and its actions, which stay at the bottom. */
export function Frame({ children, actions }: FrameProps) {
  return (
    <>
      <div className="bv-conteudo">{children}</div>
      <footer className="bv-acoes">{actions}</footer>
    </>
  )
}

/** A command the person runs in their terminal, with a copy button. */
export function Command({ label, command }: { label?: string; command: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void navigator.clipboard.writeText(command).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), COPIED_MS)
    })
  }
  return (
    <span className="bv-comando">
      {label && <small>{label}</small>}
      <code>{command}</code>
      <button type="button" onClick={copy}>
        {copied ? o.copied : o.copy}
      </button>
    </span>
  )
}
