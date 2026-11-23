import { invoke } from '@tauri-apps/api/core'

export const PAGE_SIZE = 20

export const stages = {
  prospect: '待开发', contacted: '已联系', replied: '已回复', requirements: '需求沟通',
  quote_sample: '报价或样品', sample_in_progress: '样品推进中', lost: '客户流失', negotiation: '商务谈判', won: '已成交', paused: '暂停', excluded: '不开发',
} as const
export type Stage = keyof typeof stages

export const activityTypes = {
  note: '备注', call: '电话', meeting: '会议', chat: '即时沟通', sample: '样品', other: '其他',
  email_sent: '邮件发送', email_received: '邮件回复',
} as const
export type ActivityType = keyof typeof activityTypes
export const editableActivityTypes = ['note', 'call', 'meeting', 'chat', 'sample', 'other'] as const

export const methodKinds = { email: '邮箱', phone: '电话', whatsapp: 'WhatsApp', wechat: '微信', other: '其他' } as const
export type MethodKind = keyof typeof methodKinds
export const leadStatuses = { pending: '待审核', rejected: '已拒绝', promoted: '已转正' } as const
export type LeadStatus = keyof typeof leadStatuses

export type Method = {
  id?: string
  kind: MethodKind
  value: string
  label: string
  is_primary: boolean
  is_verified: boolean
}

export type Interest = {
  id?: string
  product_text: string
  intent_quantity: number | null
  quantity_is_estimate: boolean
  requirement: string
}

export type ContactInput = {
  display_name: string
  company_name: string
  country_region: string
  customer_type: string
  customer_source: string
  customer_level: string
  stage: Stage
  first_contact_channel: string
  primary_language: string
  main_need: string
  sample_status: string
  bulk_order_status: string
  first_contact_at: number | null
  last_contact_at: number | null
  website_url: string
  alibaba_buyer_id: string
  notes: string
  methods: Method[]
  interests: Interest[]
}

export type ContactRow = Pick<ContactInput, 'display_name' | 'company_name' | 'country_region' | 'stage' | 'customer_level' | 'last_contact_at'> & {
  id: string
  sample_status: string
  email: string | null
  archived_at: number | null
  source: string
}

export type Contact = {
  id: string
  input: ContactInput
  archived_at: number | null
  source: string
  observed_first_contact_at: number | null
  observed_last_contact_at: number | null
}

export type ContactQuery = {
  keyword: string
  quick_view: '' | 'priority' | 'sample'
  country: string
  stage: string
  level: string
  customer_type: string
  source: string
  archive: 'active' | 'archived' | 'all'
  sort: 'updated' | 'name' | 'last_contact'
  offset: number
}

export type LeadInput = {
  display_name: string
  company_name: string
  country_region: string
  email: string
  website_url: string
  business_summary: string
  source_urls: string[]
  research_notes: string
  confidence: number | null
}

export type Lead = {
  id: string
  input: LeadInput
  review_status: LeadStatus
  promoted_contact_id: string | null
  source: string
}

export type LeadRow = Pick<LeadInput, 'display_name' | 'company_name' | 'country_region' | 'email'> &
  Pick<Lead, 'id' | 'review_status' | 'source'>

export type ActivityInput = {
  contact_id: string
  event_type: ActivityType
  occurred_at: number
  summary: string
  details: string
  channel: string
  followup_category: string
  customer_feedback: string
}

export type Activity = ActivityInput & { id: string; contact_name: string; source: string }
export type Duplicate = { id: string; entity: 'contact' | 'lead'; display_name: string; reason: string }

type Result =
  | { kind: 'contacts'; rows: ContactRow[]; has_more: boolean }
  | { kind: 'contact'; record: Contact }
  | { kind: 'leads'; rows: LeadRow[]; has_more: boolean }
  | { kind: 'lead'; record: Lead; duplicates: Duplicate[] }
  | { kind: 'activities'; rows: Activity[]; has_more: boolean }
  | { kind: 'saved'; id: string }
  | { kind: 'duplicates'; matches: Duplicate[] }

