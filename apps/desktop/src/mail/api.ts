import { invoke } from '@tauri-apps/api/core'

export type Account = { id: string; name: string; email: string; host?: string; port?: number; security?: 'tls' | 'starttls'; daily_limit?: number; signature?: string }
export type Attachment = { filename: string; content_base64: string }
export type DraftInput = {
  contact_id: string; account_id: string; recipient: string; subject: string; body: string
  attachments: Attachment[]; replace_attachments: boolean
}

export const states = {
  draft: '草稿', pending_approval: '待人工审批', approved: '已批准',
  queued: '等待发送', sending: '发送中', smtp_accepted: 'SMTP 已接受', failed: '发送失败', delivery_unknown: '发送结果未知',
} as const

export type DraftRow = {
  id: string; contact_id: string; account_id: string; recipient: string; subject: string
  state: keyof typeof states; revision: number; source: string; updated_at: number
}
export type Draft = DraftRow & {
  sender: string; body: string; attachments: { filename: string; size: number; sha256: string }[]
  expires_at: number | null; outcome: string | null; thread_id: string | null; has_reply: boolean
}

type AccountInput = { name: string; email: string; host: string; port: number; security: 'tls' | 'starttls'; username: string; password: string }

export type MailCommand =
  | { action: 'list_accounts' }
  | ({ action: 'save_account' } & AccountInput)
  | { action: 'set_account_preferences'; id: string; daily_limit: number; signature: string }
  | { action: 'replace_password'; id: string; password: string }
  | { action: 'test_account' | 'get_draft' | 'request_approval' | 'open_approval' | 'send_approved'; id: string }
  | { action: 'list_drafts'; contact_id: string | null; offset: number }
  | { action: 'save_draft'; id: string | null; input: DraftInput }
  | { action: 'set_suppression'; target: string; blocked: boolean }

type MailResult =
  | { kind: 'accounts'; rows: Account[] }
  | { kind: 'drafts'; rows: DraftRow[]; has_more: boolean }
  | { kind: 'draft'; record: Draft }
  | { kind: 'saved'; id: string }
  | { kind: 'approval_opened'; id: string }
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
  imap_busy: '收件同步正在进行，请稍后再试。',
  imap_recent_sync: '刚刚完成了一次同步，请稍候。',
  imap_not_configured: '请先配置该账户的 IMAP 设置。',
  imap_connection_failed: '无法连接 IMAP 服务器。',
  imap_tls_failed: 'IMAP TLS 握手或证书校验失败。',
  imap_login_failed: 'IMAP 登录失败，请核对登录名和授权码。',
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
  smtp_connection_failed: 'SMTP 连接或认证失败，请核对服务器、端口、加密方式和授权码。',
  mail_not_found: '草稿或邮箱账户不存在。',
  mail_read_only: '当前邮件状态不允许修改。',
  approval_required: '需在 FTW 工作台人工批准这封草稿。',
  approval_already_open: '已有一个邮件审批对话框正在等待确认。',
  approval_expired: '批准已过期，请重新审核。',
  approval_snapshot_changed: '邮件内容已变化，请重新审核。',
  recipient_not_on_contact: '收件邮箱须属于未归档客户的联系渠道。',
  recipient_suppressed: '收件邮箱或域名在拒发名单中。',
  invalid_attachments: '附件名称或内容无效。',
  attachment_limit: '最多选择 4 个附件，总大小上限 384 KiB。',
  smtp_rejected: 'SMTP 服务器明确拒绝了提交。',
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
