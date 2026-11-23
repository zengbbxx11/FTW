import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateLabel, sourceLabel } from '../crm/api'
import { ConfirmButton, Empty, Field, Pagination } from '../crm/components'

type Summary = { title: string; detail: string; occurred_at: number | null }
type AuditRow = { occurred_at: number; actor: string; source: string; entity: string; entity_id: string; operation: string }
type Backup = { filename: string; bytes: number; created_at: number }
type Reply =
  | { kind: 'dashboard'; pending: number; unmatched: number; recent: Summary[]; issues: Summary[] }
  | { kind: 'backups'; directory: string; rows: Backup[] }
  | { kind: 'file'; path: string }
  | { kind: 'restored'; safety_backup: string }
  | { kind: 'audit'; rows: AuditRow[]; has_more: boolean }
  | { kind: 'import_report'; accepted: number; rejected: number; issues: { row: number; reason: string }[]; applied: boolean }

type Command =
  | { action: 'dashboard' | 'backups' | 'create_backup' | 'export_contacts' }
  | { action: 'restore'; filename: string }
  | { action: 'audit'; offset: number }
  | { action: 'import_contacts'; csv: string; apply: boolean; default_stage: string }

async function send<K extends Reply['kind']>(command: Command, expected: K): Promise<Extract<Reply, { kind: K }>> {
  const reply = await invoke<Reply>('data_request', { command })
  if (reply.kind !== expected) throw new Error('收到意外的数据操作响应。')
  return reply as Extract<Reply, { kind: K }>
}

const messages: Record<string, string> = {
  backup_mail_busy: '邮件发送、收取或审批正在进行，请稍后恢复。',
  backup_restore_failed: '无法恢复此备份，请检查文件及版本。',
  backup_create_failed: '创建一致性备份失败，请检查目录权限与空间。',
  backup_list_failed: '无法读取备份目录。',
  csv_size_limit: '请选择不超过 512 KiB 的 UTF-8 CSV。',
  csv_row_limit: '每批最多 500 条，请拆成多个 CSV 文件。',
  csv_name_column_required: '需要「客户名称」或「name」这一列。',
  csv_invalid: '无法读取 CSV 表头。',
  csv_export_failed: '导出失败，请检查数据目录权限。',
  database_unavailable: '数据服务暂时不可用。',
}

function ErrorBox({ error }: { error: unknown }) {
  const code = typeof error === 'object' && error && 'code' in error ? String(error.code) : String(error)
  return <p className="alert error" role="alert">{messages[code] ?? code}</p>
}

export function Dashboard({ mail, inbox }: { mail: () => void; inbox: () => void }) {
  const query = useQuery({ queryKey: ['dashboard'], queryFn: () => send({ action: 'dashboard' }, 'dashboard') })
  return <>
    <div className="page-heading"><h1>工作台</h1></div>
    {query.isPending && <Empty>正在读取工作台…</Empty>}
    {query.isError && <ErrorBox error={query.error} />}
    {query.data && <>
      <div className="panel-columns">
        <section className="panel"><h2>待审核邮件</h2><button className="button" onClick={mail}>{query.data.pending} 封 · 邮件中心</button></section>
        <section className="panel"><h2>待关联来信</h2><button className="button" onClick={inbox}>{query.data.unmatched} 封 · 收件箱</button></section>
      </div>
      <section className="panel"><h2>最近客户活动</h2>
        {query.data.recent.length === 0 ? <p className="muted">尚无客户活动。</p> : <ul className="record-list">
          {query.data.recent.map((row, index) => <li key={index}><strong>{row.title}</strong><span>{row.detail}</span><small>{dateLabel(row.occurred_at)}</small></li>)}
        </ul>}
      </section>
      {query.data.issues.length > 0 && <section className="panel"><h2>邮箱同步异常</h2>
        <ul className="record-list">{query.data.issues.map((row, index) =>
          <li key={index}><strong>{row.title}</strong><span>{row.detail}</span><small>{dateLabel(row.occurred_at)}</small></li>)}</ul>
      </section>}
    </>}
  </>
}

export function DataTools({ initialTab = 'backup' }: { initialTab?: 'backup' | 'csv' | 'audit' }) {
  const [tab, setTab] = useState(initialTab)
  return <>
    <div className="page-heading"><h1>数据与审计</h1></div>
    <div className="customer-views">
      <button className="button" aria-pressed={tab === 'backup'} onClick={() => setTab('backup')}>备份与恢复</button>
      <button className="button" aria-pressed={tab === 'csv'} onClick={() => setTab('csv')}>客户 CSV</button>
      <button className="button" aria-pressed={tab === 'audit'} onClick={() => setTab('audit')}>操作审计</button>
    </div>
    {tab === 'backup' ? <Backups /> : tab === 'csv' ? <Transfer /> : <Audit />}
  </>
}

