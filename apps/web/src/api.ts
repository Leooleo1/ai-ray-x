// Shapes of the local report API and the token handshake with `raiox open`.

export type Source = 'claude' | 'codex'

export interface DayPoint {
  date: string
  requests: number
  tokens: number
}

export interface ModelUsage {
  model: string
  calls: number
  input: number
  cache_read: number
  cache_write: number
  output: number
  thinking: number
}

export interface Overview {
  source: Source
  models: ModelUsage[]
  sessions: number
  first_day: string | null
  last_day: string | null
  active_days: number
  longest_streak: number
  busiest_day: [string, number] | null
  daily: DayPoint[]
  active_hours: number
  by_hour: number[]
  by_weekday: number[]
  projects: [string, number][]
  tools: [string, number][]
  prompts: number
  lines_added: number
  lines_removed: number
  files_edited: number
  subagent_calls: number
  compactions: number
  reported_cost_usd: number | null
}

export interface Carried {
  name: string
  results: number
  images: number
  carried_tokens: number
  share_of_context: number
}

export interface Suggestion {
  id: string
  source: Source
  title: string
  evidence: string
  saving_tokens: number
  saving_share: number
  how: string
  value?: string | null
  confidence: 'net estimate' | 'estimate' | 'exact for past logs'
}

export interface Change {
  id: string
  suggestion: string
  file: string
  key: string
  after: string
  applied_ms: number
  undone_ms: number | null
  lesson: string | null
}

export interface ContextStatus {
  source: Source
  session: string
  project: string
  context_tokens: number
  requests_since_compaction: number
  startup_tokens: number
  loaded_since_compaction: { name: string; tokens: number }[]
  usual_compaction_at: number
}

export interface CompactionPlan {
  cap: number
  net_saving: number
  avg_context: number
  compactions_per_100: number
}

export interface CompactionModel {
  floor: number
  refill: number
  now_avg_context: number
  now_compactions_per_100: number
  calibration: number
  plans: CompactionPlan[]
  best: CompactionPlan | null
}

export interface Efficiency {
  compaction: CompactionModel
  fixed: { kind: string; median_first_request: number; share_of_context: number }[]
  skills_used: [string, number][]
  snapshots: Record<string, unknown>
  cost_shares: [string, number][]
  context: { calls: number; total: number; mean: number; median: number; p90: number }
  buckets: { up_to: number | null; share_of_calls: number; share_of_tokens: number }[]
  compaction_pre_median: number
  carried: Carried[]
}

export interface SourceReport {
  overview: Overview
  efficiency: Efficiency
  secrets: [string, number][]
  suggestions: Suggestion[]
}

export interface Report {
  setup: { done: boolean }
  bots: BotsState
  generated_at: number
  sources: SourceReport[]
  context_status: ContextStatus | null
  changes: Change[]
  applicable: Record<Source, string[]>
  project_dir: string
  outcomes: Outcome[]
}

export interface Period {
  requests: number
  sessions: number
  avg_context: number
  compactions_per_100: number
}

export interface Outcome {
  change_id: string
  suggestion: string
  before: Period
  after: Period
  conclusive: boolean
  context_change: number
}

export type AgentAuthor = 'raio' | 'dieta' | 'memo'

export type BotAgent = 'claude' | 'codex' | 'cursor'

export interface BotNote {
  project: string
  text: string
}

/** One step of a turn's path, or something the AI said between steps ("say"). */
export interface TrailItem {
  kind: BotStep['kind'] | 'say'
  target: string
  detail?: string
  ok?: boolean
  /** Lines read, or files found or changed. */
  count?: number
}

/** The path of a turn: its steps with their results, and the text being written now. */
export interface Trail {
  items: TrailItem[]
  dropped: number
  partial: string
  /** Tokens read and written so far, when the CLI says as it goes. */
  tokens?: number
}

export interface BotStep {
  kind: 'read' | 'search' | 'list' | 'edit' | 'run' | 'web' | 'think' | 'tool'
  target: string
}

