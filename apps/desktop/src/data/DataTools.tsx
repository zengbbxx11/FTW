import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Archive, ArrowRight, Bot, ClipboardList, DatabaseBackup, Download, FileSpreadsheet, LayoutDashboard, Mail, Paperclip, Upload, UsersRound } from 'lucide-react'
import { AgentChanges } from './AgentChanges'
import { BackupWorkspace, Protection, RecentBackups } from './Backups'
import { CustomerFiles } from './CustomerFiles'
import { CustomerTransfer, type TransferFocus } from './CustomerTransfer'
import { AuditWorkspace, RecentAudit } from './DataAudit'
import { DataError } from './DataError'
import { OpsStat } from './OpsUI'
import type { CustomerMailFile } from '../mail/api'
import { send, type AuditTarget, type Backup } from './api'
import './data-workspace.css'

export { DataError } from './DataError'
export type DataTab = 'overview' | 'backup' | 'csv' | 'audit' | 'agent-changes' | 'files'

const tabs = [
  ['overview', '总览', LayoutDashboard], ['backup', '备份与恢复', DatabaseBackup], ['csv', '客户 CSV', FileSpreadsheet],
  ['files', '客户附件', Paperclip], ['agent-changes', 'Agent 数据变更', Bot], ['audit', '操作审计', ClipboardList],
] as const

export function DataTools({ initialTab = 'overview', fileContact, fileDocument, openContact, openFileMail, openAuditTarget }: {
  initialTab?: DataTab; fileContact?: string; fileDocument?: string; openContact: (id: string) => void
  openFileMail: (file: CustomerMailFile) => void; openAuditTarget: (target: AuditTarget) => void
}) {
  const [tab, setTab] = useState<DataTab>(initialTab)
  const [transferFocus, setTransferFocus] = useState<TransferFocus>('import')
  const client = useQueryClient()
  const backupsVisible = tab === 'overview' || tab === 'backup'
  const backups = useQuery({ queryKey: ['backups'], enabled: backupsVisible, queryFn: () => send({ action: 'backups' }, 'backups') })
  const action = useMutation({
    mutationFn: async (filename?: string) => filename ? send({ action: 'restore', filename }, 'restored') : send({ action: 'create_backup' }, 'file'),
    onSuccess: async reply => {
      if (reply.kind === 'restored') await client.invalidateQueries()
      else await Promise.all(['backups','audit'].map(key => client.invalidateQueries({ queryKey: [key] })))
    },
  })

  function transfer(focus: TransferFocus) { setTransferFocus(focus); setTab('csv') }

  return <div className={tab === 'files' || tab === 'agent-changes' ? 'data-workspace' : 'ops-workspace data-workspace'}>
    <header className="page-heading"><div><h1>数据与审计</h1><p>本地备份、客户资料导入导出及操作记录</p></div>
      {backupsVisible && <button type="button" className="button primary" disabled={action.isPending} onClick={() => action.mutate(undefined)}>
        <DatabaseBackup aria-hidden="true" />立即备份</button>}
    </header>
    <div className="customer-views data-tool-tabs" role="group" aria-label="数据工作区">
      {tabs.map(([id,label,Icon]) => <button key={id} type="button" className="button" aria-pressed={tab === id}
        onClick={() => id === 'csv' ? transfer('import') : setTab(id)}><Icon aria-hidden="true" />{label}</button>)}
    </div>
    {backupsVisible && backups.isError && <DataError error={backups.error} />}
    {action.isError && <DataError error={action.error} />}
    {action.isPending && <p className="review-notice" role="status">{action.variables ? '正在校验并恢复备份…' : '正在创建一致性备份…'}</p>}
    {action.data?.kind === 'file' && backupsVisible && <p className="review-notice long-text" role="status">
      备份已创建 {tab === 'overview' ? action.data.path.split(/[\\/]/).at(-1) : action.data.path}</p>}
    {action.data?.kind === 'restored' && backupsVisible && <p className="review-notice long-text" role="status">备份已恢复。恢复前的安全快照位于 {action.data.safety_backup}</p>}
    {tab === 'overview' ? <DataOverview backups={backups.data?.rows} openBackup={() => setTab('backup')} openAudit={() => setTab('audit')} transfer={transfer} />
      : tab === 'backup' ? <BackupWorkspace rows={backups.data?.rows} directory={backups.data?.directory} working={action.isPending} restore={name => action.mutate(name)} />
      : tab === 'csv' ? <CustomerTransfer focus={transferFocus} />
      : tab === 'files' ? <CustomerFiles contactId={fileContact} initialDocument={fileDocument} openContact={openContact} openMail={openFileMail} />
      : tab === 'agent-changes' ? <AgentChanges openContact={openContact} openAudit={() => setTab('audit')} />
      : <AuditWorkspace open={openAuditTarget} />}
  </div>
}

function DataOverview({ backups, openBackup, openAudit, transfer }: {
  backups?: Backup[]; openBackup: () => void; openAudit: () => void; transfer: (focus: TransferFocus) => void
}) {
  const stats = useQuery({ queryKey: ['data-overview'], queryFn: () => send({ action: 'data_overview' }, 'data_overview') })
  const latest = backups?.[0]?.created_at
  const latestLabel = latest ? new Intl.DateTimeFormat('zh-CN', { month:'numeric', day:'numeric', hour:'2-digit', minute:'2-digit', hour12:false }).format(latest * 1000) : backups ? '暂无备份' : '—'
  return <>
    {stats.isError && <DataError error={stats.error} />}
    <div className="ops-metrics data-overview-metrics" aria-label="工作区数据概况" aria-busy={stats.isPending}>
      <OpsStat icon={UsersRound} tone="teal" name="客户记录（含归档）" value={stats.data?.customers ?? '—'} note="本地 SQLite" />
      <OpsStat icon={Mail} tone="purple" name="收发邮件记录" value={stats.data?.mail_records ?? '—'} note="SMTP + IMAP" />
      <OpsStat icon={Archive} tone="green" name="最近备份" value={latestLabel} />
    </div>
    <div className="ops-split data-overview-grid">
      <div className="ops-column"><RecentBackups rows={backups} open={openBackup} /><RecentAudit open={openAudit} /></div>
      <div className="ops-column">
        <section className="panel data-panel data-management"><div className="section-heading"><h2>数据管理</h2></div>
          <button type="button" onClick={() => transfer('export')}><span className="dashboard-symbol"><Download aria-hidden="true" /></span>
            <span><strong>导出客户 CSV</strong><small>选择范围与字段</small></span><ArrowRight aria-hidden="true" /></button>
          <button type="button" onClick={() => transfer('import')}><span className="dashboard-symbol"><Upload aria-hidden="true" /></span>
            <span><strong>导入客户 CSV</strong><small>预览字段与重复记录</small></span><ArrowRight aria-hidden="true" /></button>
          <button type="button" onClick={openBackup}><span className="dashboard-symbol"><DatabaseBackup aria-hidden="true" /></span>
            <span><strong>备份与恢复</strong><small>校验快照与关联资料</small></span><ArrowRight aria-hidden="true" /></button>
        </section>
        <Protection />
      </div>
    </div>
  </>
}