function Backups() {
  const client = useQueryClient()
  const rows = useQuery({ queryKey: ['backups'], queryFn: () => send({ action: 'backups' }, 'backups') })
  const action = useMutation({
    mutationFn: async (filename: string | undefined) => filename ? send({ action: 'restore', filename }, 'restored') : send({ action: 'create_backup' }, 'file'),
    onSuccess: async reply => {
      if (reply.kind === 'restored') await client.invalidateQueries()
      else await client.invalidateQueries({ queryKey: ['backups'] })
    },
  })
  return <section className="panel">
    <div className="section-heading"><h2>本地 SQLite 备份</h2>
      <button className="button primary" disabled={action.isPending} onClick={() => action.mutate(undefined)}>创建备份</button></div>
    <p className="muted">备份包含客户及邮件数据，请妥善保管。跨 Windows 用户或电脑恢复后，重新保存邮箱授权码。</p>
    {rows.data && <p className="long-text">{rows.data.directory}</p>}
    {rows.isError && <ErrorBox error={rows.error} />}
    {action.isError && <ErrorBox error={action.error} />}
    {action.isPending && <p className="muted" role="status">正在处理数据库…</p>}
    {action.data?.kind === 'file' && <p className="long-text" role="status">已创建 {action.data.path}</p>}
    {action.data?.kind === 'restored' && <p className="long-text" role="status">已恢复。恢复前的安全备份 {action.data.safety_backup}</p>}
    {rows.data?.rows.length === 0 && <Empty>尚无备份。</Empty>}
    {rows.data && <ul className="record-list">{rows.data.rows.map(row => <li key={row.filename}>
      <strong>{row.filename}</strong><small>{dateLabel(row.created_at)} · {row.bytes.toLocaleString()} 字节</small>
      <ConfirmButton label={'用 ' + row.filename + ' 替换当前工作台数据？FTW 会先生成安全备份。'} disabled={action.isPending}
        onConfirm={() => action.mutate(row.filename)}>恢复此备份</ConfirmButton>
    </li>)}</ul>}
  </section>
}

function Transfer() {
  const [csv, setCsv] = useState('')
  const [filename, setFilename] = useState('')
  const [stage, setStage] = useState('prospect')
  const [fileError, setFileError] = useState('')
  const cache = useQueryClient()
  const preview = useMutation({
    mutationFn: (apply: boolean) => send({ action: 'import_contacts', csv, apply, default_stage: stage }, 'import_report'),
    onSuccess: async reply => { if (reply.applied) await cache.invalidateQueries({ queryKey: ['contacts'] }) },
  })
  const exportCsv = useMutation({ mutationFn: () => send({ action: 'export_contacts' }, 'file') })
  const select = async (file: File | undefined) => {
    preview.reset()
    setFileError('')
    setCsv('')
    setFilename('')
    if (!file) return
    if (file.size > 512 * 1024) { setFileError('文件超过 512 KiB，请拆成多个 CSV。'); return }
    try {
      setCsv(await file.text())
      setFilename(file.name)
    } catch { setFileError('无法读取 CSV 文件。') }
  }
  return <>
    <section className="panel"><h2>导入客户</h2>
      <p className="muted">选择从 AppFlowy 或其他工具导出的 UTF-8 CSV。FTW 预览字段并跳过重复和无效记录。</p>
      <div className="form-grid">
        <Field label="客户 CSV"><input accept=".csv,text/csv" type="file" disabled={preview.isPending} onChange={e => void select(e.target.files?.[0])} /></Field>
        <Field label="阶段留空时"><select value={stage} disabled={preview.isPending} onChange={e => { setStage(e.target.value); preview.reset() }}>
          <option value="prospect">待开发</option><option value="won">已成交</option>
        </select></Field>
      </div>
      {fileError && <ErrorBox error={fileError} />}
      {preview.isError && <ErrorBox error={preview.error} />}
      {preview.data && <div className="alert warning" role="status">
        {preview.data.applied ? '已导入' : '预览可导入'} {preview.data.accepted} 条，跳过 {preview.data.rejected} 条。
        <ul>{preview.data.issues.map(issue => <li key={issue.row}>第 {issue.row} 行 · {issue.reason}</li>)}</ul>
      </div>}
      <div className="form-footer">
        <button className="button" disabled={!csv || preview.isPending} onClick={() => preview.mutate(false)}>预览导入</button>
        {preview.data && !preview.data.applied && preview.data.accepted > 0 && <ConfirmButton disabled={preview.isPending}
          label={'把 ' + filename + ' 中的可导入客户写入 FTW？'} onConfirm={() => preview.mutate(true)}>确认导入</ConfirmButton>}
      </div>
    </section>
    <section className="panel"><h2>导出未归档客户</h2>
      <button className="button" disabled={exportCsv.isPending} onClick={() => exportCsv.mutate()}>生成 CSV</button>
      {exportCsv.isError && <ErrorBox error={exportCsv.error} />}
      {exportCsv.data && <p className="long-text" role="status">{exportCsv.data.path}</p>}
    </section>
  </>
}

function Audit() {
  const [offset, setOffset] = useState(0)
  const rows = useQuery({ queryKey: ['audit', offset], queryFn: () => send({ action: 'audit', offset }, 'audit') })
  return <section className="panel"><h2>操作审计</h2>
    {rows.isError && <ErrorBox error={rows.error} />}
    {rows.data?.rows.length === 0 && <Empty>尚无操作记录。</Empty>}
    {!!rows.data?.rows.length && <div className="table-wrap"><table>
      <thead><tr><th>时间</th><th>来源</th><th>操作者</th><th>对象</th><th>操作</th></tr></thead>
      <tbody>{rows.data.rows.map((row, i) => <tr key={i}>
        <td>{dateLabel(row.occurred_at)}</td><td>{sourceLabel(row.source)}</td><td>{row.actor}</td>
        <td>{row.entity}<small>{row.entity_id}</small></td><td>{row.operation}</td>
      </tr>)}</tbody>
    </table></div>}
    {rows.data && <Pagination offset={offset} more={rows.data.has_more} change={setOffset} />}
  </section>
}
