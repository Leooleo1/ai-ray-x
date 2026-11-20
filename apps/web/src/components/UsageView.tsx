import { Skeleton } from './Skeleton'
import { useEffect, useMemo, useState } from 'react'
import { type Usage, type UsageSlice, fetchUsage } from '../api'
import { AGENT_LABEL, DEFAULT_COLOR } from '../avatars'
import { compact, locale, percent, t } from '../i18n'
import { AgentLogo } from './AgentLogo'
import { Segmented } from './Segmented'
import { CostChart } from './usage/CostChart'
import { money } from './usage/money'
import { UsageTable } from './usage/UsageTable'
import './usage/uso.css'

type Ai = 'all' | 'claude' | 'codex'
type Period = '7' | '30' | '90'
type Detail = 'model' | 'ai' | 'project' | 'day'

const SOURCES = ['claude', 'codex'] as const
const colorOf = (source: string | null | undefined) => (source === 'claude' || source === 'codex' ? DEFAULT_COLOR[source] : undefined)
const labelOf = (source: string) => (source === 'claude' || source === 'codex' ? AGENT_LABEL[source] : source)

const dayName = (iso: string) =>
  new Date(`${iso}T12:00:00`).toLocaleDateString(locale, { weekday: 'short', day: 'numeric', month: 'short' }).replace(/\./g, '')

/** The rows of the breakdown the person picked. */
function detailRows(usage: Usage, detail: Detail): UsageSlice[] {
  if (detail === 'model') return usage.by_model
  if (detail === 'project') return usage.by_project
  if (detail === 'ai') return usage.by_source.map((row) => ({ ...row, label: labelOf(row.key) }))
  return [...usage.daily].reverse().map((day) => ({
    key: day.date,
    label: dayName(day.date),
    source: null,
    usd: Object.values(day.usd).reduce((sum, v) => sum + v, 0),
    tokens: day.tokens,
    calls: 0,
  }))
}

/** What the AIs' use would cost at API prices, per day, per AI, per model and per project. */
export function UsageView() {
  const [ai, setAi] = useState<Ai>('all')
  const [period, setPeriod] = useState<Period>('30')
  const [detail, setDetail] = useState<Detail>('model')
  const [usage, setUsage] = useState<Usage | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    fetchUsage(Number(period), ai === 'all' ? undefined : ai)
      .then((next) => {
        if (!alive) return
        setUsage(next)
        setError(null)
      })
      .catch((failure: unknown) => alive && setError(failure instanceof Error ? failure.message : String(failure)))
    return () => {
      alive = false
    }
  }, [ai, period])
  const series = useMemo(() => SOURCES.filter((s) => ai === 'all' || ai === s).map((key) => ({ key, color: DEFAULT_COLOR[key] })), [ai])
  const controls = (
    <div className="uso-controles">
      <Segmented
        label={t.usageAi}
        value={ai}
        onChange={setAi}
        options={[
          { id: 'all', label: t.usageAllAis },
          { id: 'claude', label: AGENT_LABEL.claude },
          { id: 'codex', label: AGENT_LABEL.codex },
        ]}
      />
      <Segmented
        label={t.usagePeriod}
        value={period}
        onChange={setPeriod}
        options={(['7', '30', '90'] as const).map((id) => ({ id, label: t.usageDays.replace('{n}', id) }))}
      />
    </div>
  )
  if (!usage) {
    return (
      <main className="uso">
        {controls}
        {error ? <p className="nota failed quebra">{error}</p> : <Skeleton shape="usage" rows={5} />}
      </main>
    )
  }
  const totals = [
    { label: t.usageFresh, value: usage.totals.input },
    { label: t.usageRead, value: usage.totals.cache_read },
    { label: t.usageWrite, value: usage.totals.cache_write },
    { label: t.usageOutput, value: usage.totals.output },
  ]
  return (
    <main className="uso">
      {controls}
      <section className="uso-topo">
        <div className="uso-resumo">
          <span className="uso-total">{money(usage.usd)}</span>
          <span className="uso-meta">{t.usageMeta.replace('{n}', usage.sessions.toLocaleString(locale))}</span>
          <ul className="uso-ias">
            {usage.by_source.map((row) => (
              <li key={row.key}>
                <span className="uso-ia-nome">
                  <AgentLogo agent={row.key} />
                  {labelOf(row.key)}
                </span>
                <b>{money(row.usd ?? 0)}</b>
                <small>
                  {usage.usd > 0 ? percent((row.usd ?? 0) / usage.usd) : '—'} · {compact(row.tokens)} tokens
                </small>
              </li>
            ))}
          </ul>
          {usage.unpriced_calls > 0 && <small className="uso-nota">{t.usageUnpriced.replace('{n}', usage.unpriced_calls.toLocaleString(locale))}</small>}
        </div>
        <CostChart days={usage.daily} series={series} />
      </section>
      <section className="uso-totais">
        {totals.map((item) => (
          <div key={item.label}>
            <span>{item.label}</span>
            <b>{compact(item.value)}</b>
          </div>
        ))}
      </section>
      <section className="uso-detalhe">
        <header>
          <h2>{t.usageDetail}</h2>
          <Segmented
            label={t.usageDetail}
            value={detail}
            onChange={setDetail}
            options={[
              { id: 'model', label: t.usageByModel },
              { id: 'ai', label: t.usageByAi },
              { id: 'project', label: t.usageByProject },
              { id: 'day', label: t.usageByDay },
            ]}
          />
        </header>
        <UsageTable
          name={{ model: t.usageByModel, ai: t.usageByAi, project: t.usageByProject, day: t.usageByDay }[detail]}
          rows={detailRows(usage, detail)}
          total={usage.usd}
          colorOf={(row) => colorOf(row.source)}
        />
      </section>
    </main>
  )
}