export interface Batch {
  id: string
  text: string
  created_ms: number
  items: { bot: string; thread: string }[]
  skipped: string[]
}

export interface BotThread {
  id: string
  title: string
  has_session: boolean
  messages: BotMessage[]
}

export interface Routine {
  id: string
  bot: string
  text: string
  /** 0 = Monday … 6 = Sunday. */
  days: number[]
  hour: number
  minute: number
  enabled: boolean
  thread: string | null
  last_ms: number | null
  /** Runs only when the project's files changed since its last run. */
  only_if_changed?: boolean
  /** When it last skipped its run because nothing changed. */
  skipped_ms?: number | null
}

/** A project's week: tokens of the last 7 days and the 7 before, and what weighed most. */
export interface Week {
  this: number
  last: number
  top: { tool: string; tokens: number } | null
}

export type Place = 'model' | 'settings' | 'memories' | 'connections' | 'wires'

export interface BotReply {
  text: string
  proposal: string | null
  to?: BotNote[]
  open?: Place[]
  /** Commands the AI asks raiox to run, each with why raiox refuses it, if it does. */
  run?: RunRequest[]
  /** The path the turn took. */
  trail?: Trail
  /** In a channel: requests for other members (the handle in `project`). */
  ask?: BotNote[]
  /** Screenshots the AI asked for. */
  shots?: string[]
  /** A reviewer's conclusion about another bot's proposal. */
  verdict?: 'ok' | 'concerns' | 'reject'
  steps?: number | null
  read_tokens: number
  output_tokens: number
  cost_usd: number | null
  duration_ms: number
}

export interface RunRequest {
  command: string
  refused?: 'long' | 'shell' | 'empty' | 'blocked' | 'inline' | 'secret'
}

/** What a command raiox ran for the AI did. */
export interface Ran {
  command: string
  code: number | null
  output: string
  cut: boolean
  timed_out: boolean
  stopped: boolean
  duration_ms: number
}

export interface DraftFile {
  path: string
  status: 'modified' | 'added' | 'deleted'
  added: number
  removed: number
  diff: string
  binary: boolean
  truncated: boolean
}

export interface BotMessage {
  ms: number
  /** The person, the bot, or one of the house agents (Raio, Dieta, Memo). */
  author: 'you' | 'bot' | AgentAuthor
  kind: 'ask' | 'answer' | 'approve' | 'failed' | 'reset' | 'sent' | 'stopped' | 'house' | 'draft' | 'draft_applied' | 'draft_discarded' | 'draft_undone' | 'ran' | 'remembered' | 'review' | 'review_asked' | 'pass' | 'carried' | 'shot'
  data: {
    text?: string
    proposal?: string
    mode?: 'talk' | 'edit'
    reply?: BotReply
    error?: string
    /** An ask that came from the ai-memory inbox. */
    via?: 'ai-memory'
    body?: string
    /** A suggestion sent on: which answer and which project. */
    answer_ms?: number
    project?: string
    /** A draft: its id, the changed files and the secrets files kept out. */
    draft?: string
    files?: DraftFile[]
    blocked?: string[]
    /** What applying, undoing or discarding a draft touched. */
    paths?: string[]
    /** A command raiox ran, and whether the person had allowed it for the project. */
    ran?: Ran
    allowed?: boolean
    /** Where an answer was kept in ai-memory. */
    path?: string
    /** In a channel: who a message went to, and a pass from one member to another. */
    to?: string[] | string
    from?: string
    index?: number
    channel?: string
    /** Context brought from another conversation: its exchanges and where it came from. */
    items?: { who: 'you' | 'bot'; text: string }[]
    title?: string
    /** A screenshot the AI asked for: what it asked, where it was taken, and the file. */
    target?: string
    place?: string
    name?: string
    /** A review: the reviewing bot and its CLI. */
    reviewer?: string
    agent?: BotAgent
    /** A failed review: the answer it was about. */
    review?: number
  }
}

export interface Persona {
  preset: string
  name: string
  instructions: string
  color: string
  shape: string
  finish: Finish
  /** Its role in the person's team. */
  role?: string
}

