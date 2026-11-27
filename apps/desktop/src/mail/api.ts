import { invoke } from '@tauri-apps/api/core'

export type Account = { id: string; name: string; email: string; host?: string; port?: number; security?: 'tls' | 'starttls'; username?: string; daily_limit?: number; signature?: string; is_default?: boolean }
export type Asset = { id: string; filename: string; sha256: string; size: number; content_type: string; agent_allowed: boolean }
export function chooseMailAssets() { return invoke<Asset[]>('choose_mail_assets') }

export type Attachment = { filename: string; content_base64: string }
export type DraftInput = {
  contact_id: string; account_id: string; recipient: string; subject: string; body: string
  attachments: Attachment[]; replace_attachments: boolean
}

export const states = {
  draft: '草稿', pending_approval: '待人工审批', approved: '已批准',
  queued: '等待发送', sending: '发送中', smtp_accepted: 'SMTP 已接受', failed: '发送失败', delivery_unknown: '发送结果未知',
} as const
export type DraftFolder = 'drafts' | 'submissions' | 'accepted' | 'queue' | 'failed' | 'unknown' | 'bounced'

export type DraftRow = {
  id: string; contact_id: string; account_id: string; recipient: string; subject: string
  state: keyof typeof states; revision: number; source: string; updated_at: number; approval_expired: boolean
  purpose: string; job_id: string | null; task_id: string | null; agent_name: string | null
}
export type Draft = DraftRow & {
  account_active: boolean; sender: string; body: string; attachments: { asset_id: string | null; filename: string; size: number; sha256: string }[]
  approval_attempt: { id: string; revision: number; state: 'waiting' | 'approved' | 'cancelled' | 'failed' | 'interrupted'; outcome: string | null } | null
  expires_at: number | null; outcome: string | null; thread_id: string | null; has_reply: boolean; reply_to_inbox_id?: string
}

export type CustomerMailFile = {
  origin: 'inbox' | 'outbound' | 'document'; message_id: string; document_id?: string; index: number; filename: string;
  content_type: string | null; size: number | null; sha256: string | null;
  contact_id: string; contact_name: string; subject: string; occurred_at: number
  created_by?: string; created_by_name?: string; source?: 'manual' | 'agent'
}

export const mailPurposes = {
  first_outreach: '首次开发信', outreach_followup: '开发跟进', quote_communication: '报价沟通',
  sample_communication: '样品沟通', other_business: '其他业务邮件',
} as const
export type Purpose = keyof typeof mailPurposes
export type TemplateInput = { name: string; purpose: Purpose; subject: string; body: string }
export type Template = TemplateInput & { id: string; revision: number; archived_at: number | null }
export type TemplatePreview = { kind: 'rendered_template'; subject: string; body: string; missing: string[] }

export function importCustomerDocuments(contactId: string) { return invoke<number | null>('import_customer_documents', { contactId }) }
export function exportCustomerDocument(id: string, filename: string) { return invoke<boolean>('export_customer_document', { id, filename }) }

type AccountInput = { name: string; email: string; host: string; port: number; security: 'tls' | 'starttls'; username: string; password: string }

export type AgentMailPolicy = {
  revision: number; mode: 'manual' | 'disabled' | 'automatic'; accounts: string[]; purposes: string[]
  daily_limit: number; task_limit: number; interval_seconds: number
}
export type RegisteredAgent = { id: string; name: string; revoked: boolean; automatic: boolean; paused: boolean; last_seen_at: number | null }

export type AgentMailSettings = {
  kind: 'agent_mail_settings'; policy: AgentMailPolicy; automatic_available: boolean
  agents: RegisteredAgent[]


  review: { id: string; state: 'waiting' | 'saved' | 'cancelled' | 'failed'; outcome: string | null } | null
}

