import { invoke } from '@tauri-apps/api/core'
import type { CustomField, FieldDefinition, FieldEntry, FieldPatch, FieldValue, FieldValues } from './fieldTypes'
import type { CellTarget, Filter, SavedView, ViewInput } from './viewTypes'

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

export const outreachLabels = {
  not_started: '未开发', pending: '待发送', sent_no_reply_seen: '已发 · 暂未同步到回复',
  sent_reply_pending_sync: 'SMTP 已接受 · 回复待同步', replied: '已回复', failed: '发送失败',
  delivery_unknown: '提交结果未知', bounced: '已退信', do_not_contact: '拒发名单',
} as const

export type Outreach = {
  status: keyof typeof outreachLabels; last_sent_at: number | null; last_reply_at: number | null
  outreach_count: number; recent_subject: string | null; thread_id: string | null
  sync_outcome: string | null; sync_at: number | null
}

export type DevelopmentOverview = {
  id: string; record_revision: number; data_epoch: string; display_name: string; company_name: string
  country_region: string; stage: string; customer_level: string; email: string | null; mail_outreach: Outreach
  sample_status: string; main_need: string; main_need_truncated: boolean; product_interests: string[]; interests_has_more: boolean
  activities: { id: string; event_type: string; summary: string; occurred_at: number }[]
  related_contacts: { id: string; display_name: string; company_name: string; reason: string; last_contact_at: number | null; mail_status: Outreach['status'] }[]
  related_has_more: boolean
}

export type ContactRow = Pick<ContactInput, 'display_name' | 'company_name' | 'country_region' | 'stage' | 'customer_level' | 'last_contact_at'> & {
  id: string
  sample_status: string
  record_revision: number
  data_epoch: string
  primary_interest: string
  mail_outreach: Outreach
  custom_values: FieldEntry[]
  email: string | null
  archived_at: number | null
  source: string
}

export type Contact = {
  mail_outreach: Outreach
  id: string
  record_revision: number
  data_epoch: string
  effective_first_contact_at: number | null
  effective_last_contact_at: number | null
  latest_thread_reply_at: number | null
  updated_at: number
  input: ContactInput
  archived_at: number | null
  source: string
  observed_first_contact_at: number | null
  observed_last_contact_at: number | null
  merged_into?: string | null
}

export type ContactQuery = {
  filter?: Filter | null
  order?: Array<{ field: string; descending: boolean }>
  custom_columns?: string[]
  page_size?: 20 | 50 | 100
  group?: [string, string | null] | null
  keyword: string
  quick_view: '' | 'priority' | 'sample' | 'in_progress'
  country: string
  stage: string
  level: string
  sample_status?: string
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
  record_revision: number
  data_epoch: string
  input: LeadInput
  review_status: LeadStatus
  promoted_contact_id: string | null
  source: string
  created_at: number
}

export type LeadVersion = { revision: number; epoch: string }

export type LeadRow = Pick<LeadInput, 'display_name' | 'company_name' | 'country_region' | 'email'> &
  Pick<Lead, 'id' | 'review_status' | 'source'>

type LeadWorkspaceRow = LeadRow & Pick<LeadInput, 'website_url' | 'business_summary'> & { evidence_count: number }
export type LeadEvidenceInput = { subject: string; url: string; note: string; observed_at: number | null }
export type EvidenceStatus = 'pending' | 'verified' | 'rejected'
export type LeadEvidence = LeadEvidenceInput & {
  id: string; source: string; request_id: string | null; created_at: number; status: EvidenceStatus
  reviewed_at: number | null; revision: number
  review_source?: 'manual' | 'agent'
  candidate?: string
}

export type ActivityInput = {
  counts_as_contact?: boolean | null
  contact_id: string
  event_type: ActivityType
  occurred_at: number
  summary: string
  details: string
  channel: string
  followup_category: string
  customer_feedback: string
  sample?: { model: string; quantity: number | null; batch_ref?: string; progress?: string }
}

