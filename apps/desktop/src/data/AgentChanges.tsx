import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bot, Check, CircleCheck, ClipboardList, Clock, ShieldCheck, TriangleAlert, UserRound } from 'lucide-react'
import { dateLabel, methodKinds, request, stages, type AgentChange, type MethodKind } from '../crm/api'
import { ConfirmButton, Empty, Pagination, Problem } from '../crm/components'
import { WorkspaceMetrics } from '../ui/WorkspaceMetrics'

const fields: Record<string, string> = {
  stage: '客户阶段', customer_level: '客户等级', primary_language: '沟通语言',
  main_need: '主要采购需求', sample_status: '样品进度', notes: '备注',
  contact_method: '联系渠道', product_interest: '产品兴趣与意向数量',
}
const states: Record<string, string> = { applied: '已写入', pending: '待核对', kept: '保留工作台值', accepted: '已采纳' }
const evidenceLabels = { research: 'Agent 调查', inferred: 'Agent 推断', unverified: '未核实' }
function display(row: AgentChange, value: string) {
  if (!value) return '（空）'
  if (row.field === 'stage') return stages[value as keyof typeof stages] ?? value
  if (row.field !== 'contact_method' && row.field !== 'product_interest') return value
  try {
    const detail = JSON.parse(value)
    if (row.field === 'contact_method') {
      return [methodKinds[detail.kind as MethodKind], detail.value, detail.label].filter(Boolean).join(' · ')
    }
    return [detail.product_text, detail.intent_quantity != null ? `意向数量 ${detail.intent_quantity}${detail.quantity_is_estimate ? '（估计）' : ''}` : null,
      detail.type === 'correct_product_interest' && detail.requirement === '' ? '清空采购需求' : detail.requirement,
      detail.type === 'correct_product_interest' && detail.intent_quantity == null && detail.quantity_is_estimate != null
        ? (detail.quantity_is_estimate ? '标记数量为估计值' : '标记数量为确认值') : null,
    ].filter(Boolean).join(' · ')
  } catch {
    return value
  }
}

type Scope = '' | 'pending' | 'applied' | 'conflict'

export function AgentChanges({ openContact, openAudit }: { openContact: (id: string) => void; openAudit: () => void }) {
  const [scope, setScope] = useState<Scope>('')
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState('')
  const [notice, setNotice] = useState('')
  const rows = useQuery({ queryKey: ['audit','agent-changes',scope,offset],
    queryFn: () => request({ action: 'list_agent_changes', pending: false, scope, offset }, 'agent_changes') })
  const row = rows.data?.rows.find(item => item.id === selection) ?? rows.data?.rows[0]
  function select(next: Scope) { setScope(next); setOffset(0); setSelection(''); setNotice('') }
  return <section className="agent-change-workspace" aria-label="Agent 数据变更">
    <div className="section-heading"><div><h2>Agent 数据变更与冲突</h2><p className="muted">核对客户资料的原值、当前值和 Agent 建议。</p></div>
      <button className="button" onClick={openAudit}><ClipboardList aria-hidden="true" />查看操作审计</button></div>
    {rows.data && <WorkspaceMetrics label="Agent 字段变更分类" active={scope} select={select} items={[
      { id: '', label: '全部变更', count: rows.data.total, icon: Bot, tone: 'teal' },
      { id: 'applied', label: 'Agent 已写入', count: rows.data.applied, icon: CircleCheck, tone: 'blue' },
      { id: 'pending', label: '待人工核对', count: rows.data.pending, icon: Clock, tone: 'orange' },
      { id: 'conflict', label: '字段冲突', count: rows.data.conflicts, icon: TriangleAlert, tone: 'violet' },
    ] as const} />}
    {notice && <p className="review-notice" role="status">{notice}</p>}
    {rows.isError && <Problem error={rows.error} />}
    {rows.isPending && <Empty>正在读取变更记录…</Empty>}
    {rows.data?.rows.length === 0 && <Empty>当前没有此类字段变更。</Empty>}
    {row && rows.data && <div className="agent-change-layout">
      <section className="panel agent-change-list"><h3>客户字段变更</h3>
        <div className="table-wrap"><table aria-label="Agent 字段变更记录"><thead><tr><th>客户</th><th>影响字段</th><th>状态</th><th>记录时间</th></tr></thead>
          <tbody>{rows.data.rows.map(item => <tr key={item.id} data-selected={row.id === item.id}>
            <td><button className="text-button" aria-pressed={row.id === item.id} onClick={() => { setSelection(item.id); setNotice('') }}>
              <Bot aria-hidden="true" />{item.contact_name}</button></td><td>{fields[item.field]}</td>
            <td><ChangeBadge row={item} /></td><td>{dateLabel(item.created_at)}</td>
          </tr>)}</tbody></table></div>
        <Pagination offset={offset} more={rows.data.has_more} change={next => { setOffset(next); setSelection('') }} />
      </section>
      <ChangeDetail key={row.id} row={row} openContact={openContact} changed={accept => {
        setNotice(accept ? '已采纳 Agent 建议。' : '已保留工作台当前值。')
        if ((scope === 'pending' || scope === 'conflict') && rows.data?.rows.length === 1 && offset) setOffset(offset - 20)
      }} />
    </div>}
  </section>
}

