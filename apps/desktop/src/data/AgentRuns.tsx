import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight, Bot, CircleCheck, ClipboardList, Clock, RotateCcw, Search, Send, ShieldCheck, Square, TriangleAlert, UserRound } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { ConfirmButton, Empty, Pagination } from '../crm/components'
import { mailError, mailPurposes, states } from '../mail/api'
import { DataError } from './DataTools'
import { OpsStat, OpsTag } from './OpsUI'
import { send, type AgentRun, type AgentRunFilter, type AgentRunRecord, type MailJobAction } from './api'

export type AgentRecordLink = 'contact' | 'lead' | 'activity' | 'draft'
type OpenRecord = (kind: AgentRecordLink, id: string) => void
const filters = { all: '全部记录', active: '队列处理中', recorded: '已有结果', attention: '需核对' } as const
const operations: Record<string, { label: string; link?: AgentRecordLink }> = {
  create_customer: { label: '新建客户', link: 'contact' }, update_customer: { label: '补充客户资料', link: 'contact' },
  update_customer_profile: { label: '更新客户档案', link: 'contact' },
  create_customer_profile: { label: '新建完整客户档案', link: 'contact' },
  create_lead: { label: '登记候选客户', link: 'lead' }, add_activity: { label: '添加跟进记录', link: 'activity' },
  update_lead_candidate: { label: '补充候选客户资料', link: 'lead' },
  save_company_profile: { label: '保存公司档案' },
  set_company_membership: { label: '更改公司归属', link: 'contact' },
  associate_company_contact: { label: '关联公司联系人', link: 'contact' },
  create_company_contact: { label: '新增公司联系人', link: 'contact' },
  set_customer_field_archived: { label: '归档或恢复自定义字段' },
  move_customer_field: { label: '调整自定义字段顺序' },
  move_customer_view: { label: '调整共享视图顺序' },
  update_customer_batch: { label: '批量更新客户' },
  review_lead_candidate: { label: '审核候选客户', link: 'lead' },
  promote_lead_candidate: { label: '候选转正式客户', link: 'contact' },
  review_customer_evidence: { label: '核验调查证据' },
  confirm_reply_link: { label: '确认来信关联' },
  create_reply_draft: { label: '起草会话回复', link: 'draft' },
  suggest_customer_detail: { label: '提交客户资料建议', link: 'contact' }, add_lead_evidence: { label: '添加调查证据', link: 'lead' },
  add_contact_evidence: { label: '添加正式客户证据', link: 'contact' },
  patch_field_values: { label: '更新自定义属性', link: 'contact' }, create_email_draft: { label: '创建邮件草稿', link: 'draft' },
  save_email_template: { label: '保存邮件模板' }, set_email_template_archived: { label: '更改邮件模板归档状态' },
}
const requestResults: Record<string, string> = { uncertain: '结果未记录，请核对', error: '调用返回错误', duplicates: '检测到重复资料',
  agent_review_pending: '更新建议待人工确认', saved: '已保存', field_values: '已返回字段记录', template_saved: '已保存模板', lead_updated: '已更新候选客户',
  cells_updated: '已批量更新客户' }
const stateTone = { active: 'teal', recorded: 'green', attention: 'orange' } as const

function runKey(record: AgentRunRecord) {
  return record.run_type === 'mail' ? `mail:${record.agent_id}:${record.task_id}` : `request:${record.agent_id ?? ''}:${record.operation}:${record.request_id}`
}
function runTitle(record: AgentRunRecord) {
  return record.run_type === 'mail' ? record.task_id : operations[record.operation]?.label ?? record.operation
}

