// Line icons, one set and one stroke (1.6), drawn for this panel.

interface IconProps {
  name: 'xray' | 'efficiency' | 'agents' | 'ask-many' | 'plus' | 'sliders'
}

const PATHS: Record<IconProps['name'], string> = {
  xray: 'M4 8V6.5A2.5 2.5 0 0 1 6.5 4H8M16 4h1.5A2.5 2.5 0 0 1 20 6.5V8M20 16v1.5a2.5 2.5 0 0 1-2.5 2.5H16M8 20H6.5A2.5 2.5 0 0 1 4 17.5V16M4 12h16',
  efficiency: 'M5 17a8 8 0 1 1 14 0M12 13l3.5-3.5M12 13h0',
  agents:
    'M4 6.5A2.5 2.5 0 0 1 6.5 4h7A2.5 2.5 0 0 1 16 6.5v4a2.5 2.5 0 0 1-2.5 2.5H9l-3.5 3v-3.1A2.5 2.5 0 0 1 4 10.5zM19 9.5a1.8 1.8 0 0 1 1 1.6V15a2 2 0 0 1-2 2v2.5L15 17h-3.2',
  'ask-many': 'M4 12h5M13 6l7-2M13 12h7M13 18l7 2M9 12l4-6M9 12l4 6',
  plus: 'M12 5v14M5 12h14',
  sliders: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4',
}

export function Icon({ name }: IconProps) {
  return (
    <svg className="icone" viewBox="0 0 24 24" aria-hidden="true">
      <path d={PATHS[name]} />
    </svg>
  )
}