function ChangeBadge({ row }: { row: AgentChange }) {
  return <span className="review-badge" data-change-state={row.conflict ? 'conflict' : row.state}>
    {row.conflict ? '字段冲突' : states[row.state] ?? row.state}
  </span>
}

function ChangeDetail({ row, openContact, changed }: { row: AgentChange; openContact: (id: string) => void; changed: (accept: boolean) => void }) {
  const cache = useQueryClient()
  const resolve = useMutation({
    mutationFn: (accept: boolean) => request({ action: 'resolve_agent_change', id: row.id, accept }, 'saved'),
    onSuccess: (_, accept) => {
      changed(accept)
      return Promise.all(['audit','contacts','contact'].map(key => cache.invalidateQueries({ queryKey: [key] })))
    },
    onError: () => cache.invalidateQueries({ queryKey: ['audit'] }),
  })
  return <section className="panel agent-change-detail">
    <div className="section-heading"><h3>{fields[row.field]}</h3><ChangeBadge row={row} /></div>
    <p className="muted">{row.contact_name}</p>
    <div className="agent-change-values">
      <article data-version="current"><h3><UserRound aria-hidden="true" />工作台当前值</h3><p className="long-text">{display(row,row.current_value)}</p></article>
      <article data-version="proposed"><h3><Bot aria-hidden="true" />{row.state === 'applied' ? 'Agent 写入值' : 'Agent 建议值'}</h3><p className="long-text">{display(row,row.proposed_value)}</p></article>
    </div>
    <dl className="review-properties agent-change-origin">
      <div><dt>信息来源</dt><dd>{row.evidence_kind ? evidenceLabels[row.evidence_kind] : 'Agent · 未记录来源类型'}</dd></div>
      {row.evidence_url && <div><dt>来源网址</dt><dd>{row.evidence_url}</dd></div>}
      {row.evidence_note && <div><dt>来源说明</dt><dd className="long-text">{row.evidence_note}</dd></div>}
      <div><dt>MCP 请求</dt><dd>{row.request_id}</dd></div><div><dt>记录时间</dt><dd>{dateLabel(row.created_at)}</dd></div>
    </dl>
    {(row.state !== 'pending' || row.current_value !== row.previous_value) && <details className="mail-conversation">
      <summary><Clock aria-hidden="true" />记录时的工作台值</summary><p className="long-text">{display(row,row.previous_value)}</p></details>}
    <button className="button" onClick={() => openContact(row.contact_id)}><UserRound aria-hidden="true" />查看客户</button>
    {row.state === 'pending' && <>
      {row.conflict && <p className="alert warning" role="status">字段在建议提出后已有修改。请在客户资料中核对或编辑。</p>}
      {row.archived && <p className="muted">此客户已归档。可保留当前值并结束核对。</p>}
      <div className="form-footer">
        <button className="button" disabled={resolve.isPending} onClick={() => resolve.mutate(false)}><ShieldCheck aria-hidden="true" />保留当前值</button>
        <ConfirmButton primary label="用 Agent 建议值更新此字段？" disabled={row.conflict || row.archived || resolve.isPending}
          onConfirm={() => resolve.mutate(true)}><Check aria-hidden="true" />采纳建议</ConfirmButton>
      </div>
    </>}
    {resolve.isError && <Problem error={resolve.error} />}
  </section>
}
