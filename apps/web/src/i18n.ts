// Texts in English and Portuguese, picked from the browser language, plus number formats.

const en = {
  tabXray: 'X-ray',
  tabEfficiency: 'Efficiency',
  dietTitleMain: 'Token diet',
  dietSubtitle: 'what Raio, Dieta and Memo found',
  raioLine: 'context in the current session',
  dietaLine: 'less context re-read',
  suggestions: 'suggestions',
  memoLine: 'lessons kept · when you accept a suggestion',
  dietaTitle: 'Suggestions',
  dietaHint: 'saving over the period · suggestions only',
  upTo: 'up to ',
  sessionTitle: 'Current session',
  requests: 'requests since compaction',
  now: 'now',
  startup: 'loaded at startup',
  title: 'Your AI agents',
  subtitle: 'Claude Code and Codex, read from this machine',
  days: 'days',
  updated: 'up to date',
  loading: 'reading your logs',
  failed: 'could not read the report',
  tokens: 'tokens processed',
  apiValue: 'in API value · Claude Code',
  streak: 'days in a row',
  clock: 'Hours of the day',
  peak: 'peak',
  context: 'Context per request',
  median: 'median',
  p90: '90% under',
  compacts: 'compacts at',
  overShare: '{p} of requests carry over 400K tokens',
  rereadShare: 're-reading context is {p} of the cost',
  carried: 'Stays loaded',
  carriedHint: 'tokens re-read because it stayed in context',
  projects: 'Projects',
  secrets: 'Secrets files',
  printed: 'printed into the chat',
  searched: 'searched with grep',
  loaded: 'only loaded',
  written: 'written',
  mentioned: 'mentioned',
  yourPrompts: 'your prompts',
  injected: 'injected text',
  home: 'home folder',
  noToken: 'open the link that raiox printed in the terminal',
  noLogs: 'no Claude Code or Codex logs found on this machine',
}

type Texts = typeof en

const pt: Texts = {
  tabXray: 'Raio-X',
  tabEfficiency: 'Eficiência',
  dietTitleMain: 'Dieta de tokens',
  dietSubtitle: 'o que o Raio, a Dieta e o Memo acharam',
  raioLine: 'de contexto na sessão atual',
  dietaLine: 'menos contexto relido',
  suggestions: 'sugestões',
  memoLine: 'lições guardadas · quando você aceitar uma sugestão',
  dietaTitle: 'Sugestões',
  dietaHint: 'economia no período · só sugestões',
  upTo: 'até ',
  sessionTitle: 'Sessão atual',
  requests: 'pedidos desde a compactação',
  now: 'agora',
  startup: 'carregado na partida',
  title: 'Seus agentes de IA',
  subtitle: 'Claude Code e Codex, lidos desta máquina',
  days: 'dias',
  updated: 'atualizado',
  loading: 'lendo seus registros',
  failed: 'não consegui ler o relatório',
  tokens: 'tokens processados',
  apiValue: 'em valor de API · Claude Code',
  streak: 'dias seguidos',
  clock: 'Horas do dia',
  peak: 'pico',
  context: 'Contexto por pedido',
  median: 'mediana',
  p90: '90% até',
  compacts: 'compacta em',
  overShare: '{p} dos pedidos carregam mais de 400 mil tokens',
  rereadShare: 'reler o contexto é {p} do custo',
  carried: 'Fica carregado',
  carriedHint: 'tokens relidos porque continuaram no contexto',
  projects: 'Projetos',
  secrets: 'Arquivos de segredo',
  printed: 'impressos na conversa',
  searched: 'buscas com grep',
  loaded: 'só carregados',
  written: 'escritos',
  mentioned: 'citados',
  yourPrompts: 'seus pedidos',
  injected: 'texto injetado',
  home: 'pasta pessoal',
  noToken: 'abra o link que o raiox mostrou no terminal',
  noLogs: 'nenhum registro do Claude Code ou do Codex nesta máquina',
}

export const locale = navigator.language.toLowerCase().startsWith('pt') ? 'pt-BR' : 'en-US'
export const t: Texts = locale === 'pt-BR' ? pt : en

const SCALES: [number, string, string][] = [
  [1e9, 'bi', 'B'],
  [1e6, 'mi', 'M'],
  [1e3, 'mil', 'K'],
]

/** 42_100_000_000 -> { value: "42,1", unit: "bi" } (pt) or { value: "42.1", unit: "B" } (en).
 *  `fixedDecimal` keeps one decimal even when it is zero ("42,0"), for the hero. */
export function splitCompact(n: number, fixedDecimal = false): { value: string; unit: string } {
  const scale = SCALES.find(([size]) => Math.abs(n) >= size)
  if (!scale) {
    return { value: Math.round(n).toLocaleString(locale), unit: '' }
  }
  const [size, ptUnit, enUnit] = scale
  const value = (n / size).toLocaleString(locale, { minimumFractionDigits: fixedDecimal ? 1 : 0, maximumFractionDigits: 1 })
  return { value, unit: locale === 'pt-BR' ? ptUnit : enUnit }
}

export function compact(n: number): string {
  const { value, unit } = splitCompact(n)
  if (!unit) return value
  return locale === 'pt-BR' ? `${value} ${unit}` : `${value}${unit}`
}

export function percent(share: number): string {
  return share.toLocaleString(locale, { style: 'percent', maximumFractionDigits: 0 })
}

export function usd(n: number): string {
  const { value, unit } = splitCompact(n)
  if (!unit) return locale === 'pt-BR' ? `US$ ${value}` : `$${value}`
  return locale === 'pt-BR' ? `US$ ${value} ${unit}` : `$${value}${unit}`
}

/** "05 SET – 18 NOV · 75 DIAS" style period, in the browser's language. */
export function period(first: string, last: string, days: number): string {
  const format = (iso: string) =>
    new Date(`${iso}T12:00:00`)
      .toLocaleDateString(locale, { day: '2-digit', month: 'short' })
      .replace('.', '')
      .replace(' de ', ' ')
  return `${format(first)} – ${format(last)} · ${days} ${t.days}`.toUpperCase()
}

export function nowStamp(date: Date): string {
  const day = date.toLocaleDateString(locale, { weekday: 'short', day: '2-digit', month: 'short' })
  const time = date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
  return `${day.replace(/\./g, '').replace(' de ', ' ')} · ${time}`.toUpperCase()
}

export function fill(template: string, value: string): string {
  return template.replace('{p}', value)
}