export type Finish = 'solid' | 'flat' | 'pixel'

export interface Bot {
  id: string
  agent: BotAgent
  persona: Persona | null
  model: string | null
  effort: string | null
  /** Exact commands this bot may run without asking. */
  allowed?: string[]
  /** A lighter model for talk; changes run on `model`. */
  talk_model?: string | null
  /** The project's week, for Raio's report and tips. */
  week?: Week | null
  /** Another bot of the project that reviews this one's proposals. */
  reviewer?: string | null
  /** The project's daily token limit for its bots and what they used today. */
  budget?: { limit: number | null; used: number; choices: number[] }
  name: string
  dir: string
  has_session: boolean
  busy: boolean
  /** The conversation the running turn belongs to. */
  busy_thread: string | null
  /** What the running turn is doing now (read, search, run…). */
  step: BotStep | null
  /** The running turn's path so far. */
  trail?: Trail | null
  /** When the running turn started. */
  started_ms?: number | null
  messages: BotMessage[]
  /** Parallel conversations besides the main one. */
  threads: BotThread[]
  pins: { thread: string; ms: number }[]
  routines: Routine[]
}

export interface BotCandidate {
  project: string
  name: string
  agent: BotAgent
  requests: number
  last_ms: number
}

export interface BotsState {
  bots: Bot[]
  candidates: BotCandidate[]
  /** The CLIs the person turned on that can run bots. */
  agents: { id: BotAgent; name: string }[]
  /** Wired project pairs: only these can message each other. */
  links: [string, string][]
  batches: Batch[]
  channels: Channel[]
  max_text: number
}

/** One item of a channel: the person's message or pass (no bot), or a member's message in its channel conversation. */
export interface ChannelItem {
  bot: string | null
  thread?: string
  message: BotMessage
}

export interface Channel {
  id: string
  name: string
  /** The first member answers when a message mentions nobody. */
  members: { bot: string; handle: string }[]
  /** Each member's conversation for this channel. */
  threads: Record<string, string>
  messages: ChannelItem[]
  created_ms: number
}

export type BotAction = 'activate' | 'deactivate' | 'ask' | 'approve' | 'reset' | 'configure' | 'send' | 'handle' | 'stop' | 'thread_new' | 'thread_delete' | 'pin' | 'routine_new' | 'routine_toggle' | 'routine_delete' | 'link_toggle' | 'ask_many' | 'batch_delete' | 'house' | 'draft' | 'run' | 'disallow' | 'remember' | 'talk_model' | 'limit' | 'review' | 'reviewer' | 'memory_forget' | 'routine_changed' | 'correct' | 'channel_new' | 'channel_delete' | 'channel_post' | 'channel_pass' | 'carry' | 'preview_start' | 'preview_stop'

export interface InboxMessage {
  id: string
  subject: string | null
  body: string
  created_at: string
}

export interface Inbox {
  known: boolean
  messages: InboxMessage[]
}

export interface Connection {
  id: string
  name: string
  installed: boolean
  version: string | null
  logged_in: boolean | null
  method: string | null
  plan: string | null
  sessions: number
  requests: number
  install: string
  login: string
  bots: boolean
  xray: boolean
  enabled: boolean
}

export interface SettingsState {
  connections: Connection[]
  defaults: Record<string, { model: string | null; effort: string | null }>
}

export interface UsageRow {
  name: string
  tokens: number
  requests: number
}

/** What a bot's project ran on lately and what the panel can set. */
export interface BotUsage {
  days: number
  project: string
  models: UsageRow[]
  efforts: UsageRow[]
  default: { model: string | null; effort: string | null }
  offered_models: string[]
  offered_efforts: string[]
}

export interface LocalProject {
  project: string
  name: string
  github: string | null
  claude: number
  codex: number
  last_ms: number | null
}

export interface SetupState {
  done: boolean
  ais: Connection[]
  clone_to: string
  clones: { running: string[]; failed: Record<string, string> }
  projects: LocalProject[]
}

export interface GitHubRepo {
  repo: string
  name: string
  private: boolean
  pushed_at: string
  description: string
}

