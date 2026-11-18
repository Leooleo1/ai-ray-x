// Texts in English and Portuguese, picked from the browser language, plus number formats.

const en = {
  setupAgain: 'Set up projects and bots',
  bots: 'Bots',
  addBot: 'Add a bot',
  botsTitle: 'Bots in your projects',
  botsHint: 'the AI you already use, inside the project, with its context',
  botsRule: 'Only where you turn it on. It reads while talking and edits only after you approve. Each message runs Claude Code or Codex for real and counts against your plan; every answer shows what it read.',
  activate: 'Turn on',
  deactivate: 'Turn off',
  requestsShort: 'requests',
  noCandidates: 'No project with Claude Code or Codex usage yet.',
  botTalkHint: 'reads only · edits when you approve',
  newConversation: 'New conversation',
  botReading: 'reading the project',
  botEditing: 'editing',
  proposal: 'Proposal',
  approveEdit: 'Approve and edit',
  approved: 'You approved',
  editedBadge: 'edited',
  readWord: 'read',
  claudeEditScope: 'edits files in this project, no terminal',
  codexEditScope: 'edits and runs commands in the Codex sandbox, only in this folder',
  conversationReset: 'new conversation',
  composeBot: 'Ask',
  botStarters: ['What does this project do?', 'What changed last?', 'Where is the biggest risk?'],
  compactingAt: 'compacting at',
  rereadPerRequest: 're-read per request',
  summariesPer100: 'summaries per 100 requests',
  afterSummary: 'after each summary (restart + re-read)',
  backToChannel: 'Back to the channel',
  channels: 'Channel',
  directs: 'Direct messages',
  available: 'available',
  typingShort: 'typing…',
  roomRule: 'Only in this project, and only when you ask.',
  channelHint: 'Raio, Dieta and Memo · mention one to ask',
  mentionHint: 'Mention an agent: @raio, @dieta or @memo',
  composeChannel: 'Write to @raio, @dieta or @memo',
  composeDirect: 'Message',
  send: 'Send',
  today: 'Today',
  tabAgents: 'Agents',
  roomTitle: 'Agents room',
  roomSubtitle: 'Raio, Dieta and Memo, only in this project and only when asked',
  roomEmpty: 'The conversation starts when you ask an agent, here or with /token-diet.',
  roomOffTitle: 'Room off in this folder',
  roomOff: 'The agents only act in projects with the token-diet skill installed (.claude/skills/token-diet). Open raiox from such a project.',
  apply: 'Apply',
  confirm: 'Confirm',
  cancel: 'Cancel',
  applied: 'applied',
  undo: 'undo',
  applyTarget: 'writes {p} in .claude/settings.local.json of this project',
  memoSaved: 'Memo kept the lesson in ai-memory',
  memoFailed: 'applied; Memo could not keep the lesson',
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
  setupAgain: 'Configurar projetos e bots',
  bots: 'Bots',
  addBot: 'Ativar bot',
  botsTitle: 'Bots nos seus projetos',
  botsHint: 'a IA que você já usa, dentro do projeto, com o contexto dele',
  botsRule: 'Só onde você ativar. Conversa só lendo e edita depois que você aprova. Cada mensagem roda o Claude Code ou o Codex de verdade e conta no seu plano; cada resposta mostra quanto leu.',
  activate: 'Ativar',
  deactivate: 'Desativar',
  requestsShort: 'pedidos',
  noCandidates: 'Nenhum projeto com uso de Claude Code ou Codex ainda.',
  botTalkHint: 'só lê · edita quando você aprova',
  newConversation: 'Nova conversa',
  botReading: 'lendo o projeto',
  botEditing: 'editando',
  proposal: 'Proposta',
  approveEdit: 'Aprovar e editar',
  approved: 'Você aprovou',
  editedBadge: 'editou',
  readWord: 'leu',
  claudeEditScope: 'edita arquivos deste projeto, sem terminal',
  codexEditScope: 'edita e roda comandos no sandbox do Codex, só nesta pasta',
  conversationReset: 'nova conversa',
  composeBot: 'Pergunte ao',
  botStarters: ['O que este projeto faz?', 'O que mudou por último?', 'Onde está o maior risco?'],
  compactingAt: 'compactando em',
  rereadPerRequest: 'relido por pedido',
  summariesPer100: 'resumos a cada 100 pedidos',
  afterSummary: 'depois de cada resumo (recomeça + relê)',
  backToChannel: 'Voltar ao canal',
  channels: 'Canal',
  directs: 'Mensagens diretas',
  available: 'disponível',
  typingShort: 'digitando…',
  roomRule: 'Só neste projeto, e só quando você chama.',
  channelHint: 'Raio, Dieta e Memo · mencione um para perguntar',
  mentionHint: 'Mencione um agente: @raio, @dieta ou @memo',
  composeChannel: 'Escreva para @raio, @dieta ou @memo',
  composeDirect: 'Mensagem para',
  send: 'Enviar',
  today: 'Hoje',
  tabAgents: 'Agentes',
  roomTitle: 'Sala de agentes',
  roomSubtitle: 'Raio, Dieta e Memo, só neste projeto e só quando chamados',
  roomEmpty: 'A conversa começa quando você pergunta a um agente, aqui ou com /token-diet.',
  roomOffTitle: 'Sala desligada nesta pasta',
  roomOff: 'Os agentes só agem em projetos com a skill token-diet instalada (.claude/skills/token-diet). Abra o raiox a partir de um deles.',
  apply: 'Aplicar',
  confirm: 'Confirmar',
  cancel: 'Cancelar',
  applied: 'aplicado',
  undo: 'desfazer',
  applyTarget: 'grava {p} no .claude/settings.local.json deste projeto',
  memoSaved: 'o Memo guardou a lição no ai-memory',
  memoFailed: 'aplicado; o Memo não conseguiu guardar a lição',
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