export function AgentRuns({ back, open, changes }: { back: () => void; open: OpenRecord; changes: () => void }) {
  const [filter, setFilter] = useState<AgentRunFilter>('all')
  const [input, setInput] = useState('')
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState('')
  const keyword = useDeferredValue(input.trim())
  const query = useQuery({ queryKey: ['agent-runs', filter, keyword, offset], refetchInterval: 5000,
    queryFn: () => send({ action: 'agent_runs', filter, keyword, offset }, 'agent_runs') })
  const data = query.data
  const selected = data?.rows.find(row => runKey(row.record) === selection) ?? data?.rows[0]
  return <div className="ops-workspace agent-runs">
    <header className="page-heading"><div><h1>Agent 开发任务中心</h1><p>查看 Agent 发信任务、MCP 写入结果和关联业务记录</p></div>
      <div className="row-actions"><button type="button" className="button" onClick={back}><ArrowLeft aria-hidden="true" />返回工作台</button>
        <button type="button" className="button" onClick={changes}><ShieldCheck aria-hidden="true" />查看 Agent 数据变更</button></div></header>
    {query.isError && <DataError error={query.error} />}
    {query.isPending && <Empty>正在读取 Agent 执行记录…</Empty>}
    {data && <div className="ops-metrics">
      <OpsStat icon={ClipboardList} tone="teal" name="关联执行记录" value={data.counts.all} />
      <OpsStat icon={Clock} tone="purple" name="队列处理中" value={data.counts.active} />
      <OpsStat icon={CircleCheck} tone="green" name="已有结果" value={data.counts.recorded} />
      <OpsStat icon={TriangleAlert} tone="orange" name="需核对" value={data.counts.attention} />
    </div>}
    <div className="ops-controls">
      <div className="row-actions global-search-tabs" role="group" aria-label="执行记录状态">
        {(Object.keys(filters) as AgentRunFilter[]).map(state => <button type="button" className="button" aria-pressed={filter === state} key={state}
          onClick={() => { setFilter(state); setOffset(0); setSelection('') }}>{filters[state]} {data && <span>{data.counts[state]}</span>}</button>)}
      </div>
      <label className="followup-search"><Search aria-hidden="true" /><input type="search" aria-label="查找 Agent 记录" placeholder="任务 ID、请求 ID 或 Agent 名称"
        maxLength={160} value={input} onChange={event => { setInput(event.target.value); setOffset(0); setSelection('') }} /></label>
    </div>
    {data && <div className="ops-split agent-run-grid" data-selected={Boolean(selected)}>
        <section className="panel ops-table" aria-label="Agent 执行记录">
          <div className="section-heading"><h2>任务与调用</h2></div>
          {data.rows.length ? <div className="table-wrap"><table><thead><tr><th>任务或操作</th><th>状态</th><th>邮件记录</th><th>时间</th></tr></thead>
            <tbody>{data.rows.map(row => {
              const record = row.record
              return <tr key={runKey(record)} data-selected={selected === row}><td><button type="button" className="text-button"
                aria-pressed={selected === row} onClick={() => setSelection(runKey(record))}>{runTitle(record)}</button>
                <small>{record.run_type === 'mail' ? `${record.agent_name} · 发信任务` : `${record.agent_name || 'MCP'} · ${record.request_id}`}</small></td>
                <td><OpsTag tone={stateTone[row.state]}>{filters[row.state]}</OpsTag></td>
                <td>{record.run_type === 'mail' ? `${record.accepted} 封已接受 / ${record.jobs} 条` : '—'}</td>
                <td>{dateLabel(row.occurred_at)}</td></tr>
            })}</tbody></table></div> : <Empty>当前没有符合条件的 Agent 执行记录。</Empty>}
          <Pagination offset={offset} more={data.counts[filter] > offset + 20} change={next => { setOffset(next); setSelection('') }} />
        </section>
        {selected && <RunDetail key={runKey(selected.record)} item={selected} open={open} />}
      </div>}
  </div>
}