export type Activity = ActivityInput & { id: string; revision: number; contact_name: string; source: string }
type ContactRevision = {
  revision: number; actor_id: string; actor_name: string; source: string; occurred_at: number
  fields: { field: string; before: string | number | null; after: string | number | null }[]
}
type CustomFieldChange = {
  revision: number; field_id: string; field_name: string; actor_id: string; actor_name: string
  source: string; task_id: string | null; before: FieldValue | null; after: FieldValue | null; occurred_at: number
}
export type Duplicate = { id: string; entity: 'contact' | 'lead'; display_name: string; reason: string }

export type AgentChange = {
  id: string; contact_id: string; contact_name: string; field: string; previous_value: string;
  current_value: string; proposed_value: string; request_id: string; state: string; created_at: number
  conflict: boolean; archived: boolean
  evidence_kind: 'research' | 'inferred' | 'unverified' | null
  evidence_url: string | null; evidence_note: string | null
}

export type SampleRow = {
  id: string; contact_id: string; contact_name: string; company_name: string; country_region: string;
  status: string; model: string; quantity: number | null; summary: string; details: string; feedback: string; occurred_at: number | null
  batch_ref: string; events: number | null; activity_id: string | null
}

export type QualityIssue = {
  category: string; contact_id: string; contact_name: string; other_id: string | null; other_name: string | null; detail: string; draft_id: string | null
}

export type CompanyContact = Pick<ContactRow, 'id' | 'display_name' | 'email' | 'stage' | 'last_contact_at'> & {
  mail_status: Outreach['status']
  country_region: string; branch: string
}
export type CompanyProfileInput = { name: string; website: string; notes: string; aliases: string[] }
export type CompanyProfile = CompanyProfileInput & CellTarget

export type StateOption = { field: string; code: string; label: string; aliases: string[] }

type Result =
  | { kind: 'views'; views: SavedView[] }
  | { kind: 'cells_updated'; count: number }
  | { kind: 'bulk_contacts'; count: number; fingerprint: string }
  | { kind: 'fields'; fields: CustomField[] }
  | { kind: 'field_values'; record: FieldValues }
  | { kind: 'state_catalog'; options: StateOption[] }
  | { kind: 'agent_changes'; rows: AgentChange[]; has_more: boolean; total: number; pending: number; applied: number; conflicts: number }
  | { kind: 'sample_workspace'; rows: SampleRow[]; counts: Array<[string, number]>; has_more: boolean }
  | { kind: 'customer_quality'; rows: QualityIssue[]; counts: Array<[string, number]>; active_customers: number; has_more: boolean }
  | { kind: 'company_contacts'; rows: CompanyContact[]; total: number; contacted: number; has_more: boolean; profile: CompanyProfile | null; branch: string }
  | { kind: 'company_profiles'; rows: CompanyProfile[]; has_more: boolean }
  | { kind: 'customer_stats'; active: number; priority: number; sample: number; in_progress: number; won: number }
  | { kind: 'contacts'; rows: ContactRow[]; has_more: boolean }
  | { kind: 'contact_preview'; rows: ContactRow[]; total: number }
  | { kind: 'contact'; record: Contact }
  | { kind: 'development_overview'; record: DevelopmentOverview }
  | { kind: 'leads'; rows: LeadRow[]; has_more: boolean }
  | { kind: 'lead_workspace'; rows: LeadWorkspaceRow[]; has_more: boolean }
  | { kind: 'lead_evidence'; rows: LeadEvidence[]; total: number; verified: number; pending: number; has_more: boolean; epoch: string }
  | { kind: 'lead'; record: Lead; duplicates: Duplicate[] }
  | { kind: 'activities'; rows: Activity[]; has_more: boolean }
  | { kind: 'contact_history'; rows: ContactRevision[]; has_more: boolean }
  | { kind: 'custom_field_history'; rows: CustomFieldChange[]; has_more: boolean }
  | { kind: 'saved'; id: string }
  | { kind: 'duplicates'; matches: Duplicate[] }

