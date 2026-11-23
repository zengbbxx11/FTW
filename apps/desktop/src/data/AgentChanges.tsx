import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateLabel, request, stages, type AgentChange } from '../crm/api'
import { ConfirmButton, Empty, Pagination, Problem } from '../crm/components'
import { CustomerIcon } from '../crm/CustomerIcon'

const fields: Record<string, string> = {
  stage: '客户阶段', customer_level: '客户等级', primary_language: '沟通语言',
  main_need: '主要采购需求', sample_status: '样品进度', notes: '备注',
}
const states: Record<string, string> = { applied: '已写入', pending: '待核对', kept: '保留工作台值', accepted: '已采纳' }
function display(row: AgentChange, value: string) {
  return row.field === 'stage' ? stages[value as keyof typeof stages] ?? value : value || '（空）'
}

export function AgentChanges({ openContact }: { openContact: (id: string) => void }) {
  const [pending, setPending] = useState(false)
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState('')
  const [notice, setNotice] = useState('')
  const rows = useQuery({ queryKey: ['audit','agent-changes',pending,offset],
    queryFn: () => request({ action: 'list_agent_changes', pending, offset }, 'agent_changes') })
  const row = rows.data?.rows.find(item => item.id === selection) ?? rows.data?.rows[0]
  return <section aria-label="Agent 数据变更">
    <div className="section-heading"><div><h2>Agent 数据变更</h2><p className="muted">核对客户字段的写入历史、修订冲突和待确认建议。</p></div>
      <div className="row-actions">
        <button className="button" aria-pressed={!pending} onClick={() => { setPending(false); setOffset(0) }}>全部变更</button>
        <button className="button" aria-pressed={pending} onClick={() => { setPending(true); setOffset(0) }}>待核对</button>
      </div></div>
    {notice && <p className="review-notice" role="status">{notice}</p>}
    {rows.isError && <Problem error={rows.error} />}
    {rows.isPending && <Empty>正在读取变更记录…</Empty>}
    {rows.data?.rows.length === 0 && <Empty>{pending ? '当前没有待核对的字段。' : '当前没有可对比的 Agent 字段变更。'}</Empty>}
    {row && <div className="review-layout">
      <section className="panel review-list"><h3>变更记录</h3>
        {rows.data!.rows.map(item => <button key={item.id} className="review-list-item" aria-pressed={row.id === item.id}
          onClick={() => { setSelection(item.id); setNotice('') }}>
          <span className="review-item-icon"><CustomerIcon name="edit" /></span>
          <span><strong>{item.contact_name}</strong><small>{fields[item.field]} · {dateLabel(item.created_at)}</small></span>
          <span className="review-badge" data-change-state={item.state}>{states[item.state]}</span>
        </button>)}
        <Pagination offset={offset} more={rows.data!.has_more} change={setOffset} />
      </section>
      <ChangeDetail key={row.id} row={row} openContact={openContact} changed={accept => {
        setNotice(accept ? '已采纳 Agent 建议。' : '已保留工作台当前值。')
        if (pending && rows.data?.rows.length === 1 && offset) setOffset(offset - 20)
      }} />
    </div>}
  </section>
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
  const later = row.current_value !== row.previous_value && row.current_value !== row.proposed_value
  return <section className="panel review-detail">
    <div className="section-heading"><h3>{fields[row.field]} · {row.contact_name}</h3>
      <button className="button" onClick={() => openContact(row.contact_id)}>查看客户</button></div>
    <div className="comparison-cards">
      <article><h3>{row.state === 'applied' ? '修改前' : '记录时的工作台值'}</h3><p className="long-text">{display(row,row.previous_value)}</p></article>
      <article><h3>{row.state === 'applied' ? 'Agent 写入值' : 'Agent 建议值'}</h3><p className="long-text">{display(row,row.proposed_value)}</p></article>
    </div>
    {row.current_value !== row.previous_value && <div className="review-preview"><strong>当前字段值</strong><p className="long-text">{display(row,row.current_value)}</p></div>}
    <p className="muted">MCP 请求 {row.request_id} · {dateLabel(row.created_at)}</p>
    {row.state === 'pending' && <>
      {later && <p className="alert warning">该字段此后又有修改。请在客户资料中核对并编辑。</p>}
      <div className="form-footer">
        <button className="button" disabled={resolve.isPending} onClick={() => resolve.mutate(false)}>保留当前值</button>
        <ConfirmButton primary label="用 Agent 建议值更新此字段？" disabled={later || resolve.isPending}
          onConfirm={() => resolve.mutate(true)}>采纳建议</ConfirmButton>
      </div>
    </>}
    {resolve.isError && <Problem error={resolve.error} />}
  </section>
}
