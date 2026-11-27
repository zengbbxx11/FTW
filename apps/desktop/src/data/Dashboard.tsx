import { Select } from '../ui/Select'
import { useState, type CSSProperties } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowDown, ArrowRight, ArrowUp, Bot, Check, ClipboardCheck, Clock, FileText, History, Inbox, Mail,
  MessageSquare, Package, Plus, RotateCcw, Send, SlidersHorizontal, TriangleAlert, Upload, UsersRound, X } from 'lucide-react'
import { activityTypes, dateLabel, sourceLabel } from '../crm/api'
import { Empty } from '../crm/components'
import { IconButton } from '../ui/WorkbenchUI'
import { DataError } from './DataTools'
import { send, type DashboardData } from './api'

export type DashboardLink = 'customers' | 'sent' | 'waiting' | 'replied' | 'mail' | 'activities' | 'agent' | 'analytics' | 'new' | 'csv' | 'review' | 'approvals' | 'sync'
type Widget = 'metrics' | 'recent' | 'progress' | 'trend' | 'agent' | 'quick'
type Period = 7 | 14 | 30
type Layout = { order: Widget[]; hidden: Widget[]; compact: boolean; period: Period }

const layoutKey = 'ftw.dashboard.layout'
const widgetLabels: Record<Widget, string> = {
  metrics: '关键统计', recent: '最近业务动态', progress: '邮件开发进展', trend: '邮件提交趋势', agent: '最近 Agent 操作', quick: '快捷操作',
}
const defaults: Layout = { order: ['metrics', 'recent', 'progress', 'trend', 'agent', 'quick'], hidden: [], compact: false, period: 14 }
const periods = [7, 14, 30] as const

function readLayout(): Layout {
  try {
    const stored: Partial<Layout> = JSON.parse(localStorage.getItem(layoutKey) ?? '{}') ?? {}
    const order = Array.isArray(stored.order) ? stored.order.filter(key => Object.hasOwn(widgetLabels, key)) : []
    return { order: [...new Set([...order, ...defaults.order])],
      hidden: Array.isArray(stored.hidden) ? stored.hidden.filter(key => Object.hasOwn(widgetLabels, key)) : [],
      compact: stored.compact === true, period: stored.period && periods.includes(stored.period) ? stored.period : defaults.period }
  } catch { return defaults }
}

const eventIcons = { note: FileText, chat: MessageSquare, sample: Package, email_sent: Send, email_received: Mail }
const agentEntities: Record<string, string> = { contact: '客户', lead: '候选客户', activity: '跟进记录', email_draft: '邮件草稿',
  email_outbox_job: '邮件队列', customer_field: '客户字段', contact_field_values: '自定义属性' }
const agentOperations: Record<string, string> = { create: '新建', update: '更新', patch: '补充', queue: '入队',
  enqueue: '入队', edit: '编辑', request_approval: '提交审核' }

