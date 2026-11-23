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
  { id: 'name', title: '客户姓名', kind: 'text', width: 180 },
  { id: 'company', title: '公司或店铺', kind: 'text', width: 150 },
  { id: 'country', title: '国家或地区', kind: 'text', width: 110 },
  { id: 'stage', title: '客户阶段', kind: 'single_select', width: 108 },
  { id: 'sample', title: '样品进度', kind: 'single_select', width: 126 },
  { id: 'level', title: '客户等级', kind: 'single_select', width: 98 },
  { id: 'product', title: '产品兴趣', kind: 'text', width: 144 },
  { id: 'last_contact', title: '最近联系', kind: 'date', width: 138 },
  { id: 'email', title: '主要邮箱', kind: 'text', width: 186 },
  { id: 'source', title: '记录来源', kind: 'text', width: 104 },
]

const defaultHidden = new Set(['email', 'source'])
export const initialColumns: ColumnSetting[] = systemColumns.map(column => ({
  field: column.id, label: null, width: column.width, hidden: defaultHidden.has(column.id),
}))

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