type Command =
  | { action: 'list_views'; include_archived?: boolean }
  | { action: 'save_view'; view: ViewInput }
  | { action: 'move_view'; id: string; expected_revision: number; direction: -1 | 1 }
  | { action: 'archive_view'; id: string; expected_revision: number; archived: boolean }
  | { action: 'delete_view'; id: string; expected_revision: number }
  | { action: 'update_cells'; targets: CellTarget[]; field: string; value: string }
  | { action: 'preview_bulk_contacts'; query: ContactQuery }
  | { action: 'update_matching_contacts'; query: ContactQuery; fingerprint: string; field: string; value: string }
  | { action: 'list_fields'; include_archived?: boolean }
  | { action: 'save_field'; definition: FieldDefinition }
  | { action: 'archive_field'; id: string; expected_revision: number; archived: boolean }
  | { action: 'move_field'; id: string; expected_revision: number; direction: -1 | 1 }
  | { action: 'get_field_values'; contact_id: string }
  | { action: 'patch_field_values'; patch: FieldPatch }
  | { action: 'create_contact_with_fields'; input: ContactInput; allow_duplicate: boolean; entries: FieldEntry[]; company?: CellTarget }
  | { action: 'state_catalog' }
  | { action: 'save_contact_checked'; id: string; input: ContactInput; allow_duplicate: boolean; expected_revision: number; expected_epoch: string }
  | { action: 'customer_stats' }
  | { action: 'customer_quality'; category: string; keyword?: string; offset: number }
  | { action: 'company_contacts'; id: string; offset: number }
  | { action: 'associate_company'; anchor: CellTarget; contact: CellTarget }
  | { action: 'find_company_profiles'; keyword: string }
  | { action: 'save_company_profile'; anchor: CellTarget; profile: CellTarget | null; input: CompanyProfileInput; include_group: boolean }
  | { action: 'set_company_membership'; contact: CellTarget; profile: CellTarget | null; branch: string }
  | { action: 'merge_contacts'; primary: CellTarget; secondary: CellTarget }
  | { action: 'list_agent_changes'; offset: number; pending: boolean; scope?: '' | 'pending' | 'applied' | 'conflict' }
  | { action: 'sample_workspace'; keyword: string; status: string; offset: number; by_batch?: boolean }
  | { action: 'resolve_agent_change'; id: string; accept: boolean }
  | { action: 'list_contacts'; query: ContactQuery }
  | { action: 'preview_contacts'; query: ContactQuery }
  | { action: 'get_contact' | 'get_lead'; id: string }
  | { action: 'development_overview'; id: string }
  | { action: 'list_lead_evidence'; lead_id: string; offset: number }
  | { action: 'add_lead_evidence'; lead_id: string; input: LeadEvidenceInput }
  | { action: 'list_contact_evidence'; contact_id: string; offset: number }
  | { action: 'add_contact_evidence'; contact_id: string; input: LeadEvidenceInput }
  | { action: 'review_lead_evidence'; id: string; revision: number; epoch: string; status: EvidenceStatus }
  | { action: 'save_contact'; id: string | null; input: ContactInput; allow_duplicate: boolean }
  | { action: 'set_archived'; id: string; archived: boolean }
  | { action: 'set_contact_archive'; target: CellTarget; archived: boolean }
  | { action: 'list_leads'; offset: number; review_status: string }
  | { action: 'lead_workspace'; offset: number; review_status: string; keyword: string; source: string }
  | { action: 'save_lead'; id: string | null; input: LeadInput; allow_duplicate: boolean; expected: LeadVersion | null }
  | { action: 'set_lead_review'; id: string; rejected: boolean; expected: LeadVersion }
  | { action: 'promote_lead'; id: string; allow_duplicate: boolean; expected: LeadVersion }
  | { action: 'list_activities'; contact_id: string | null; event_type: string; offset: number; keyword?: string; source?: string }
  | { action: 'get_activity'; id: string }
  | { action: 'save_activity'; id: string | null; input: ActivityInput }
  | { action: 'update_activity'; id: string; expected_revision: number; input: ActivityInput }
  | { action: 'contact_history'; id: string; offset: number }
  | { action: 'custom_field_history'; id: string; offset: number }

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
  view_name: '视图名称', view_duplicate: '已有同名视图', view_default_hidden: '默认视图须保持可见',
  view_choose_default: '请先将另一个视图设为默认', view_columns: '列数、列宽或引用字段超出范围',
  view_builtin_delete: '内置视图可隐藏或归档，不能删除。',
  query_field: '视图引用了已归档或不可访问的字段，请编辑视图以修复', query_columns: '每次最多载入 12 个自定义列',
  query_value: '检查筛选条件的值', query_depth: '请减少筛选条件或嵌套分组', query_empty_group: '筛选分组至少需要一个条件',
  field_name: '字段名称', field_name_duplicate: '已有同名的启用字段', field_color: '选择有效的标签颜色',
  field_value: '检查字段类型、数值、日期或选项是否有效', field_url: '网址须为有效的 HTTP 或 HTTPS 地址',
  field_options: '选项数量或字段类型', field_option_label: '选项名称必填且不能重复',
  field_option_archive_required: '已有选项需要归档以保留历史值', field_limit: '自定义字段数量达到上限',
  display_name: '姓名或客户名称', company_name: '公司', country_region: '国家或地区',
  stage: '客户阶段', method_kind: '渠道类型', method_value: '联系账号', method_label: '渠道备注',
  duplicate_method: '存在重复的联系渠道', primary_method: '每种渠道只能选一个主要账号',
  website_url: '网站须为有效的 HTTP 或 HTTPS 地址', source_urls: '来源网址须为有效的 HTTP 或 HTTPS 地址',
  company_aliases: '公司别名最多 20 个且不得重复', company_branch: '分支名称',
  evidence_subject: '证据主题', evidence_url: '来源网址须为有效的 HTTP 或 HTTPS 地址',
  evidence_note: '证据说明', evidence_observed_at: '调查时间', evidence_status: '证据核验状态',
  lead_promotion_review_required: '请先由人工在候选档案中保存已复核的资料',
  contact_method_verification_required: '请由人工核实联系方式',
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
    if (code === 'crm_revision_conflict') return '这条记录已由其他操作更新。本次修改仍在表单中，请核对最新资料后再保存。'
    if (detail === 'field_in_use_by_view') return '此字段仍用于在用视图的筛选、排序或分组。请先调整该视图的配置，再归档字段。'
    if (detail === 'merge_open_mail') return '待归档客户仍有草稿、审批或发送任务，请先处理这些邮件。'
    if (detail === 'company_identity_conflict') return '所选客户已有不同的公司或地区。请在客户资料中核对后再关联。'
    if (detail === 'company_incomplete') return '请先在当前客户资料中补全公司和国家或地区。'
    if (detail === 'company_contact') return '该客户已经在当前公司中。'
    if (detail === 'company_membership_conflict' || detail === 'company_profile_required') return '此联系人已关联独立公司档案。请从其公司工作区核对当前关联。'
    if (detail === 'contact_already_merged') return '这条记录已合并，请打开保留的客户。'
    if (detail === 'agent_change_superseded') return '这个字段在发生冲突后又被修改。请核对最新值后在客户详情中编辑。'
    if (detail === 'bulk_scope_changed') return '匹配客户或资料版本已变化，未执行修改。请重新选择全部匹配客户。'
    if (detail === 'agent_email_already_assigned') return '该邮箱已关联另一位客户。请先核对客户身份。'
    if (detail === 'agent_detail_overwrite' || detail === 'agent_interest_ambiguous') return '产品已有相关资料。请在客户详情中核对原记录。'
    if (code === 'crm_invalid_field') {
      return '请检查：' + (fieldLabels[detail] ?? detail)
    }
    const message = messages[code] ?? '操作失败。'
    return detail ? message + ' ' + detail : message
  }
  return error instanceof Error ? error.message : String(error)
}

const dateFormatter = new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' })
const recentDate = new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
const historicalDate = new Intl.DateTimeFormat('sv-SE', { year: 'numeric', month: '2-digit', day: '2-digit' })

export function shortDate(epoch: number | null) {
  if (epoch === null) return '—'
  const date = new Date(epoch * 1000)
  return (date.getFullYear() === new Date().getFullYear() ? recentDate : historicalDate).format(date)
}

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
