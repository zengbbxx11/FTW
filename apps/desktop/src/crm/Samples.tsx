import { Select } from '../ui/Select'
import { useDeferredValue, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, ClipboardList, Clock, History, Layers, Package, Plus, Search, Truck, UserRound, UsersRound } from 'lucide-react'
import { dateLabel, request, type ActivityInput } from './api'
import { Empty, Pagination, Problem } from './components'
import { ActivityEditor, FollowupCustomerPicker } from './Timeline'
import { WorkspaceMetrics } from '../ui/WorkspaceMetrics'

export function SampleWorkspace({ back, openContact, openActivity }: {
  back: () => void; openContact: (id: string) => void; openActivity: (id: string) => void
}) {
  const [byBatch, setByBatch] = useState(false)
  const [status, setStatus] = useState('')
  const [search, setSearch] = useState('')
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState('')
  const [editor, setEditor] = useState<{ id: string; sample?: ActivityInput['sample'] } | null>(null)
  const keyword = useDeferredValue(search.trim())
  const query = useQuery({ queryKey: ['sample-workspace', byBatch, keyword, status, offset],
    queryFn: () => request({ action: 'sample_workspace', by_batch: byBatch, keyword, status, offset }, 'sample_workspace') })
  const catalog = useQuery({ queryKey: ['crm-states'], staleTime: Infinity, queryFn: () => request({ action: 'state_catalog' }, 'state_catalog') })
  const states = catalog.data?.options.filter(option => option.field === 'sample_status') ?? []
  const counts = new Map(query.data?.counts)
  const rows = query.data?.rows ?? []
  const current = rows.find(row => row.id === selected) ?? rows[0]
  function label(value: string) { return states.find(item => item.code === value)?.label ?? (value || '未填写进度') }
  function filter(value: string) { setStatus(value); setSelected(''); setOffset(0) }
  return <section className="sample-workspace">
    <header className="page-heading"><div>
      <button type="button" className="text-button back-link" onClick={back}><ArrowLeft aria-hidden="true" />返回客户管理</button>
      <h1>样品进度跟踪</h1><p>按客户或批次查看样品进度、测试沟通和反馈。</p></div>
      {!editor && <FollowupCustomerPicker label="新增样品沟通记录" select={id => { setSelected(id); setEditor({ id }) }} />}
    </header>
    {query.data && <WorkspaceMetrics label="样品进度概览" active={status} select={filter} items={[
      { id: '', label: byBatch ? '样品批次及未分批记录' : '样品相关客户', count: [...counts.values()].reduce((total, count) => total + count, 0), icon: Package, tone: 'teal' },
      { id: 'planned', label: '待安排寄样', count: counts.get('planned') ?? 0, icon: Clock, tone: 'orange' },
      { id: 'in_transit', label: '样品在途', count: counts.get('in_transit') ?? 0, icon: Truck, tone: 'blue' },
      { id: 'evaluating', label: '样品评估中', count: counts.get('evaluating') ?? 0, icon: ClipboardList, tone: 'violet' },
    ]} />}
    <div className="sample-toolbar">
      <div className="row-actions sample-modes" role="group" aria-label="样品视图">
        <button className="button" aria-pressed={!byBatch} onClick={() => { setByBatch(false); filter('') }}><UsersRound aria-hidden="true" />按客户</button>
        <button className="button" aria-pressed={byBatch} onClick={() => { setByBatch(true); filter('') }}><Layers aria-hidden="true" />按批次</button>
      </div>
      <label className="followup-search"><Search aria-hidden="true" /><input type="search" aria-label="搜索样品记录" placeholder="搜索客户、型号或近期反馈" maxLength={300}
        value={search} onChange={event => { setSearch(event.target.value); setOffset(0); setSelected('') }} /></label>
      <Select aria-label="筛选样品进度" value={status} onChange={event => filter(event.target.value)}>
        <option value="">全部样品进度</option>{states.map(item => <option key={item.code} value={item.code}>{item.label}</option>)}
      </Select>
    </div>
    {editor && <section className="panel sample-editor"><h2>新增样品沟通记录</h2>
      <ActivityEditor key={editor.id} contactId={editor.id} initialType="sample" initialSample={editor.sample} close={() => setEditor(null)} />
    </section>}
    {query.isError && <Problem error={query.error} />}
    {query.isPending && <Empty>正在读取样品记录…</Empty>}
    {query.data && <div className="sample-layout" data-selected={Boolean(current)}>
      <section className="panel sample-list"><div className="section-heading"><h2>{byBatch ? '样品批次' : '样品客户'}</h2></div>
        {rows.length ? <div className="table-wrap"><table aria-label="样品进度列表">
          <thead><tr><th>客户</th><th>{byBatch ? '批次' : '国家或地区'}</th><th>样品型号</th><th>数量</th><th>样品进度</th><th>最近沟通</th></tr></thead>
          <tbody>{rows.map(row => <tr key={row.id} data-selected={current?.id === row.id}>
            <td><button type="button" className="text-button" aria-pressed={current?.id === row.id}
              aria-label={byBatch ? row.contact_name + ' · ' + (row.batch_ref || row.summary || '未分批') : undefined}
              onClick={() => setSelected(row.id)}>{row.contact_name}</button>
              {row.company_name && <small>{row.company_name}</small>}</td><td>{byBatch ? row.batch_ref || '未分批' : row.country_region || '—'}</td>
            <td>{row.model || '—'}</td><td>{row.quantity === null ? '—' : `${row.quantity} 件`}</td>
            <td><span className="sample-step" data-status={row.status}>{label(row.status)}</span></td><td>{row.summary || '—'}</td>
          </tr>)}</tbody>
        </table></div> : <Empty>没有符合条件的样品记录。</Empty>}
        <Pagination offset={offset} more={query.data.has_more} change={next => { setOffset(next); setSelected('') }} />
      </section>
      {current && <aside className="panel sample-detail" aria-label="样品详情">
        <div className="section-heading"><h2><Package aria-hidden="true" />样品详情</h2>
          <span className="sample-step" data-status={current.status}>{label(current.status)}</span></div>
        <h3>{current.contact_name}</h3>
        <dl className="review-properties">
          <div><dt>国家或地区</dt><dd>{current.country_region || '—'}</dd></div>
          {byBatch && <div><dt>批次 / 沟通次数</dt><dd>{current.batch_ref || '未分批'} · {current.events ?? 0} 条</dd></div>}
          <div><dt>样品型号</dt><dd>{current.model || '—'}</dd></div>
          <div><dt>样品数量</dt><dd>{current.quantity === null ? '—' : `${current.quantity} 件`}</dd></div>
          <div><dt>最近沟通</dt><dd>{dateLabel(current.occurred_at)}</dd></div>
        </dl>
        {current.summary && <div className="review-preview"><strong>{current.summary}</strong>
          {current.details && <p className="long-text">{current.details}</p>}</div>}
        {current.feedback && <div className="review-preview"><strong>客户反馈</strong><p className="long-text">{current.feedback}</p></div>}
        <div className="row-actions">
          <button className="button" onClick={() => openContact(current.contact_id)}><UserRound aria-hidden="true" />打开客户详情</button>
          {current.activity_id && <button className="button" onClick={() => openActivity(current.activity_id!)}><History aria-hidden="true" />最近跟进</button>}
          {!editor && <button className="button primary" onClick={() => setEditor({ id: current.contact_id,
            sample: byBatch && current.batch_ref ? { batch_ref: current.batch_ref, model: current.model, quantity: current.quantity, progress: current.status } : undefined,
          })}><Plus aria-hidden="true" />记录样品沟通</button>}
        </div>
      </aside>}
    </div>}
  </section>
}
