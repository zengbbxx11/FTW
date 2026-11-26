import { invoke } from '@tauri-apps/api/core'
import type { ActivityType } from '../crm/api'

export type Summary = { title: string; detail: string; occurred_at: number | null }
export type AuditRow = { occurred_at: number; actor: string; actor_name?: string; task_id?: string; source: string; entity: string; entity_id: string; operation: string }
export type Backup = { filename: string; bytes: number; created_at: number }
export type AuditTarget = { record_type: 'customer' | 'lead' | 'activity' | 'draft' | 'inbox'; id: string }
export type AuditEntry = AuditRow & { id: string; label: string | null; target: AuditTarget | null }
export type AuditFilter = { source: string; entity: string; keyword: string; days: number }
export type CsvScope = 'active' | 'archived' | 'all'

export type DashboardData = {
  kind: 'dashboard'; pending: number; unmatched: number
  stats: { contacts: number; smtp_accepted: number; waiting: number; replied: number; sent_contacts: number }
  recent: (Summary & { id: string; event_type: ActivityType; source: string })[]
  issues: Summary[]; trend: { day: string; accepted: number }[]; agent: AuditRow[]
}

export type SearchCategory = 'all' | 'customer' | 'mail' | 'activity' | 'file'
export type SearchTarget =
  | { record_type: 'customer' | 'inbox' | 'draft' | 'activity'; id: string }
  | { record_type: 'document'; id: string; contact_id: string }
  | { record_type: 'inbox_attachment' | 'outbound_attachment'; id: string; index: number }
export type SearchHit = { target: SearchTarget; category: Exclude<SearchCategory, 'all'>; title: string;
  detail: string; preview: string; occurred_at: number }

export type AgentRunFilter = 'all' | 'active' | 'recorded' | 'attention'
export type AgentRunRecord =
  | { run_type: 'mail'; agent_id: string; agent_name: string; task_id: string; customers: number; jobs: number;
      active: number; accepted: number; failures: number; unknown: number; bounced: number }
  | { run_type: 'request'; agent_id?: string; agent_name?: string; operation: string; request_id: string; result: string;
      outcome: string | null; entity_id: string | null; success: boolean | null; retryable: boolean; requires_human: boolean }
export type AgentRun = { record: AgentRunRecord; state: Exclude<AgentRunFilter, 'all'>; occurred_at: number }
export type AgentMailJob = { id: string; draft_id: string; contact_id: string; contact_name: string; subject: string;
  purpose: string; state: string; outcome: string | null; occurred_at: number; can_cancel: boolean; can_reopen: boolean }
export type MailJobAction = 'cancel' | 'reopen'
export type MailAnalytics = {
  days: number; include_legacy: boolean
  totals: { accepted: number; replied: number; bounced: number; waiting: number; unsynced: number; failed: number;
    unknown: number; latest_sync: number | null }
  trend: { day: string; accepted: number; replied: number }[]
  sources: { source: string | null; accepted: number; replied: number; bounced: number }[]
}

type Reply = DashboardData
  | { kind: 'data_overview'; customers: number; mail_records: number }
  | { kind: 'search'; counts: Record<SearchCategory, number>; rows: SearchHit[] }
  | { kind: 'agent_runs'; counts: Record<AgentRunFilter, number>; rows: AgentRun[] }
  | { kind: 'agent_run_mail'; rows: AgentMailJob[]; has_more: boolean }
  | { kind: 'mail_job_updated'; job_id: string; draft_id: string; draft_revision: number; state: 'cancelled' | 'draft_ready' }
  | { kind: 'mail_analytics'; data: MailAnalytics }
  | { kind: 'backups'; directory: string; rows: Backup[] }
  | { kind: 'backup_inspection'; filename: string; schema_version: number; customers: number; mail_records: number; objects: number }
  | { kind: 'file'; path: string }
  | { kind: 'restored'; safety_backup: string }
  | { kind: 'audit'; rows: AuditRow[]; has_more: boolean }
  | { kind: 'audit_workspace'; rows: AuditEntry[]; entities: string[]; has_more: boolean }
  | { kind: 'import_report'; accepted: number; rejected: number; issues: { row: number; reason: string }[]; applied: boolean;
      mappings: { source: string; field: string }[]; unused_columns: number }

type Command =
  | { action: 'dashboard' | 'data_overview' | 'backups' | 'create_backup' | 'export_contacts' }
  | { action: 'search'; keyword: string; category: SearchCategory; offset: number }
  | { action: 'agent_runs'; filter: AgentRunFilter; keyword: string; offset: number }
  | { action: 'agent_run_mail'; agent_id: string; task_id: string; offset: number }
  | { action: 'update_mail_job'; job_id: string; job_action: MailJobAction }
  | { action: 'mail_analytics' | 'export_mail_analytics'; days: number; include_legacy: boolean }
  | { action: 'restore'; filename: string }
  | { action: 'inspect_backup'; filename: string }
  | { action: 'audit'; offset: number }
  | ({ action: 'audit_workspace'; offset: number } & AuditFilter)
  | { action: 'import_contacts'; csv: string; apply: boolean; default_stage: string }
  | { action: 'export_selected_contacts'; scope: CsvScope; fields: string[] }

export async function send<K extends Reply['kind']>(command: Command, expected: K): Promise<Extract<Reply, { kind: K }>> {
  const reply = await invoke<Reply>('data_request', { command })
  if (reply.kind !== expected) throw new Error('收到意外的数据操作响应。')
  return reply as Extract<Reply, { kind: K }>
}