type Command =
  | { action: 'list_contacts'; query: ContactQuery }
  | { action: 'get_contact' | 'get_lead'; id: string }
  | { action: 'save_contact'; id: string | null; input: ContactInput; allow_duplicate: boolean }
  | { action: 'set_archived'; id: string; archived: boolean }
  | { action: 'list_leads'; offset: number; review_status: string }
  | { action: 'save_lead'; id: string | null; input: LeadInput; allow_duplicate: boolean }
  | { action: 'set_lead_review'; id: string; rejected: boolean }
  | { action: 'promote_lead'; id: string; allow_duplicate: boolean }
  | { action: 'list_activities'; contact_id: string | null; event_type: string; offset: number }
  | { action: 'save_activity'; id: string | null; input: ActivityInput }

export type WriteResult = Extract<Result, { kind: 'saved' | 'duplicates' }>

export async function request<Kind extends Result['kind']>(
  command: Command,
  ...expected: Kind[]
): Promise<Extract<Result, { kind: Kind }>> {
  const result = await invoke<Result>('crm_request', { command })
  if (!expected.some(kind => kind === result.kind)) throw new Error('工作台收到意外的 Core 响应。')
  return result as Extract<Result, { kind: Kind }>
}

const fieldLabels: Record<string, string> = {
  display_name: '姓名或客户名称', company_name: '公司', country_region: '国家或地区',
  stage: '客户阶段', method_kind: '渠道类型', method_value: '联系账号', method_label: '渠道备注',
  duplicate_method: '存在重复的联系渠道', primary_method: '每种渠道只能选一个主要账号',
  website_url: '网站须为有效的 HTTP 或 HTTPS 地址', source_urls: '来源网址须为有效的 HTTP 或 HTTPS 地址',
  email: '邮箱地址', description: '描述', intent_quantity: '意向数量', product_text: '型号或品类',
  requirement: '采购要求', occurred_at: '发生时间', summary: '跟进摘要', details: '跟进详情',
  notes: '备注', research_notes: '调查备注', business_summary: '业务摘要', confidence: '置信度须为 0 到 100',
  first_contact_at: '首次联系时间', last_contact_at: '最近联系时间须晚于或等于首次联系时间',
  collection_limit: '渠道最多 12 个，产品兴趣最多 20 项',
}

const messages: Record<string, string> = {
  crm_not_found: '记录不存在，请重新打开列表。',
  crm_read_only: '此记录的当前状态不允许修改。',
  invalid_request: 'Core 版本不支持该操作。请关闭旧版 FTW 和 Core 后重试。',
  database_unavailable: '数据库暂时不可用，请查看 Core 日志。',
  core_timeout: 'Core 响应超时，请重新检查列表中的记录再决定是否重试。',
  core_missing: '找不到同目录下的 ftw-core.exe。',
  profile_unavailable: '无法打开数据目录。',
  core_start_failed: 'Core 启动失败，请查看错误详情。',
  pipe_unavailable: '本机 Core 连接不可用。',
}

export function errorMessage(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error) {
    const code = String(error.code)
    const detail = 'detail' in error ? String(error.detail) : ''
    if (code === 'crm_invalid_field') {
      return '请检查：' + (fieldLabels[detail] ?? detail)
    }
    const message = messages[code] ?? '操作失败。'
    return detail ? message + ' ' + detail : message
  }
  return error instanceof Error ? error.message : String(error)
}

const dateFormatter = new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' })

export function dateLabel(epoch: number | null) {
  return epoch === null ? '—' : dateFormatter.format(new Date(epoch * 1000))
}

export function dateInput(epoch: number | null) {
  if (epoch === null) return ''
  const date = new Date(epoch * 1000)
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16)
}

export function fromDateInput(value: string) {
  return value ? Math.floor(new Date(value).getTime() / 1000) : null
}

export function sourceLabel(source: string) {
  return source === 'agent' ? 'Agent' : source === 'system' ? '系统' : '手动'
}
