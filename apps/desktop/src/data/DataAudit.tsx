import { Select } from '../ui/Select'
import { useDeferredValue, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowRight, ClipboardList, Funnel, Search } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { Empty, Pagination } from '../crm/components'
import { DataError } from './DataError'
import { OpsTag } from './OpsUI'
import { send, type AuditEntry, type AuditFilter, type AuditTarget } from './api'

const initial: AuditFilter = { source: '', entity: '', days: 0, keyword: '' }
const sources: Record<string, string> = { manual: '人工', agent: 'Agent / MCP', system: '系统' }
const entities: Record<string, string> = {
  contact: '客户', lead: '候选客户', activity: '跟进记录', email_draft: '邮件草稿', email_outbox: '发送队列', inbox_message: '收件记录',
  email_account: '邮箱账户', imap_account: '收件账户', mail_asset: '邮件附件', customer_document: '客户文件', backup: '本地备份',
  customer_view: '客户视图', custom_field: '自定义字段', email_template: '邮件模板', suppression: '拒发名单',
}
const operations: Record<string, string> = {
  create: '新建', update: '更新', edit: '编辑', restore: '恢复', create_backup: '创建备份', archive: '归档', unarchive: '取消归档',
  import: '导入', delete: '删除', merge: '合并', promote: '转为客户', approve: '人工批准', request_approval: '提交审核', enqueue: '加入发送队列',
  authorized_enqueue: '按 Agent 授权入队', link_contact: '关联客户', patch_custom_fields: '编辑自定义属性', propose_agent_fields: '提交字段变更建议',
  agent_field_conflict: '发现字段冲突', smtp_accepted: 'SMTP 已接受', failed: '提交失败', delivery_unknown: '提交结果未知',
  block: '拒发', unblock: '取消拒发', move: '调整顺序', set_preferences: '保存偏好',
  save_company_profile: '保存公司档案', set_company_membership: '更新公司归属', remove_company_membership: '解除公司关联',
}

function operation(row: AuditEntry) {
  if (row.entity === 'backup' && row.operation === 'create') return '创建本地备份'
  return operations[row.operation] ?? row.operation
}

function useAudit(filter: AuditFilter, offset: number) {
  return useQuery({ queryKey: ['audit', 'workspace', filter, offset], queryFn: () => send({ action: 'audit_workspace', offset, ...filter }, 'audit_workspace') })
}

export function RecentAudit({ open }: { open: () => void }) {
  const query = useAudit(initial, 0)
  return <section className="panel ops-table data-recent-audit" aria-label="近期操作审计">
    <div className="section-heading"><h2>操作审计记录</h2><button type="button" className="button" onClick={open}><Funnel aria-hidden="true" />筛选审计</button></div>
    {query.isError && <DataError error={query.error} />}
    {query.isPending && <Empty>正在读取操作记录…</Empty>}
    {query.data && <AuditTable rows={query.data.rows.slice(0, 5)} />}
  </section>
}

export function AuditWorkspace({ open }: { open: (target: AuditTarget) => void }) {
  const [filter, setFilter] = useState(initial)
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState('')
  const deferred = useDeferredValue(filter.keyword.trim())
  const query = useAudit({ ...filter, keyword: deferred }, offset)
  const current = query.data?.rows.find(row => row.id === selected) ?? query.data?.rows[0]
  function change(patch: Partial<AuditFilter>) {
    setFilter(old => ({ ...old, ...patch })); setOffset(0); setSelected('')
  }
  return <>
    <div className="panel ops-controls data-audit-filters">
      <label className="followup-search"><Search aria-hidden="true" /><input type="search" maxLength={160} aria-label="搜索审计记录"
        placeholder="记录名称、编号、来源或操作代码" value={filter.keyword} onChange={event => change({ keyword: event.target.value })} /></label>
      <div className="row-actions"><Select aria-label="审计来源" value={filter.source} onChange={event => change({ source: event.target.value })}>
        <option value="">全部来源</option>{Object.entries(sources).map(([key,label]) => <option key={key} value={key}>{label}</option>)}
      </Select><Select aria-label="审计对象类型" value={filter.entity} onChange={event => change({ entity: event.target.value })}>
        <option value="">全部对象</option>{(query.data?.entities ?? (filter.entity ? [filter.entity] : [])).map(key => <option key={key} value={key}>{entities[key] ?? key}</option>)}
      </Select><Select aria-label="审计时间范围" value={filter.days} onChange={event => change({ days: Number(event.target.value) })}>
        <option value="0">全部日期</option>{[7,30,90].map(days => <option key={days} value={days}>最近 {days} 天</option>)}
      </Select><button type="button" className="button" disabled={!filter.source && !filter.entity && !filter.keyword && !filter.days} onClick={() => change(initial)}>清除筛选</button></div>
    </div>
    <div className="ops-split data-audit-grid" data-selected={Boolean(current)}>
      <section className="panel ops-table"><div className="section-heading"><h2>操作记录</h2></div>
        {query.isPending && <Empty>正在读取审计…</Empty>}
        {query.isError && <DataError error={query.error} />}
        {query.data && <><AuditTable rows={query.data.rows} current={current?.id} pick={setSelected} />
          <Pagination offset={offset} pageSize={20} more={query.data.has_more} change={next => { setOffset(next); setSelected('') }} /></>}
      </section>
      {current && <section className="panel ops-detail data-audit-detail" aria-label="审计详情">
        <div className="section-heading"><h2><ClipboardList aria-hidden="true" />审计详情</h2><SourceTag source={current.source} /></div>
        <h3>{operation(current)}</h3>
        <dl className="review-properties"><div><dt>操作时间</dt><dd>{dateLabel(current.occurred_at)}</dd></div>
          <div><dt>操作者</dt><dd>{current.actor_name ?? current.actor}</dd></div><div><dt>对象类型</dt><dd>{entities[current.entity] ?? current.entity}</dd></div>
          {current.task_id && <div><dt>任务或请求编号</dt><dd>{current.task_id}</dd></div>}
          {current.label && <div><dt>关联记录</dt><dd>{current.label}</dd></div>}
          <div><dt>对象编号</dt><dd><code>{current.entity_id}</code></dd></div>
          <div><dt>操作代码</dt><dd><code>{current.operation}</code></dd></div></dl>
        {current.target && <button type="button" className="button primary" onClick={() => open(current.target!)}><ArrowRight aria-hidden="true" />打开关联记录</button>}
      </section>}
    </div>
  </>
}

function SourceTag({ source }: { source: string }) {
  return <OpsTag tone={source === 'agent' ? 'purple' : source === 'manual' ? 'teal' : 'gray'}>{sources[source] ?? source}</OpsTag>
}

function AuditTable({ rows, current, pick }: { rows: AuditEntry[]; current?: string; pick?: (id: string) => void }) {
  if (!rows.length) return <Empty>没有符合条件的审计记录。</Empty>
  return <div className="table-wrap"><table aria-label="操作审计记录"><thead><tr><th>时间</th><th>来源</th><th>操作</th><th>关联记录</th></tr></thead>
    <tbody>{rows.map(row => <tr key={row.id} data-selected={row.id === current}>
      <td>{dateLabel(row.occurred_at)}</td><td><SourceTag source={row.source} /></td>
      <td>{pick ? <button type="button" className="text-button" aria-pressed={row.id === current} onClick={() => pick(row.id)}>{operation(row)}</button> : <strong>{operation(row)}</strong>}</td>
      <td>{row.label || (row.entity === 'backup' ? row.entity_id : entities[row.entity] ?? row.entity)}</td>
    </tr>)}</tbody>
  </table></div>
}