export interface GitHubState {
  installed: boolean
  logged_in: boolean
  login: string | null
  repos: GitHubRepo[]
  install: string
  connect: string
}

const TOKEN_KEY = 'raiox.session'
const DEVICE_KEY = 'raiox.aparelho'
/** Whether the panel was opened through the phone access (the tunnel's public address), not on this machine. */
export const IS_REMOTE = !['127.0.0.1', 'localhost', '[::1]'].includes(window.location.hostname)
/** The error a request gives when this device is not paired, or was removed. */
export const UNPAIRED = 'unpaired'

/** Reads the session token from the URL fragment once, then hides it from the address bar. */
export function sessionToken(): string | null {
  if (IS_REMOTE) return localStorage.getItem(DEVICE_KEY)
  const fromHash = new URLSearchParams(window.location.hash.slice(1)).get('t')
  if (fromHash) {
    sessionStorage.setItem(TOKEN_KEY, fromHash)
    history.replaceState(null, '', window.location.pathname)
    return fromHash
  }
  return sessionStorage.getItem(TOKEN_KEY)
}

/** The pairing code a QR link brought, if any. */
export function pairingCode(): string | null {
  return new URLSearchParams(window.location.hash.slice(1)).get('pair')
}

async function readJson<T>(response: Response): Promise<T & { error?: string }> {
  const raw = await response.text()
  try {
    return JSON.parse(raw) as T & { error?: string }
  } catch {
    return { error: raw } as T & { error?: string }
  }
}

/** A device the person removed (or never paired) forgets its token and asks to pair. */
function unpairedIf(response: Response): void {
  if (IS_REMOTE && response.status === 401) {
    localStorage.removeItem(DEVICE_KEY)
    throw new Error(UNPAIRED)
  }
}

/** Trades the QR link's code for this device's own token, kept on the device. */
export async function pairDevice(code: string): Promise<void> {
  history.replaceState(null, '', window.location.pathname)
  const response = await fetch('/api/pair', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code }) })
  const data = await readJson<{ token?: string }>(response)
  if (!response.ok || !data.token) throw new Error(data.error ?? `${response.status}`)
  localStorage.setItem(DEVICE_KEY, data.token)
}

export async function fetchReport(token: string): Promise<Report> {
  const response = await fetch('/api/report', { headers: { Authorization: `Bearer ${token}` } })
  unpairedIf(response)
  if (!response.ok) {
    throw new Error(`${response.status} ${await response.text()}`)
  }
  return (await response.json()) as Report
}

async function request<T>(path: string, body?: Record<string, unknown>): Promise<T> {
  const token = sessionToken()
  if (!token) throw new Error('no session token')
  const response = await fetch(path, {
    method: body ? 'POST' : 'GET',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  })
  unpairedIf(response)
  const data = await readJson<T>(response)
  if (!response.ok) throw new Error(data.error ?? `${response.status}`)
  return data
}

export function fetchSetup(): Promise<SetupState> {
  return request<SetupState>('/api/setup')
}

export function fetchGitHub(): Promise<GitHubState> {
  return request<GitHubState>('/api/setup/github')
}

export function cloneRepo(repo: string): Promise<SetupState> {
  return request<SetupState>('/api/setup/clone', { repo })
}

export function finishSetup(): Promise<{ done: boolean }> {
  return request<{ done: boolean }>('/api/setup/finish', {})
}

export function botUsage(id: string): Promise<BotUsage> {
  return request<BotUsage>('/api/bot/usage', { id })
}

export function botInbox(id: string): Promise<Inbox> {
  return request<Inbox>('/api/bot/inbox', { id })
}

export interface MemoryPage {
  title: string | null
  path: string
  kind: string | null
  updated_at: string | null
}

export interface Memories {
  known: boolean
  count?: number
  pages: MemoryPage[]
  web?: string
}

/** Dieta's cuts for one bot's project: its own numbers, or all projects' when it has few requests. */
export interface ProjectDiet {
  suggestions: Suggestion[]
  compaction: CompactionModel
  scope: { own: boolean; calls: number; minimum: number }
}