export function Dashboard({ navigate, openActivity }: { navigate: (link: DashboardLink) => void; openActivity: (id: string) => void }) {
  const [layout, setLayout] = useState(readLayout)
  const [editing, setEditing] = useState<Layout | null>(null)
  const [period, setPeriod] = useState<Period>(layout.period)
  const [storageError, setStorageError] = useState(false)
  const query = useQuery({ queryKey: ['dashboard'], queryFn: () => send({ action: 'dashboard' }, 'dashboard') })
  const current = editing ?? layout
  function saveLayout() {
    if (!editing) return
    try {
      localStorage.setItem(layoutKey, JSON.stringify(editing))
      setLayout(editing)
      setPeriod(editing.period)
      setEditing(null)
      setStorageError(false)
    } catch { setStorageError(true) }
  }
  function moveWidget(widget: Widget, direction: -1 | 1) {
    if (!editing) return
    const order = editing.order.slice()
    const index = order.indexOf(widget)
    const next = index + direction
    if (next < 0 || next >= order.length) return
    order[index] = order[next]
    order[next] = widget
    setEditing({ ...editing, order })
  }
  const data = query.data
  function panel(widget: Widget) {
    if (!data) return null
    switch (widget) {
      case 'metrics': return <div className="dashboard-metrics" aria-label={widgetLabels.metrics}>
        <Metric icon={UsersRound} tone="teal" value={data.stats.contacts} label="CRM 客户" onClick={() => navigate('customers')} />
        <Metric icon={Send} tone="purple" value={data.stats.smtp_accepted} label="SMTP 已接受" onClick={() => navigate('sent')} />
        <Metric icon={Clock} tone="orange" value={data.stats.waiting} label="已发未见回复客户" onClick={() => navigate('waiting')} />
        <Metric icon={Mail} tone="green" value={data.stats.replied} label="已发且有回复客户" onClick={() => navigate('replied')} />
      </div>
      case 'recent': return <section className="panel dashboard-panel">
        <PanelHeading label={widgetLabels.recent} action="查看全部" onClick={() => navigate('activities')} />
        {data.recent.length ? <ul className="dashboard-events">{data.recent.slice(0, current.compact ? 4 : 5).map(event => {
          const Icon = eventIcons[event.event_type as keyof typeof eventIcons] ?? History
          return <li key={event.id}><button type="button" onClick={() => openActivity(event.id)}>
            <span className="dashboard-symbol" data-tone={event.event_type === 'email_received' ? 'purple' : event.event_type === 'email_sent' ? 'orange' : 'teal'}><Icon aria-hidden="true" /></span>
            <span className="dashboard-event-text"><strong>{event.title}</strong>
              <small>{event.detail} · {activityTypes[event.event_type] ?? event.event_type} · {sourceLabel(event.source)}</small></span>
            <time>{dateLabel(event.occurred_at)}</time>
          </button></li>
        })}</ul> : <Empty>尚无客户活动。</Empty>}
      </section>
      case 'progress': return <Progress data={data} open={() => navigate('analytics')} />
      case 'trend': return <Trend data={data} period={editing?.period ?? period} setPeriod={setPeriod} editing={Boolean(editing)} />
      case 'agent': return <section className="panel dashboard-panel">
        <PanelHeading label={widgetLabels.agent} action="任务中心" onClick={() => navigate('agent')} />
        {data.agent.length ? <ul className="dashboard-agent">{data.agent.slice(0, current.compact ? 2 : 3).map((event, index) =>
          <li key={`${event.entity_id}:${event.occurred_at}:${index}`}><Bot aria-hidden="true" /><div>
            <strong>{agentOperations[event.operation] ?? event.operation} · {agentEntities[event.entity] ?? event.entity}</strong>
            <small>{event.actor} · {dateLabel(event.occurred_at)}</small></div></li>)}</ul> : <Empty>尚无 Agent 操作记录。</Empty>}
      </section>
      case 'quick': return <section className="panel dashboard-panel">
        <PanelHeading label={widgetLabels.quick} />
        <div className="dashboard-quick">
          <button type="button" className="button" onClick={() => navigate('new')}><Plus aria-hidden="true" />新增客户</button>
          <button type="button" className="button" onClick={() => navigate('csv')}><Upload aria-hidden="true" />导入客户 CSV</button>
          <button type="button" className="button" onClick={() => navigate('mail')}><Inbox aria-hidden="true" />打开收件箱</button>
        </div>
      </section>
    }
  }

  const visible = current.order.filter(widget => !current.hidden.includes(widget))
  const defaultArrangement = visible.length === defaults.order.length && visible.every((widget, index) => widget === defaults.order[index])
  const renderWidget = (widget: Widget) => <div key={widget} className="dashboard-widget" data-widget={widget}>{panel(widget)}</div>
  return <div className="dashboard-workspace" data-density={current.compact ? 'compact' : 'standard'}>
    <header className="page-heading">
      <div><h1>{editing ? '自定义工作台布局' : '工作台'}</h1><p>{editing ? '调整卡片的显示、顺序和默认邮件趋势范围' : '客户开发、邮件待办与近期业务动态'}</p></div>
      <div className="row-actions">{editing ? <>
        <button type="button" className="button" onClick={() => { setEditing(null); setStorageError(false) }}><X aria-hidden="true" />取消</button>
        <button type="button" className="button" onClick={() => setEditing(defaults)}><RotateCcw aria-hidden="true" />恢复默认布局</button>
        <button type="button" className="button primary" onClick={saveLayout}><Check aria-hidden="true" />保存布局</button>
      </> : <>
        <button type="button" className="button" onClick={() => setEditing(layout)}><SlidersHorizontal aria-hidden="true" />自定义布局</button>
        <button type="button" className="button primary" onClick={() => navigate('new')}><Plus aria-hidden="true" />新增客户</button>
      </>}</div>
    </header>
    {query.isPending && <Empty>正在读取工作台…</Empty>}
    {query.isError && <DataError error={query.error} />}
    {data && <>
      {!editing && <div className="dashboard-attention" aria-label="待办和同步异常">
        <button type="button" className="button" onClick={() => navigate('approvals')}><ClipboardCheck aria-hidden="true" />待审核邮件 <strong>{data.pending}</strong></button>
        <button type="button" className="button" onClick={() => navigate('review')}><Inbox aria-hidden="true" />客户来信待核对 <strong>{data.unmatched}</strong></button>
        {data.issues.length > 0 && <button type="button" className="button" onClick={() => navigate('sync')}><TriangleAlert aria-hidden="true" />邮箱同步异常 <strong>{data.issues.length}</strong></button>}
      </div>}
      <div className={editing ? 'dashboard-settings' : undefined}>
        <div className="dashboard-widgets" data-editing={Boolean(editing)} aria-label="工作台卡片" inert={editing !== null}>
          {defaultArrangement ? <>
            {renderWidget('metrics')}
            <div className="dashboard-column">{renderWidget('recent')}{renderWidget('trend')}</div>
            <div className="dashboard-column">{renderWidget('progress')}{renderWidget('agent')}{renderWidget('quick')}</div>
          </> : visible.map(renderWidget)}
          {visible.length === 0 && <section className="panel"><Empty>尚未显示卡片。可在布局设置中重新启用。</Empty></section>}
        </div>
        {editing && <aside className="panel dashboard-config" aria-label="组件设置">
          <h2>组件设置</h2>
          <ul>{editing.order.map((widget, index) => <li key={widget}>
            <label><input type="checkbox" checked={!editing.hidden.includes(widget)} onChange={({ target: { checked } }) => setEditing(draft => draft && { ...draft,
              hidden: checked ? draft.hidden.filter(item => item !== widget) : [...draft.hidden, widget] })} />{widgetLabels[widget]}</label>
            <div className="row-actions">
              <IconButton label={`上移${widgetLabels[widget]}`} disabled={index === 0} onClick={() => moveWidget(widget, -1)}><ArrowUp aria-hidden="true" /></IconButton>
              <IconButton label={`下移${widgetLabels[widget]}`} disabled={index === editing.order.length - 1} onClick={() => moveWidget(widget, 1)}><ArrowDown aria-hidden="true" /></IconButton>
            </div>
          </li>)}</ul>
          <label className="field">显示密度<Select value={editing.compact ? 'compact' : 'standard'}
            onChange={({ target: { value } }) => setEditing(draft => draft && { ...draft, compact: value === 'compact' })}>
            <option value="standard">标准</option><option value="compact">紧凑</option></Select></label>
          <label className="field">默认时间范围<Select value={editing.period}
            onChange={({ target: { value } }) => setEditing(draft => draft && { ...draft, period: Number(value) as Period })}>
            {periods.map(days => <option key={days} value={days}>最近 {days} 天</option>)}</Select></label>
          {storageError && <p role="alert" className="alert error">无法保存本地布局，请检查工作台数据目录。</p>}
        </aside>}
      </div>
    </>}
  </div>
}

