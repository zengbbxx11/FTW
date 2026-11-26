import { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Archive, ArrowRight, CheckCircle2, FolderOpen, RotateCcw, ShieldCheck } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { ConfirmButton, Empty } from '../crm/components'
import { DataError } from './DataError'
import { OpsTag } from './OpsUI'
import { send, type Backup } from './api'

function label(filename: string) {
  if (filename.startsWith('workbench-manual-')) return '手动备份'
  if (filename.startsWith('workbench-before-restore-')) return '恢复前安全快照'
  if (filename.startsWith('workbench-before-v')) return '迁移前安全快照'
  return '本地快照'
}

function fileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`
}

export function RecentBackups({ rows, open }: { rows?: Backup[]; open: () => void }) {
  return <section className="panel data-panel data-recent-backups" aria-label="最近备份">
    <div className="section-heading"><h2>最近备份</h2><button type="button" className="button" onClick={open}><ArrowRight aria-hidden="true" />备份与恢复</button></div>
    {rows?.length ? <ul className="backup-recent-list">{rows.slice(0, 4).map(row => <li key={row.filename}>
      <span className="dashboard-symbol"><Archive aria-hidden="true" /></span><div><strong title={row.filename}>{row.filename}</strong>
        <small>{dateLabel(row.created_at)} · {label(row.filename)}</small></div><span>{fileSize(row.bytes)}</span>
    </li>)}</ul> : <Empty>{rows ? '尚无本地备份。' : '正在读取本地备份…'}</Empty>}
  </section>
}

export function BackupWorkspace({ rows, directory, working, restore }: {
  rows?: Backup[]; directory?: string; working: boolean; restore: (filename: string) => void
}) {
  const [selected, setSelected] = useState('')
  const current = rows?.find(row => row.filename === selected) ?? rows?.[0]
  return <div className="ops-split data-backup-grid">
    <section className="panel ops-table data-backup-list"><div className="section-heading"><h2>本地快照</h2>{rows && <small>{rows.length} 份</small>}</div>
      {rows?.length ? <div className="table-wrap"><table aria-label="本地备份列表"><thead><tr><th>快照文件</th><th>类型</th><th>数据库大小</th></tr></thead>
        <tbody>{rows.map(row => <tr key={row.filename} data-selected={current === row}><td><button type="button" className="text-button" aria-pressed={current === row}
          onClick={() => setSelected(row.filename)}>{row.filename}</button><small>{dateLabel(row.created_at)}</small></td>
          <td>{label(row.filename)}</td><td>{fileSize(row.bytes)}</td></tr>)}</tbody></table></div>
        : <Empty>{rows ? '尚无本地备份，可创建第一份快照。' : '正在读取本地备份…'}</Empty>}
      {directory && <p className="data-directory"><FolderOpen aria-hidden="true" /><code>{directory}</code></p>}
    </section>
    {current ? <BackupDetail key={current.filename} row={current} working={working} restore={() => restore(current.filename)} /> : <Protection />}
  </div>
}

function BackupDetail({ row, working, restore }: { row: Backup; working: boolean; restore: () => void }) {
  const inspect = useMutation({ mutationFn: () => send({ action: 'inspect_backup', filename: row.filename }, 'backup_inspection') })
  const report = inspect.data
  return <section className="panel ops-detail data-backup-detail" aria-label="备份详情">
    <div className="section-heading"><h2><Archive aria-hidden="true" />备份详情</h2><OpsTag tone="teal">{label(row.filename)}</OpsTag></div>
    <h3>{row.filename}</h3>
    <dl className="review-properties"><div><dt>记录时间</dt><dd>{dateLabel(row.created_at)}</dd></div>
      <div><dt>数据库文件</dt><dd>{fileSize(row.bytes)}</dd></div></dl>
    <div className="row-actions">
      <button type="button" className="button" disabled={inspect.isPending || working} onClick={() => inspect.mutate()}>
        <ShieldCheck aria-hidden="true" />{inspect.isPending ? '正在校验…' : '校验备份'}</button>
      <ConfirmButton label={`用「${row.filename}」替换当前工作台数据？FTW 会先校验备份、为当前数据创建安全快照，并将历史自动发信授权切回人工审批。`}
        disabled={working || inspect.isPending} onConfirm={restore}><RotateCcw aria-hidden="true" />恢复此备份</ConfirmButton>
    </div>
    {inspect.isError && <DataError error={inspect.error} />}
    {report && <div className="backup-check review-notice" role="status"><strong><CheckCircle2 aria-hidden="true" />数据库与文件校验通过</strong>
      <dl><div><dt>SQLite Schema</dt><dd>{report.schema_version}</dd></div><div><dt>客户记录</dt><dd>{report.customers}</dd></div>
        <div><dt>邮件记录</dt><dd>{report.mail_records}</dd></div><div><dt>已校验资料对象</dt><dd>{report.objects}</dd></div></dl></div>}
    <p className="muted">恢复前自动保存当前数据。跨 Windows 用户或电脑恢复后，需重新保存邮箱授权码。</p>
  </section>
}

export function Protection() {
  return <section className="panel data-panel data-protection"><div className="section-heading"><h2>数据保护</h2></div>
    <strong><ShieldCheck aria-hidden="true" />本地优先存储</strong>
    <p>备份包含客户、邮件和关联资料，请妥善保管 SQLite 快照及同目录的资料对象。</p>
    <p className="review-notice">恢复时由 Core 校验备份并先为当前数据创建安全快照。</p>
  </section>
}
