import { stages, outreachLabels, type ContactQuery } from './api'
import { systemColumns, type ViewConfig, type Filter } from './viewTypes'

export const builtinViewIds = new Set(['all', 'priority', 'sample', 'mail_no_reply', 'mail_replied', 'mail_bounced'])
export function viewQuery(config: ViewConfig): ContactQuery {
  return { keyword: '', quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '', archive: 'active', sort: 'updated', ...config.query, offset: 0 }
}
function filterText(filter: Filter): string {
  if (filter.type === 'group') return '(' + filter.children.map(filterText).join(filter.logic === 'and' ? ' 且 ' : ' 或 ') + ')'
  const title = systemColumns.find(column => column.id === filter.field)?.title ?? '自定义字段'
  const op = { eq: '等于', not_eq: '不等于', contains: '包含', empty: '为空', gte: '≥', lte: '≤', within_days: '最近天数', older_than_days: '早于天数' }[filter.operator]
  const value = filter.field === 'stage' ? stages[filter.value as keyof typeof stages] : filter.field === 'mail_status' ? outreachLabels[filter.value as keyof typeof outreachLabels] : undefined
  return `${title} ${op} ${value ?? filter.value ?? ''}`.trim()
}
export function viewSummary(config: ViewConfig) {
  const q = viewQuery(config)
  const parts = [q.keyword && `搜索：${q.keyword}`, q.country && `地区：${q.country}`, q.stage && `阶段：${stages[q.stage as keyof typeof stages] ?? q.stage}`,
    q.level && `等级：${q.level}`, q.customer_type && `类型：${q.customer_type}`, q.source && `来源：${q.source}`,
    q.quick_view && ({ priority: '重点客户', sample: '样品客户', in_progress: '跟进中' }[q.quick_view] ?? q.quick_view),
    q.filter && filterText(q.filter), q.archive !== 'active' && (q.archive === 'all' ? '包含归档客户' : '仅归档客户')].filter(Boolean)
  return (parts.join('；') || '全部有效客户') + ` · ${config.layout === 'board' ? '看板' : '表格'} · ${config.density === 'compact' ? '紧凑' : '标准'}行高`
}