function RunDetail({ item, open }: { item: AgentRun; open: OpenRecord }) {
  const { record, state, occurred_at } = item
  const link = record.run_type === 'request' ? operations[record.operation]?.link : undefined
  return <aside className="panel ops-detail" aria-label="执行记录详情">
    <div className="section-heading"><h2><Bot aria-hidden="true" />执行详情</h2><OpsTag tone={stateTone[state]}>{filters[state]}</OpsTag></div>
    <h3>{runTitle(record)}</h3>
    {record.run_type === 'mail' ? <>
      <dl className="review-properties">
        <div><dt>登记身份</dt><dd>{record.agent_name}</dd></div><div><dt>关联客户</dt><dd>{record.customers}</dd></div>
        <div><dt>最后活动</dt><dd>{dateLabel(occurred_at)}</dd></div>
      </dl>
      <div className="agent-run-counts"><span>SMTP 已接受 <strong>{record.accepted}</strong></span><span>等待或处理中 <strong>{record.active}</strong></span>
        <span>提交失败 <strong>{record.failures}</strong></span><span>结果未知 <strong>{record.unknown}</strong></span>
        <span>明确退信 <strong>{record.bounced}</strong></span></div>
      <MailJobList agent={record.agent_id} task={record.task_id} open={open} />
    </> : <>
      <dl className="review-properties"><div><dt>请求 ID</dt><dd><code>{record.request_id}</code></dd></div>
        <div><dt>操作时间</dt><dd>{dateLabel(occurred_at)}</dd></div><div><dt>结果</dt><dd>{requestResults[record.result] ?? record.result}</dd></div>
        {(record.requires_human || record.retryable) && <div><dt>后续处理</dt><dd>{record.requires_human ? '需要人工核对' : '读取最新版本后可重新发起'}</dd></div>}
        {record.outcome && <div><dt>错误代码</dt><dd><code>{record.outcome}</code></dd></div>}
      </dl>
      {link && record.entity_id && <button type="button" className="button primary" onClick={() => open(link, record.entity_id!)}>
        <ArrowRight aria-hidden="true" />打开关联记录</button>}
    </>}
  </aside>
}

function MailJobList({ agent, task, open }: { agent: string; task: string; open: OpenRecord }) {
  const [offset, setOffset] = useState(0)
  const cache = useQueryClient()
  const update = useMutation({
    mutationFn: ({ id, action }: { id: string; action: MailJobAction }) =>
      send({ action: 'update_mail_job', job_id: id, job_action: action }, 'mail_job_updated'),
    onSuccess: async result => {
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['agent-run-mail'] }),
        cache.invalidateQueries({ queryKey: ['agent-runs'] }),
        cache.invalidateQueries({ queryKey: ['mail-draft', result.draft_id] }),
      ])
    },
  })
  const query = useQuery({ queryKey: ['agent-run-mail', agent, task, offset],
    queryFn: () => send({ action: 'agent_run_mail', agent_id: agent, task_id: task, offset }, 'agent_run_mail') })
  return <>
    <h3>邮件执行记录</h3>
    {query.isPending && <Empty>正在读取邮件队列…</Empty>}
    {query.isError && <DataError error={query.error} />}
    {update.isError && <p className="alert error" role="alert">{mailError(update.error)}</p>}
    {update.data && <p className="review-notice" role="status">{update.data.state === 'cancelled' ? '邮件任务已取消，内容已回到草稿。' : '已准备新草稿，可核对后提交。'}</p>}
    <ol className="ops-events">{query.data?.rows.map(row => <li key={row.id}>
      <span className="dashboard-symbol" data-tone={['failed', 'delivery_unknown', 'bounced'].includes(row.state) ? 'orange' : 'teal'}><Send aria-hidden="true" /></span>
      <div><button type="button" className="text-button" onClick={() => open('draft', row.draft_id)}>{row.subject}</button>
        <small>{row.state === 'bounced' ? '明确退信' : row.state === 'cancelled' ? '已取消' : states[row.state as keyof typeof states] ?? row.state} · {mailPurposes[row.purpose as keyof typeof mailPurposes] ?? row.purpose}</small>
        <small>{dateLabel(row.occurred_at)}{row.outcome && ` · ${mailError(row.outcome)}`}</small>
        <button type="button" className="text-button" onClick={() => open('contact', row.contact_id)}><UserRound aria-hidden="true" />{row.contact_name}</button>
        {row.can_cancel && <ConfirmButton label="取消这项未发送邮件，并将内容退回草稿？" disabled={update.isPending}
          onConfirm={() => update.mutate({ id: row.id, action: 'cancel' })}><Square aria-hidden="true" />取消发送</ConfirmButton>}
        {row.can_reopen && <ConfirmButton label="创建可编辑的新草稿版本，以便人工核对后再提交？" disabled={update.isPending}
          onConfirm={() => update.mutate({ id: row.id, action: 'reopen' })}><RotateCcw aria-hidden="true" />准备重试</ConfirmButton>}
      </div>
    </li>)}</ol>
    {query.data && <Pagination offset={offset} more={query.data.has_more} change={setOffset} />}
  </>
}