export type MailCommand =
  | { action: 'list_templates'; purpose: Purpose | ''; offset: number; archived: boolean }
  | { action: 'get_template'; id: string }
  | { action: 'save_template'; id: string | null; expected_revision: number | null; input: TemplateInput }
  | { action: 'set_template_archive'; id: string; expected_revision: number; archived: boolean }
  | { action: 'render_template'; id: string; contact_id: string }
  | { action: 'preview_template'; input: TemplateInput; contact_id: string }
  | { action: 'agent_mail_settings' | 'list_assets' }
  | { action: 'prepare_draft_assets'; id: string; revision: number }
  | { action: 'customer_document_asset'; contact_id: string; document_id: string }
  | { action: 'list_suppression'; offset: number }
  | { action: 'set_asset_access'; id: string; allowed: boolean }
  | { action: 'save_draft_with_assets'; id: string | null; input: DraftInput; asset_ids: string[] }
  | { action: 'update_draft'; id: string; expected_revision: number; input: DraftInput; asset_ids: string[] | null }
  | { action: 'set_agent_mail_policy'; policy: AgentMailPolicy; agent_ids: string[] }
  | { action: 'cancel_mail_policy_review'; id: string }
  | { action: 'register_mail_agent'; name: string; token: string }
  | { action: 'set_agent_paused'; id: string; paused: boolean }
  | { action: 'revoke_mail_agent'; id: string }
  | { action: 'list_accounts' }
  | ({ action: 'save_account' } & AccountInput)
  | { action: 'update_account'; input: Omit<AccountInput, 'password'> & { id: string; password: string | null } }
  | { action: 'delete_account'; id: string }
  | { action: 'set_default_sender'; id: string }
  | { action: 'set_account_preferences'; id: string; daily_limit: number; signature: string }
  | { action: 'replace_password'; id: string; password: string }
  | { action: 'test_account' | 'get_draft' | 'request_approval' | 'open_approval' | 'send_approved'; id: string }
  | { action: 'list_drafts'; contact_id: string | null; offset: number; folder?: DraftFolder; keyword?: string }
  | { action: 'queue_summary'; contact_id: string | null }
  | { action: 'file_workspace'; contact_id: string | null; keyword: string; direction: string; offset: number; document_id?: string }
  | { action: 'preview_customer_image'; id: string; offset: number }
  | { action: 'save_draft'; id: string | null; input: DraftInput }
  | { action: 'create_reply_draft'; input: { inbox_message_id: string; subject: string | null; body: string }; asset_ids: string[] }
  | { action: 'set_suppression'; target: string; blocked: boolean }

type MailResult =
  | { kind: 'document_chunk'; id: string; content_base64: string; offset: number; next_offset: number | null; size: number; sha256: string }
  | AgentMailSettings
  | { kind: 'templates'; rows: Template[]; has_more: boolean }
  | { kind: 'template'; record: Template }
  | TemplatePreview
  | { kind: 'assets'; rows: Asset[] }
  | { kind: 'suppression'; rows: string[]; has_more: boolean }
  | { kind: 'accounts'; rows: Account[] }
  | { kind: 'drafts'; rows: DraftRow[]; has_more: boolean }
  | { kind: 'queue_summary'; queue: number; accepted: number; failed: number; unknown: number }
  | { kind: 'file_workspace'; rows: CustomerMailFile[]; has_more: boolean }
  | { kind: 'draft'; record: Draft }
  | { kind: 'saved'; id: string }
  | { kind: 'approval_opened'; id: string; attempt_id: string }
  | { kind: 'queued'; id: string; job_id: string; state: string }
  | { kind: 'connection_verified' }

export async function mailCall<Kind extends MailResult['kind']>(
  command: MailCommand, expected: Kind,
): Promise<Extract<MailResult, { kind: Kind }>> {
  const response = await invoke<MailResult>('mail_request', { command })
  if (response.kind !== expected) throw new Error('收到意外的邮件服务响应。')
  return response as Extract<MailResult, { kind: Kind }>
}

