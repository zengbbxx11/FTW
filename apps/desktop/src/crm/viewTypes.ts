import type { ContactQuery } from './api'
import type { CustomField, FieldKind } from './fieldTypes'

export type FieldOperator = 'eq' | 'not_eq' | 'contains' | 'gte' | 'lte' | 'empty' | 'within_days' | 'older_than_days'
export type Filter = { type: 'group'; logic: 'and' | 'or'; children: Filter[] }
  | { type: 'rule'; field: string; operator: FieldOperator; value: string | number | boolean | null }
export type ColumnSetting = { field: string; label: string | null; width: number; hidden: boolean }
export type ViewConfig = {
  query?: Partial<ContactQuery>; layout?: 'table' | 'board'; density?: 'compact' | 'standard'
  group_field?: string | null; columns?: ColumnSetting[]
}
export type SavedView = {
  id: string; revision: number; name: string; hidden: boolean; is_default: boolean; archived: boolean; config: ViewConfig
}
export type ViewInput = Omit<SavedView, 'id' | 'revision' | 'archived'> & { id: string | null; expected_revision: number | null }
export type CellTarget = { id: string; revision: number; epoch: string }
export type AvailableColumn = { id: string; title: string; kind: FieldKind; width: number; custom?: CustomField }

export const systemColumns: AvailableColumn[] = [
  { id: 'name', title: '客户姓名', kind: 'text', width: 210 },
  { id: 'company', title: '公司 / 店铺', kind: 'text', width: 126 },
  { id: 'country', title: '国家 / 地区', kind: 'text', width: 124 },
  { id: 'stage', title: '客户阶段', kind: 'single_select', width: 118 },
  { id: 'sample', title: '样品进度', kind: 'single_select', width: 118 },
  { id: 'level', title: '客户等级', kind: 'single_select', width: 118 },
  { id: 'product', title: '产品兴趣', kind: 'text', width: 148 },
  { id: 'mail_status', title: '邮件开发', kind: 'single_select', width: 124 },
  { id: 'mail_last_sent', title: '最近发信', kind: 'date', width: 122 },
  { id: 'mail_last_reply', title: '最近回复', kind: 'date', width: 122 },
  { id: 'mail_count', title: '开发提交数', kind: 'number', width: 112 },
  { id: 'mail_subject', title: '最近开发主题', kind: 'text', width: 150 },
  { id: 'last_contact', title: '最近联系', kind: 'date', width: 122 },
  { id: 'email', title: '主要邮箱', kind: 'text', width: 186 },
  { id: 'source', title: '记录来源', kind: 'text', width: 104 },
]

const defaultHidden = new Set(['company', 'email', 'source', 'mail_last_sent', 'mail_last_reply', 'mail_count', 'mail_subject'])
export const initialColumns: ColumnSetting[] = systemColumns.map(column => ({
  field: column.id, label: null, width: column.width, hidden: defaultHidden.has(column.id),
}))

export function initialColumnsForView(id: string | undefined): ColumnSetting[] {
  const order = id === 'sample' ? ['name', 'country', 'product', 'sample', 'stage', 'last_contact']
    : id?.startsWith('mail_') ? ['name', 'country', 'mail_last_sent', 'mail_status', 'mail_count', 'mail_last_reply'] : null
  if (!order) return initialColumns
  return [...order, ...initialColumns.map(column => column.field).filter(field => !order.includes(field))]
    .map(field => ({ ...initialColumns.find(column => column.field === field)!, hidden: !order.includes(field) }))
}

export function columnsWithDefaults(saved: ColumnSetting[] | undefined, available: AvailableColumn[]) {
  const columns = saved?.length ? saved : initialColumns
  const defined = new Set(columns.map(column => column.field))
  return [
    ...columns,
    ...available.filter(column => !defined.has(column.id)).map(column =>
      ({ field: column.id, label: null, width: column.width, hidden: true })),
  ]
}

export function availableColumns(fields: CustomField[]): AvailableColumn[] {
  return [...systemColumns, ...fields.map(field => ({ id: field.id, title: field.name, kind: field.kind, width: 148, custom: field }))]
}
