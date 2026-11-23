import { useDeferredValue, useState } from 'react'
import { QueryBuilder, type Field as QueryField, type RuleGroupType } from 'react-querybuilder'
import 'react-querybuilder/dist/query-builder.css'
import { useQuery } from '@tanstack/react-query'
import { dateLabel, outreachLabels, request, stages, type ContactQuery, type StateOption } from './api'
import type { AvailableColumn, FieldOperator, Filter } from './viewTypes'

const operators = {
  eq: '等于', not_eq: '不等于', contains: '包含', empty: '为空',
  gte: '大于等于', lte: '小于等于', within_days: '最近 N 天', older_than_days: '早于 N 天',
} as const
const options = (keys: FieldOperator[]) => keys.map(name => ({ name, label: operators[name] }))

function queryField(column: AvailableColumn, catalog: StateOption[]): QueryField {
  const basic = { name: column.id, label: column.title }
  if (['last_contact','mail_last_sent','mail_last_reply'].includes(column.id)) return { ...basic, inputType: 'number', operators: options(['within_days', 'older_than_days', 'empty']) }
  if (column.kind === 'checkbox') return {
    ...basic, valueEditorType: 'select', values: [{ name: 'true', label: '是' }, { name: 'false', label: '否' }],
    operators: options(['eq', 'not_eq', 'empty']),
  }
  if (column.kind === 'single_select' || column.kind === 'multi_select') {
    const field = column.id === 'level' ? 'customer_level' : 'sample_status'
    const values = column.custom ? column.custom.options.filter(option => !option.archived).map(option => ({ name: option.id, label: option.label }))
      : column.id === 'stage' ? Object.entries(stages).map(([name, label]) => ({ name, label }))
      : column.id === 'mail_status' ? Object.entries(outreachLabels).map(([name,label]) => ({ name,label }))
      : catalog.filter(option => option.field === field).map(option => ({ name: option.code, label: option.label }))
    return { ...basic, valueEditorType: 'select', values,
      operators: options(column.kind === 'multi_select' ? ['contains', 'empty'] : ['eq', 'not_eq', 'empty']) }
  }
  if (column.kind === 'number' || column.kind === 'date') return {
    ...basic, inputType: column.kind, operators: options(['eq', 'gte', 'lte', 'empty']),
  }
  return { ...basic, operators: options(['contains', 'eq', 'not_eq', 'empty']) }
}

function fromFilter(filter: Filter | null | undefined): RuleGroupType {
  if (!filter) return { combinator: 'and', rules: [] }
  if (filter.type === 'rule') return { combinator: 'and', rules: [{ field: filter.field, operator: filter.operator, value: filter.value }] }
  return { combinator: filter.logic, rules: filter.children.map(child => child.type === 'group'
    ? fromFilter(child) : { field: child.field, operator: child.operator, value: child.value }) }
}

function toFilter(group: RuleGroupType, columns: AvailableColumn[]): Filter | null {
  if (!group.rules.length) return null
  const children: Filter[] = group.rules.map(rule => {
    if ('rules' in rule) {
      const child = toFilter(rule, columns)
      if (!child) throw new Error('分组至少需要一个条件。')
      return child
    }
    const column = columns.find(column => column.id === rule.field)
    const operator = rule.operator as FieldOperator
    if (!column || !(operator in operators)) throw new Error('请选择可用字段和条件。')
    const raw = String(rule.value ?? '').trim()
    let value: string | number | boolean | null = raw
    if (operator === 'empty') value = null
    else if (!raw) throw new Error('请输入筛选值。')
    else if (column.kind === 'checkbox') value = raw === 'true'
    else if (column.kind === 'number' || operator === 'within_days' || operator === 'older_than_days') {
      value = Number(raw)
      if (!Number.isFinite(value)) throw new Error('请输入有效数字。')
    }
    return { type: 'rule', field: rule.field, operator, value }
  })
  return { type: 'group', logic: group.combinator === 'or' ? 'or' : 'and', children }
}

export default function AdvancedFilters({ columns, catalog, filter, query, apply, close }: {
  columns: AvailableColumn[]; catalog: StateOption[]; filter: Filter | null | undefined
  query: ContactQuery; apply: (filter: Filter | null) => void; close: () => void
}) {
  const [draft, setDraft] = useState(() => fromFilter(filter))
  const [error, setError] = useState('')
  const deferred = useDeferredValue(draft)
  let criteria: ContactQuery | null = null
  try {
    criteria = { ...query, filter: toFilter(deferred, columns), offset: 0, page_size: 20, custom_columns: [] }
  } catch { /* Some rules are unfinished while the user edits them. */ }
  const preview = useQuery({
    queryKey: ['contacts', 'filter-preview', criteria],
    enabled: criteria !== null,
    queryFn: () => request({ action: 'list_contacts', query: criteria! }, 'contacts'),
  })
  return <div className="filter-editor-layout"><section className="panel r3-advanced">
    <div className="section-heading"><h2>组合筛选</h2>
      <button type="button" className="button" onClick={close}>关闭</button></div>
    <QueryBuilder fields={columns.map(column => queryField(column,catalog))}
      query={draft} onQueryChange={setDraft} maxLevels={3} autoSelectValue={false}
      operators={options(Object.keys(operators) as FieldOperator[])}
      combinators={[{ name: 'and', label: '满足全部' }, { name: 'or', label: '满足任一' }]}
      translations={{ addRule: { label: '添加条件' }, addGroup: { label: '添加分组' }, removeRule: { label: '移除' }, removeGroup: { label: '移除分组' } }}
       />
    {error && <p className="alert error" role="alert">{error}</p>}
    <div className="form-footer">
      <button type="button" className="button" onClick={() => apply(null)}>清除组合筛选</button>
      <button type="button" className="button primary" onClick={() => {
        try { apply(toFilter(draft, columns)) } catch (issue) {
          setError(issue instanceof Error ? issue.message : String(issue))
        }
      }}>应用</button>
    </div>
  </section>
  <section className="panel filter-preview" aria-label="筛选结果预览">
    <h2>结果预览</h2>
    {!criteria ? <p className="muted">填完筛选值后显示结果。</p>
      : preview.isError ? <p className="muted">无法预览当前条件。</p>
      : preview.isPending ? <p className="muted">正在查找匹配客户…</p>
      : preview.data.rows.length ? <><small>最多显示前 5 位客户</small><table>
        <thead><tr><th>客户</th><th>国家 / 地区</th><th>最近联系</th></tr></thead>
        <tbody>{preview.data.rows.slice(0, 5).map(row => <tr key={row.id}>
          <td>{row.display_name}</td><td>{row.country_region || '—'}</td><td>{dateLabel(row.last_contact_at)}</td>
        </tr>)}</tbody>
      </table></> : <p className="muted">没有符合条件的客户。</p>}
  </section></div>
}