const mailErrorLabels: Record<string, string> = {
  invalid_template_purpose: '请选择有效的邮件用途。',
  invalid_template_variable: '模板含有未知或未闭合的变量，请使用右侧列出的变量。',
  template_revision_conflict: '该模板已变更，请重新打开后再编辑。',
  mail_unresolved_template: '主题或正文含未替换的模板变量，请补齐后再提交。',
  agent_policy_cancelled: '用户取消了 Windows 核验。',
  agent_policy_verification_failed: 'Windows 未能验证登录账户。请使用当前 Windows 登录密码，PIN 无法用于此窗口。',
  mail_agent_paused: '这个 Agent 已暂停接入。',
  mail_agent_not_authorized: '这个 Agent 已登记。请在发送模式中勾选它的自动发信权限。',
  mail_agent_selection_required: '请至少选择一个允许自动发信的 Agent。',
  agent_policy_review_expired: '权限确认已过期，请重新发起。',
  agent_policy_changed: '权限设置已更新，请重新打开后再保存。',
  mail_revision_conflict: '草稿已被其他操作更新。请保留所需内容，关闭编辑后重新打开最新草稿。',
  agent_policy_invalid: '请检查发送限额、发件邮箱和授权用途。',
  agent_send_history_unavailable: '发信安全账本不可用，自动发送保持关闭。',
  agent_automatic_unavailable: '当前版本尚未开放自动发送。',
  agent_send_disabled: '工作台已禁止 Agent 发送邮件。',
  agent_policy_not_authorized: '当前 Agent 或邮件用途不在有效授权范围内。',
  agent_task_limit: '已达到该任务的发送数量上限。',
  agent_daily_limit: '已达到 Agent 的发件账户近 24 小时发送上限。',
  outreach_already_recorded: '该邮箱已有开发记录、待发送任务或结果未知的邮件。',
  outreach_requires_history: '该用途需要有可核对的历史邮件。',
  imap_busy: '收件同步正在进行，请稍后再试。',
  imap_recent_sync: '刚刚完成了一次同步，请稍候。',
  imap_not_configured: '请先配置该账户的 IMAP 设置。',
  imap_connection_failed: '无法连接 IMAP 服务器。',
  imap_tls_failed: 'IMAP TLS 握手或证书校验失败。',
  imap_login_failed: 'IMAP 登录失败，请核对登录名和授权码。',
  attachment_destination_unavailable: '无法保存附件。请选择一个新的本地文件名。',
  imap_download_limit: '完整邮件超过 16 MiB，已保留邮件头。',
  imap_download_incomplete: '邮件服务器返回的正文不完整，请稍后重新下载。',
  imap_body_not_loaded: '请先下载完整邮件。',
  imap_uid_changed: '服务器的邮件位置已变化，请先重新同步收件箱。',
  imap_timeout: 'IMAP 请求超时，可重新同步。',
  imap_inbox_unavailable: '无法打开服务器上的 INBOX。',
  invalid_request: '当前 Core 版本过旧，请结束旧版 Core 并打开新版 FTW。',
  invalid_mail_field: '请检查帐号名称、登录名和邮件主题，避免空值或换行。',
  invalid_mail_address: '邮箱地址格式不正确。',
  invalid_mail_body: '正文不能为空，且最多 12,000 UTF-8 字节。',
  invalid_smtp_host_or_domain: '请输入有效的 SMTP 主机名或域名。',
  smtp_configuration_invalid: 'SMTP 端口或加密配置无效。',
  invalid_account_preferences: '发送上限须在 1 到 500 封之间，签名最多 2,000 UTF-8 字节。',
  smtp_daily_limit: '该账户已达到近 24 小时发送上限。',
  restored_without_resend: '从备份恢复，发送结果需要人工核对。',
  invalid_mail_password: '请输入有效的邮箱授权码或密码。',
  credential_unavailable: 'Windows 无法保护或解锁凭据。换机后请重新保存授权码。',
  local_agent_use_pause: '可以暂停本机 Agent 接入。',
  smtp_connection_failed: 'SMTP 连接或认证失败，请核对服务器、端口、加密方式和授权码。',
  mail_not_found: '草稿或邮箱账户不存在。',
  mail_account_busy: '邮箱正在发送邮件，请稍后再修改或删除账户。',
  mail_account_changed: '发件账户已变更，原发送任务已停止，请重新核对邮件。',
  mail_account_removed: '发件账户已删除，原发送任务已停止。',
  mail_read_only: '当前邮件状态不允许修改。',
  approval_window_failed: 'Windows 审批窗口未能正常完成，可重新发起审批。',
  approval_interrupted: '审批过程被中断，可重新发起审批。',
  approval_required: '需在 FTW 工作台人工批准这封草稿。',
  mail_link_changed: '来信归属或处理状态已经变化，已刷新列表。请核对最新状态后重试。',
  mail_selection_invalid: '请选择 1 至 100 封不重复的邮件。',
  mail_already_linked: '这封邮件已经确认关联客户，请通过更正关联处理。',
  mail_reply_link_required: '请先核对这封业务来信的客户关联。',
  mail_reply_header_missing: '这封来信缺少可用于原会话回复的邮件标识。',
  mail_reply_context_changed: '原会话或客户地址已变化，请回到来信重新起草。',
  mail_reply_purpose_invalid: '会话回复须使用已获授权的跟进或业务用途。',
  mail_link_human_review_required: '这封来信需要在工作台人工核对关联。',
  approval_already_open: '已有一个邮件审批对话框正在等待确认。',
  approval_expired: '批准已过期，请重新审核。',
  approval_snapshot_changed: '邮件内容已变化，请重新审核。',
  recipient_not_on_contact: '收件邮箱须属于未归档客户的联系渠道。',
  recipient_suppressed: '收件邮箱或域名在拒发名单中。',
  invalid_attachments: '附件名称或内容无效。',
  attachment_limit: '最多选择 4 个 PDF 或图片附件，合计上限 10 MiB。',
  attachment_type: '仅支持内容与扩展名一致的 PDF、JPEG、PNG 或 WebP。',
  attachment_storage_unavailable: '本地附件无法读取，或文件与已保存的校验值不一致。',
  attachment_not_available: '附件不存在，或尚未授权给 Agent。',
  attachment_picker_failed: '无法打开 Windows 文件选择窗口。',
  smtp_rejected: 'SMTP 服务器明确拒绝了提交。',
  cancelled_before_submission: '发送前已取消。',
  mail_job_action_unavailable: '邮件任务状态已变化，请刷新后核对。',
  document_preview_unavailable: '此文件可另存查看。',
  smtp_result_unknown: 'SMTP 提交结果未知，需要先核对邮箱记录。',
  interrupted_during_submission: '发送过程被中断，结果未知。',
}

export function mailError(error: unknown) {
  if (error && typeof error === 'object' && 'code' in error) {
    const code = String(error.code)
    return mailErrorLabels[code] ?? ('邮件操作失败（' + code + '）')
  }
  const reason = error instanceof Error ? error.message : String(error)
  return mailErrorLabels[reason] ?? reason
}
