// Line icons, one set and one stroke (1.6), split into strokes so each icon redraws cleanly on hover.

import type { CSSProperties } from 'react'

export type IconName =
  | 'xray'
  | 'efficiency'
  | 'usage'
  | 'flows'
  | 'more'
  | 'search'
  | 'agents'
  | 'desk'
  | 'deliveries'
  | 'wiki'
  | 'ask-many'
  | 'plus'
  | 'sliders'
  | 'hash'
  | 'sidebar-collapse'
  | 'sidebar-expand'

interface IconProps {
  name: IconName
}

const STROKES: Record<IconName, string[]> = {
  xray: [
    'M4 8V6.5A2.5 2.5 0 0 1 6.5 4H8',
    'M16 4h1.5A2.5 2.5 0 0 1 20 6.5V8',
    'M20 16v1.5a2.5 2.5 0 0 1-2.5 2.5H16',
    'M8 20H6.5A2.5 2.5 0 0 1 4 17.5V16',
    'M4 12h16',
  ],
  efficiency: ['M5 17a8 8 0 1 1 14 0', 'M12 13l3.5-3.5', 'M12 13h.01'],
  usage: ['M4 4v16h16', 'M7.5 15l3.5-4 3 2.5 4.5-6'],
  search: ['M11 4.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13z', 'M16 16l4 4'],
  more: ['M5.5 12h.01', 'M12 12h.01', 'M18.5 12h.01'],
  flows: ['M5 5h5v5H5z', 'M14 14h5v5h-5z', 'M7.5 10v2.5a2 2 0 0 0 2 2H14'],
  agents: [
    'M4 6.5A2.5 2.5 0 0 1 6.5 4h7A2.5 2.5 0 0 1 16 6.5v4a2.5 2.5 0 0 1-2.5 2.5H9l-3.5 3v-3.1A2.5 2.5 0 0 1 4 10.5z',
    'M19 9.5a1.8 1.8 0 0 1 1 1.6V15a2 2 0 0 1-2 2v2.5L15 17h-3.2',
  ],
  desk: ['M4 5h7v6H4z', 'M13 5h7v4h-7z', 'M13 11h7v8h-7z', 'M4 13h7v6H4z'],
  deliveries: ['M4 7.5 12 3.5l8 4v9l-8 4-8-4z', 'M4 7.5l8 4 8-4', 'M12 11.5v9'],
  wiki: ['M5 4.5h9.5A2.5 2.5 0 0 1 17 7v12.5H7.5A2.5 2.5 0 0 1 5 17z', 'M5 17a2.5 2.5 0 0 1 2.5-2.5H17', 'M9 8h5', 'M9 11h3'],
  'ask-many': ['M4 12h5', 'M9 12l4-6 7-2', 'M9 12h11', 'M9 12l4 6 7 2'],
  plus: ['M12 5v14', 'M5 12h14'],
  sliders: ['M4 7h9M17 7h3', 'M4 17h3M11 17h9', 'M15 5v4', 'M9 15v4'],
  hash: ['M10 4.5L8 19.5', 'M16 4.5l-2 15', 'M5 9.5h14', 'M4.5 14.5h14'],
  'sidebar-collapse': ['M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5v13a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5z', 'M9.5 4v16', 'M15.5 9.5 13 12l2.5 2.5'],
  'sidebar-expand': ['M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5v13a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5z', 'M9.5 4v16', 'M13.5 9.5 16 12l-2.5 2.5'],
}

export function Icon({ name }: IconProps) {
  const strokes = STROKES[name]
  return (
    <svg className="icone" viewBox="0 0 24 24" aria-hidden="true">
      {strokes.map((d, index) => (
        <path key={index} d={d} pathLength={100} style={{ '--i': index } as CSSProperties} />
      ))}
    </svg>
  )
}
