/** How freely a bot works, as a coding CLI's modes go (Shift+Tab steps through them, freest last). */
export type AccessMode = 'allow' | 'approve' | 'auto' | 'full'

export const MODES: AccessMode[] = ['allow', 'approve', 'auto', 'full']

interface ModeGrants {
  auto_edit: boolean
  full_access: boolean
  bypass?: boolean
}

/** The mode a bot's grants amount to. */
export function modeOf(grants: ModeGrants): AccessMode {
  if (grants.full_access) return grants.bypass ? 'full' : 'auto'
  return grants.auto_edit ? 'approve' : 'allow'
}

/** The grants a mode sets (running what is safe stays as the person left it in the first two). */
export function grantsFor(mode: AccessMode): Record<string, boolean> {
  switch (mode) {
    case 'allow':
      return { full_access: false, bypass: false, auto_edit: false }
    case 'approve':
      return { full_access: false, bypass: false, auto_edit: true }
    case 'auto':
      return { full_access: true, bypass: false }
    case 'full':
      return { full_access: true, bypass: true }
  }
}

/** The mode after this one, around the circle. */
export const nextMode = (mode: AccessMode): AccessMode => MODES[(MODES.indexOf(mode) + 1) % MODES.length]
