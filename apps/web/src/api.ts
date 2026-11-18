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
  room: { opted_in: boolean; messages: RoomMessage[] }
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

export type BotAgent = 'claude' | 'codex'

export interface BotReply {
  text: string
  proposal: string | null
  read_tokens: number
  output_tokens: number
  cost_usd: number | null
  duration_ms: number
}

export interface BotMessage {
  ms: number
  author: 'you' | 'bot'
  kind: 'ask' | 'answer' | 'approve' | 'failed' | 'reset'
  data: { text?: string; proposal?: string; mode?: 'talk' | 'edit'; reply?: BotReply; error?: string }
}

export interface Persona {
  preset: string
  name: string
  instructions: string
  color: string
  shape: string
}

export interface Bot {
  id: string
  agent: BotAgent
  persona: Persona | null
  name: string
  dir: string
  has_session: boolean
  busy: boolean
  messages: BotMessage[]
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
  max_text: number
}

export type BotAction = 'activate' | 'deactivate' | 'ask' | 'approve' | 'reset'

export interface AiStatus {
  id: BotAgent
  name: string
  installed: boolean
  version: string | null
  logged_in: boolean
  method: string | null
  plan: string | null
  sessions: number
  requests: number
  install: string
  login: string
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
  ais: AiStatus[]
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

export interface RoomMessage {
  ms: number
  author: AgentAuthor | 'you'
  kind: 'ask' | 'answer' | 'applied' | 'undone' | 'kept' | 'kept_failed'
  data: Record<string, unknown>
}

const TOKEN_KEY = 'raiox.session'

/** Reads the session token from the URL fragment once, then hides it from the address bar. */
export function sessionToken(): string | null {
  const fromHash = new URLSearchParams(window.location.hash.slice(1)).get('t')
  if (fromHash) {
    sessionStorage.setItem(TOKEN_KEY, fromHash)
    history.replaceState(null, '', window.location.pathname)
    return fromHash
  }
  return sessionStorage.getItem(TOKEN_KEY)
}

export async function fetchReport(token: string): Promise<Report> {
  const response = await fetch('/api/report', { headers: { Authorization: `Bearer ${token}` } })
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
  const data = (await response.json()) as T & { error?: string }
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

export function applySuggestion(id: string, source: Source): ReturnType<typeof post> {
  return post('/api/apply', { id, source })
}

export function undoChange(change: string): ReturnType<typeof post> {
  return post('/api/undo', { change })
}

export function askAgent(agent: AgentAuthor, text?: string): ReturnType<typeof post> {
  return post('/api/ask', text ? { agent, text } : { agent })
}
