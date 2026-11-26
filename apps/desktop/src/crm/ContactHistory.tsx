import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { dateLabel, request, sourceLabel } from './api'
import { Empty, Pagination, Problem } from './components'
import { useCustomerFields } from './Fields'
import type { CustomField, FieldValue } from './fieldTypes'

const labels: Record<string, string> = {
  display_name: '客户名称', company_name: '公司', country_region: '国家或地区',
  stage: '客户阶段', customer_level: '客户等级', primary_language: '主要语言',
  main_need: '主要需求', sample_status: '样品状态', notes: '备注', archived_at: '归档时间',
}

function display(field: string, value: string | number | null) {
  if (value === null || value === '') return '—'
  return field === 'archived_at' && typeof value === 'number' ? dateLabel(value) : String(value)
}

export function ContactHistory({ id }: { id: string }) {
  const [custom, setCustom] = useState(false)
  return <section className="panel">
    <h2>变更历史</h2>
    <div className="row-actions">
      <button type="button" className="button" aria-pressed={!custom} onClick={() => setCustom(false)}>内置字段</button>
      <button type="button" className="button" aria-pressed={custom} onClick={() => setCustom(true)}>自定义字段记录</button>
    </div>
    {custom ? <CustomHistory id={id} /> : <BuiltInHistory id={id} />}
  </section>
}

function BuiltInHistory({ id }: { id: string }) {
  const [offset, setOffset] = useState(0)
  const history = useQuery({ queryKey: ['contact-history', id, offset],
    queryFn: () => request({ action: 'contact_history', id, offset }, 'contact_history') })
  return <>
    {history.isError && <Problem error={history.error} />}
    {history.isPending && <Empty>正在读取客户变更…</Empty>}
    {history.data && !history.data.rows.length && <Empty>尚无已记录的字段变更。</Empty>}
    {history.data && <ul className="record-list">
      {history.data.rows.map(row => <li key={row.revision}>
        <div>
          <strong>{row.source === 'manual' ? '工作台用户' : row.actor_name}</strong>
          <p className="muted">{sourceLabel(row.source)} · {dateLabel(row.occurred_at)} · 版本 {row.revision}</p>
          <dl className="profile-grid">{row.fields.map(change => <div key={change.field}>
            <dt>{labels[change.field] ?? change.field}</dt>
            <dd className="long-text">原值 {display(change.field, change.before)}<br />新值 {display(change.field, change.after)}</dd>
          </div>)}</dl>
        </div>
      </li>)}
    </ul>}
    <Pagination offset={offset} more={history.data?.has_more ?? false} change={setOffset} />
  </>
}

function customValue(value: FieldValue | null, field: CustomField | undefined) {
  if (value === null) return '—'
  const label = (id: string) => field?.options.find(option => option.id === id)?.label ?? id
  if (value.type === 'multi_select') return value.value.map(label).join('、')
  if (value.type === 'single_select') return label(value.value)
  if (value.type === 'checkbox') return value.value ? '是' : '否'
  return String(value.value)
}

function CustomHistory({ id }: { id: string }) {
  const [offset, setOffset] = useState(0)
  const definitions = useCustomerFields(true)
  const history = useQuery({ queryKey: ['contact-history', id, 'custom', offset],
    queryFn: () => request({ action: 'custom_field_history', id, offset }, 'custom_field_history') })
  return <>
    {history.isError && <Problem error={history.error} />}
    {history.isPending && <Empty>正在读取客户变更…</Empty>}
    {history.data && !history.data.rows.length && <Empty>尚无已记录的自定义字段变更。</Empty>}
    {history.data && <ul className="record-list">{history.data.rows.map(row => {
      const field = definitions.data?.fields.find(field => field.id === row.field_id)
      return <li key={row.revision + ':' + row.field_id}>
        <div>
          <strong>{row.field_name}</strong>
          <p className="muted">{row.source === 'manual' ? '工作台用户' : row.actor_name} · {dateLabel(row.occurred_at)} · 版本 {row.revision}</p>
          {row.task_id && <small>{row.task_id}</small>}
          <p className="long-text">原值 {customValue(row.before, field)}<br />新值 {customValue(row.after, field)}</p>
        </div>
      </li>
    })}</ul>}
    <Pagination offset={offset} more={history.data?.has_more ?? false} change={setOffset} />
  </>
}