export function botDiet(id: string): Promise<ProjectDiet> {
  return request<ProjectDiet>('/api/bot/diet', { id })
}

/** A project's app as raiox runs it: the suggested dev command, and whether it runs, where, and its last lines. */
export interface Preview {
  suggestion: string | null
  running: boolean
  command: string | null
  started_ms: number | null
  exit_code: number | null
  url: string | null
  log: string
  /** A booted iOS simulator device, if any. */
  simulator: string | null
}

export function botPreview(id: string): Promise<Preview> {
  return request<Preview>('/api/bot/preview', { id })
}

/** A screenshot of a local page or of the simulator, as a data URL, with its file name. */
export function botPreviewShot(id: string, target: 'page' | 'simulator', url: string, width: number, height: number): Promise<{ image: string; name: string }> {
  return request('/api/bot/preview_shot', { id, target, url, width, height })
}

export function botShotImage(id: string, name: string): Promise<{ image: string }> {
  return request('/api/bot/shot_image', { id, name })
}

/** The phone access: Cloudflare Tunnel settings, as the person left them. */
export interface TunnelConfig {
  on: boolean
  mode: 'quick' | 'named'
  tunnel: string
  hostname: string
  team: string
  aud: string
}

export interface PairedDevice {
  id: string
  name: string
  created_ms: number
  seen_ms: number
}

export interface RemoteState {
  tunnel: {
    installed: boolean
    config: TunnelConfig
    running: boolean
    started_ms: number | null
    exit_code: number | null
    /** The public address, once cloudflared gave it. */
    host: string | null
    log: string
  }
  devices: PairedDevice[]
}

/** A one-time pairing link and its QR code (an SVG data URL). */
export interface Pairing {
  url: string
  qr: string
  expires_ms: number
}

export function fetchRemote(): Promise<RemoteState> {
  return request<RemoteState>('/api/remote')
}

export function setTunnel(config: TunnelConfig): Promise<RemoteState> {
  return request<RemoteState>('/api/remote/tunnel', { ...config })
}

export function newPairing(): Promise<Pairing> {
  return request<Pairing>('/api/remote/pair', {})
}

export function forgetDevice(id: string): Promise<RemoteState> {
  return request<RemoteState>('/api/remote/forget', { id })
}

export function botMemories(id: string): Promise<Memories> {
  return request<Memories>('/api/bot/memories', { id })
}

export function fetchSettings(): Promise<SettingsState> {
  return request<SettingsState>('/api/settings')
}

export function saveSettings(change: Record<string, unknown>): Promise<SettingsState> {
  return request<SettingsState>('/api/settings', change)
}

export function fetchBots(): Promise<BotsState> {
  return request<BotsState>('/api/bots')
}

/** Every bot action answers with the bots' state, so the panel redraws at once. */
export function botAction(action: BotAction, body: Record<string, unknown>): Promise<BotsState> {
  return request<BotsState>(`/api/bot/${action}`, body)
}

async function post(path: string, body: Record<string, string>): Promise<{ error?: string; memo?: { kept?: string; error?: string } }> {
  const token = sessionToken()
  if (!token) throw new Error('no session token')
  const response = await fetch(path, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const data = (await response.json()) as { error?: string; memo?: { kept?: string; error?: string } }
  if (!response.ok) throw new Error(data.error ?? `${response.status}`)
  return data
}

/** A change goes to the bot's project when `bot` is given, else to the project `raiox open` started in. */
export function applySuggestion(id: string, source: Source, bot?: string): ReturnType<typeof post> {
  return post('/api/apply', bot ? { id, source, bot } : { id, source })
}

/** The session working in a project now: the bot's project, else the one `raiox open` started in; null when it has none. */
export function fetchSession(bot?: string): Promise<ContextStatus | null> {
  return request<ContextStatus | null>('/api/session', bot ? { bot } : {})
}

export function undoChange(change: string, bot?: string): ReturnType<typeof post> {
  return post('/api/undo', bot ? { change, bot } : { change })
}