function Metric({ icon: Icon, tone, value, label, onClick }: { icon: typeof UsersRound; tone: string; value: number; label: string; onClick: () => void }) {
  return <button type="button" className="panel dashboard-metric" onClick={onClick}>
    <span className="dashboard-symbol" data-tone={tone}><Icon aria-hidden="true" /></span>
    <span><strong>{value.toLocaleString('zh-CN')}</strong><small>{label}</small></span>
    <ArrowRight className="dashboard-card-arrow" aria-hidden="true" />
  </button>
}

function PanelHeading({ label, action, onClick }: { label: string; action?: string; onClick?: () => void }) {
  return <div className="section-heading"><h2>{label}</h2>{action && <button type="button" className="button" onClick={onClick}><ArrowRight aria-hidden="true" />{action}</button>}</div>
}

function Progress({ data, open }: { data: DashboardData; open: () => void }) {
  const { sent_contacts: sent, replied, waiting } = data.stats
  const first = sent ? 100 * replied / sent : 0
  const second = sent ? 100 * (replied + waiting) / sent : 0
  return <section className="panel dashboard-panel">
    <PanelHeading label="邮件开发进展" action="效果分析" onClick={open} />
    <div className="dashboard-progress">
      <div className="dashboard-ring" data-empty={!sent} style={{ '--replied': `${first}%`, '--waiting': `${second}%` } as CSSProperties}>
        <span><strong>{sent.toLocaleString('zh-CN')}</strong><small>已发客户</small></span>
      </div>
      <dl className="dashboard-legend">
        <div data-tone="teal"><dt>已确认有回复</dt><dd>{replied}</dd></div>
        <div data-tone="purple"><dt>已发未见回复</dt><dd>{waiting}</dd></div>
        <div data-tone="orange"><dt>待同步及其他</dt><dd>{sent - waiting - replied}</dd></div>
      </dl>
    </div>
  </section>
}

