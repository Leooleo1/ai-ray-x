import { type ReactNode, useEffect, useRef, useState } from 'react'
import { type AppVersion, type Bot, IS_REMOTE, type Profile, checkApp, fetchAppVersion, fetchProfile, saveSettings } from '../api'
import { AGENT_LABEL, faceForBot, faceOf } from '../avatars'
import { nowStamp, t } from '../i18n'
import { PRESETS } from '../onboarding/presets'
import { TourDialog } from '../tour/TourDialog'
import './profile.css'

interface ProfileMenuProps {
  bots: Bot[]
  updatedAt: Date
  collapsed?: boolean
  onSettings: (tab: 'conexoes' | 'celular') => void
  onSetup: () => void
}

/** Two letters for the monogram: the name's first and last words, else the login. */
function initials(profile: Profile | null): string {
  const words = (profile?.name ?? profile?.github ?? 'raiox').split(/\s+/).filter(Boolean)
  const letters = words.length > 1 ? `${words[0][0]}${words[words.length - 1][0]}` : words[0].slice(0, 2)
  return letters.toUpperCase()
}

const UPDATE_ICON = <path pathLength={100} d="M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19h14" />

const ITEMS: { id: 'settings' | 'phone' | 'help' | 'setup'; icon: ReactNode }[] = [
  { id: 'settings', icon: <path pathLength={100} d="M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4" /> },
  { id: 'phone', icon: <path pathLength={100} d="M9 2.5h6a2.5 2.5 0 0 1 2.5 2.5v14a2.5 2.5 0 0 1-2.5 2.5H9A2.5 2.5 0 0 1 6.5 19V5A2.5 2.5 0 0 1 9 2.5zM10.5 18.5h3" /> },
  { id: 'help', icon: <path pathLength={100} d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM9.5 9.5a2.5 2.5 0 1 1 3.4 2.3c-.6.3-.9.8-.9 1.4V14M12 17.2v.1" /> },
  { id: 'setup', icon: <path pathLength={100} d="M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4" /> },
]

/** The sidebar's foot: who uses the panel, and a menu with settings, the phone, help and the first setup. */
export function ProfileMenu({ bots, updatedAt, collapsed = false, onSettings, onSetup }: ProfileMenuProps) {
  const [profile, setProfile] = useState<Profile | null>(null)
  const [open, setOpen] = useState(false)
  const [touring, setTouring] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    fetchProfile()
      .then(setProfile)
      .catch(() => setProfile(null))
  }, [])
  useEffect(() => {
    if (!open) return
    const outside = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    window.addEventListener('mousedown', outside)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('mousedown', outside)
      window.removeEventListener('keydown', escape)
    }
  }, [open])
  // Check for updates: only the desktop app updates itself, so only it shows the item.
  const [app, setApp] = useState<AppVersion | null>(null)
  const [checking, setChecking] = useState(false)
  const [checked, setChecked] = useState<string | null>(null)
  useEffect(() => {
    if (open) fetchAppVersion().then(setApp).catch(() => undefined)
  }, [open])
  const checkUpdates = async () => {
    setChecking(true)
    setChecked(null)
    try {
      const next = await checkApp()
      setApp(next)
      setChecked(next.available ? t.updateFound.replace('{v}', next.available) : t.upToDate.replace('{v}', next.version))
      // The note above the profile shows the update at once.
      window.dispatchEvent(new Event('raiox:update'))
    } catch (failure: unknown) {
      setChecked(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setChecking(false)
    }
  }
  const toggleAuto = async () => {
    if (!app) return
    try {
      await saveSettings({ auto_update: !app.auto })
      setApp(await fetchAppVersion())
      window.dispatchEvent(new Event('raiox:update'))
    } catch (failure: unknown) {
      setChecked(failure instanceof Error ? failure.message : String(failure))
    }
  }
  const name = profile?.name ?? profile?.github ?? 'raiox'
  const labels = { settings: t.settingsTitle, phone: t.tabPhone, help: t.profileHelp, setup: t.profileRedo }
  const pick = (id: (typeof ITEMS)[number]['id']) => {
    setOpen(false)
    if (id === 'settings') onSettings('conexoes')
    else if (id === 'phone') onSettings('celular')
    else if (id === 'help') setTouring(true)
    else onSetup()
  }
  const first = bots[0]
  const face = first ? faceForBot(first) : faceOf(PRESETS[0].shape, PRESETS[0].color, PRESETS[0].name)
  return (
    <div className="perfil" ref={box}>
      {open && (
        <div className="perfil-menu" role="menu" aria-label={t.profileMenu}>
          <div className="perfil-quem">
            <b>{name}</b>
            {profile?.github && <small>{`@${profile.github} · GitHub`}</small>}
          </div>
          {ITEMS.map((item) => (
            <button key={item.id} type="button" role="menuitem" className="perfil-item" onClick={() => pick(item.id)}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                {item.icon}
              </svg>
              {labels[item.id]}
            </button>
          ))}
          {app?.can_update && !IS_REMOTE && (
            <button type="button" role="menuitem" className="perfil-item" disabled={checking} onClick={() => void checkUpdates()}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                {UPDATE_ICON}
              </svg>
              <span className="perfil-item-texto">
                {checking ? t.checkingUpdates : t.checkUpdates}
                {checked && <small>{checked}</small>}
              </span>
            </button>
          )}
          {app?.can_update && !IS_REMOTE && (
            <button type="button" role="menuitemcheckbox" aria-checked={app.auto} className="perfil-item perfil-auto" title={t.autoUpdateHint} onClick={() => void toggleAuto()}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M20 12a8 8 0 0 1-14.3 4.9M4 12a8 8 0 0 1 14.3-4.9M18.5 3.5v3.6h-3.6M5.5 20.5v-3.6h3.6" />
              </svg>
              <span className="perfil-item-texto">
                {t.autoUpdate}
                <small>{t.autoUpdateShort}</small>
              </span>
              <span className={`config-chave ${app.auto ? 'ligada' : ''}`} aria-hidden="true">
                <i />
              </span>
            </button>
          )}
          <p className="perfil-nota">{t.roomRule}</p>
          {app && <p className="perfil-versao">{t.appVersion.replace('{v}', app.version)}</p>}
        </div>
      )}
      <button
        type="button"
        className={`perfil-botao ${open ? 'aberto' : ''}`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={collapsed ? name : undefined}
        title={collapsed ? name : undefined}
        onClick={() => setOpen(!open)}
      >
        <span className="perfil-monograma" aria-hidden="true">
          {profile?.photo ? <img src={profile.photo} alt="" /> : initials(profile)}
          <i />
        </span>
        <span className="perfil-nome">
          <b>{name}</b>
          <small>{`${t.updated} ${nowStamp(updatedAt).split('·').pop()?.trim() ?? ''}`}</small>
        </span>
        <svg className="perfil-seta" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M8 10l4-4 4 4M8 14l4 4 4-4" />
        </svg>
      </button>
      {touring && <TourDialog face={face} agent={AGENT_LABEL[first?.agent ?? 'claude']} onClose={() => setTouring(false)} />}
    </div>
  )
}
