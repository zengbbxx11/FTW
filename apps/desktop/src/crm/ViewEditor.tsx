import { Select } from '../ui/Select'
import { lazy, Suspense, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { request, stages, type StateOption } from './api'
import { Field, Problem } from './components'
import { ColumnSettings } from './SavedViews'
import { CustomerIcon } from './CustomerIcon'
import { initialColumns, type AvailableColumn, type ViewConfig, type ViewInput } from './viewTypes'
import { viewQuery, viewSummary } from './viewSummary'
const AdvancedFilters = lazy(() => import('./AdvancedFilters'))

export function ViewEditor({ value, change, save, cancel, pending, available, catalog, current, defaultLocked }: {
  value: ViewInput; change: (value: ViewInput) => void; save: () => void; cancel: () => void; pending: boolean
  available: AvailableColumn[]; catalog: StateOption[]; current: ViewConfig
  defaultLocked: boolean
}) {
  const [filters, setFilters] = useState(false)
  const q = viewQuery(value.config)
  const preview = useQuery({ queryKey: ['contacts', 'view-preview', q], queryFn: () => request({ action: 'preview_contacts', query: q }, 'contact_preview') })
  const configure = (patch: Partial<ViewConfig>) => change({ ...value, config: { ...value.config, ...patch } })
  return <section className="panel saved-view-editor" aria-label="客户视图编辑器">
    <div className="section-heading"><h2>{value.id ? '编辑客户视图' : '新建客户视图'}</h2><button type="button" className="button" onClick={cancel}>关闭</button></div>
    {!value.id && <div className="row-actions view-start-options"><span>创建起点</span>
      <button type="button" className="button" onClick={() => change({ ...value, config: structuredClone(current) })}>保存当前设置</button>
      <button type="button" className="button" onClick={() => change({ ...value, config: { columns: initialColumns, layout: 'table', density: 'standard' } })}>从空白创建</button></div>}
    <form onSubmit={event => { event.preventDefault(); save() }}>
      <div className="form-grid">
        <Field label="视图名称"><input autoFocus required maxLength={100} value={value.name} onChange={event => change({ ...value, name: event.target.value })} /></Field>
        <Field label="布局"><Select value={value.config.layout ?? 'table'} onChange={event => configure({ layout: event.target.value as 'table' | 'board' })}><option value="table">表格</option><option value="board">看板</option></Select></Field>
        {value.config.layout === 'board' && <Field label="看板分组"><Select value={value.config.group_field ?? 'stage'} onChange={event => configure({ group_field: event.target.value })}>
          <option value="stage">客户阶段</option>{available.filter(column => column.custom?.kind === 'single_select').map(column => <option key={column.id} value={column.id}>{column.title}</option>)}
        </Select></Field>}
        <Field label="默认排序"><Select value={q.sort} onChange={event => configure({ query: { ...q, sort: event.target.value as typeof q.sort, order: [] } })}>
          <option value="updated">最近更新</option><option value="name">客户名称</option><option value="last_contact">最近联系</option></Select></Field>
        <Field label="搜索关键词"><input value={q.keyword} onChange={event => configure({ query: { ...q, keyword: event.target.value } })} /></Field>
        <Field label="国家 / 地区"><input value={q.country} onChange={event => configure({ query: { ...q, country: event.target.value } })} /></Field>
        <Field label="客户范围"><Select value={q.quick_view} onChange={event => configure({ query: { ...q, quick_view: event.target.value as typeof q.quick_view } })}>
          <option value="">全部</option><option value="priority">重点客户</option><option value="sample">样品客户</option><option value="in_progress">跟进中</option></Select></Field>
        <Field label="客户阶段"><Select value={q.stage} onChange={event => configure({ query: { ...q, stage: event.target.value } })}>
          <option value="">全部阶段</option>{Object.entries(stages).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</Select></Field>
        <Field label="客户等级"><input value={q.level} onChange={event => configure({ query: { ...q, level: event.target.value } })} /></Field>
        <Field label="客户类型"><input value={q.customer_type} onChange={event => configure({ query: { ...q, customer_type: event.target.value } })} /></Field>
        <Field label="客户来源"><input value={q.source} onChange={event => configure({ query: { ...q, source: event.target.value } })} /></Field>
        <Field label="归档范围"><Select value={q.archive} onChange={event => configure({ query: { ...q, archive: event.target.value as typeof q.archive } })}>
          <option value="active">有效客户</option><option value="archived">已归档客户</option><option value="all">包括已归档</option></Select></Field>
      </div>
      <div className="row-actions">
        <label><input type="checkbox" checked={value.is_default} disabled={defaultLocked} onChange={event => change({ ...value, is_default: event.target.checked, hidden: event.target.checked ? false : value.hidden })} />设为默认视图</label>
        <label><input type="checkbox" checked={value.hidden} disabled={value.is_default} onChange={event => change({ ...value, hidden: event.target.checked })} />隐藏标签</label>
      </div>
      <div className="row-actions view-start-options">
        <button type="button" className="button" onClick={() => setFilters(!filters)}><CustomerIcon name="filter" />组合筛选</button>
      </div>
      <details><summary>高级排序（最多三项）</summary>
        {(q.order ?? []).map((order, index) => <div className="row-actions" key={index}>
          <Select aria-label={`排序字段 ${index + 1}`} value={order.field} onChange={event => configure({ query: { ...q, order: q.order!.map((item, i) => i === index ? { ...item, field: event.target.value } : item) } })}>
            {available.filter(column => column.kind !== 'multi_select').map(column => <option value={column.id} key={column.id}>{column.title}</option>)}</Select>
          <Select aria-label={`排序方向 ${index + 1}`} value={String(order.descending)} onChange={event => configure({ query: { ...q, order: q.order!.map((item, i) => i === index ? { ...item, descending: event.target.value === 'true' } : item) } })}><option value="false">升序</option><option value="true">降序</option></Select>
          <button type="button" className="button" onClick={() => configure({ query: { ...q, order: q.order!.filter((_, i) => i !== index) } })}>移除</button>
        </div>)}
        <button type="button" className="button" disabled={(q.order?.length ?? 0) >= 3} onClick={() => {
          const next = available.find(column => column.kind !== 'multi_select' && !q.order?.some(order => order.field === column.id))
          if (next) configure({ query: { ...q, order: [...q.order ?? [], { field: next.id, descending: false }] } })
        }}>添加排序</button>
      </details>
      <div className="view-preview-summary" role="status"><strong>{viewSummary(value.config)}</strong>
        <span>{preview.isPending ? '正在计算匹配数量…' : preview.data ? `匹配 ${preview.data.total} 位客户` : '无法预览当前条件'}</span>
        {!!q.order?.length && <span>高级排序：{q.order.map(order => `${available.find(c => c.id === order.field)?.title ?? order.field} ${order.descending ? '降序' : '升序'}`).join('、')}</span>}
      </div>
      {preview.isError && <Problem error={preview.error} />}
      <div className="form-footer"><button type="button" className="button" onClick={cancel}>取消</button>
        <button className="button primary" disabled={pending || preview.isError || filters}><CustomerIcon name="save" />保存视图</button></div>
    </form>
    <div className="view-start-options"><ColumnSettings available={available} saved={value.config.columns} change={columns => configure({ columns })} density={value.config.density ?? 'standard'}
      setDensity={density => configure({ density })} /></div>
    {filters && <Suspense fallback={<p>正在加载组合筛选…</p>}><AdvancedFilters columns={available} catalog={catalog} filter={q.filter} query={q} close={() => setFilters(false)}
      apply={filter => { configure({ query: { ...q, filter } }); setFilters(false) }} /></Suspense>}
  </section>
}