function Trend({ data, period, setPeriod, editing }: { data: DashboardData; period: Period; setPeriod: (value: Period) => void; editing: boolean }) {
  const [table, setTable] = useState(false)
  const series = data.trend.slice(-period)
  const ceiling = Math.max(1, ...series.map(row => row.accepted))
  return <section className="panel dashboard-panel">
    <div className="section-heading"><h2>邮件提交趋势</h2><div className="row-actions">
      <button type="button" className="button" aria-pressed={table} onClick={() => setTable(!table)}>{table ? '查看图表' : '查看数据'}</button>
      <Select aria-label="邮件提交趋势时间范围" value={period} disabled={editing} onChange={event => setPeriod(Number(event.target.value) as Period)}>
        {periods.map(days => <option key={days} value={days}>最近 {days} 天</option>)}</Select>
    </div></div>
    {table ? <div className="dashboard-trend-table table-wrap"><table aria-label="每日 SMTP 接受量">
      <thead><tr><th>日期</th><th>SMTP 已接受</th></tr></thead>
      <tbody>{series.map(row => <tr key={row.day}><td>{row.day}</td><td>{row.accepted}</td></tr>)}</tbody>
    </table></div> : <>
      <div className="dashboard-bars" role="img" aria-label={`最近 ${period} 天，SMTP 已接受 ${series.reduce((sum, row) => sum + row.accepted, 0)} 封`}>
        {series.map(row => <span key={row.day} title={`${row.day} · ${row.accepted} 封`} style={{ '--bar': `${100 * row.accepted / ceiling}%` } as CSSProperties} />)}
      </div>
      <div className="dashboard-axis"><span>{series[0]?.day.slice(5)}</span><span>{series.at(-1)?.day.slice(5)}</span></div>
    </>}
  </section>
}
