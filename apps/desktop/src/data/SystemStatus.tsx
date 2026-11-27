import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Activity, ClipboardList, Database, Download, Link, Mail, Monitor, RefreshCw, type LucideIcon } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { Empty } from '../crm/components'
import { testImapAccount, useImapAccounts, type ImapAccount } from '../mail/Inbox'
import { mailCall, mailError } from '../mail/api'
import { OpsTag } from './OpsUI'
import { UpdatePanel } from './Updates'

type Health = { coreVersion: string; schemaVersion: number; storageMode: 'portable' | 'local_app_data' | 'development';
  dataRoot: string; processId: number; webviewVersion: string | null; architecture: string }
type McpProbe = { protocol: string; toolCount: number }
type Tone = 'teal' | 'green' | 'orange' | 'purple' | 'red' | 'gray'
type Status = { icon: LucideIcon; name: string; tone: Tone; label: string; detail: string; time: number | null }
type Protocol = 'SMTP' | 'IMAP'
type MailTest = { protocol: Protocol; id: string; label: string }
type MailResult = MailTest & { ok: boolean; message: string; time: number }

const modeLabels = { portable: '便携模式', local_app_data: 'Windows 用户目录', development: '开发目录' }
const errorLabels: Record<string, string> = {
  profile_unavailable: '无法打开数据目录', pipe_unavailable: '无法连接 Core', core_missing: '找不到 Core 程序',
  core_start_failed: 'Core 启动失败', core_timeout: 'Core 响应超时', core_unhealthy: 'Core 无法读取数据库',
  mcp_missing: '找不到 ftw-mcp.exe', mcp_probe_timeout: '本地 MCP 检测超时', mcp_probe_failed: 'MCP 工具交互或查询失败',
  invalid_diagnostics_extension: '请将诊断报告保存为 .json 文件。',
  diagnostics_export_failed: '无法保存诊断报告。请重试并选择可写目录。',
}

function problem(error: unknown) {
  const code = typeof error === 'object' && error !== null && 'code' in error ? String(error.code) : ''
  return errorLabels[code] ?? (code || '连接失败')
}

function syncStatus(accounts: ImapAccount[]) {
  if (!accounts.length) return { tone: 'gray', label: '未配置' } as const
  if (accounts.some(row => row.outcome && !['ok', 'running', 'more_available'].includes(row.outcome))) return { tone: 'red', label: '同步异常' } as const
  if (accounts.some(row => row.outcome === 'running')) return { tone: 'teal', label: '正在同步' } as const
  if (accounts.every(row => row.outcome === 'ok')) return { tone: 'green', label: '已同步' } as const
  if (accounts.some(row => row.outcome === 'more_available')) return { tone: 'orange', label: '待继续同步' } as const
  return { tone: 'gray', label: '同步待确认' } as const
}

export function SystemStatus({ audit, mail }: { audit: () => void; mail: (protocol: Protocol) => void }) {
  const core = useQuery({ queryKey: ['core-health'], queryFn: () => invoke<Health>('core_health') })
  const mcp = useQuery({ queryKey: ['mcp-health'], queryFn: () => invoke<McpProbe>('mcp_status') })
  const smtp = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const imap = useImapAccounts()
  const [target, setTarget] = useState('')
  const [lastTest, setLastTest] = useState<MailResult | null>(null)
  const report = useMutation({ mutationFn: () => invoke<string | null>('export_support_report') })
  const mailCheck = useMutation({ mutationFn: (item: MailTest) => item.protocol === 'SMTP'
    ? mailCall({ action: 'test_account', id: item.id }, 'connection_verified') : testImapAccount(item.id),
    onSuccess: (_, item) => setLastTest({ ...item, ok: true, message: '连接与登录验证通过。', time: Date.now() / 1000 }),
    onError: (error, item) => setLastTest({ ...item, ok: false, message: mailError(error), time: Date.now() / 1000 }),
  })
  const configured = imap.data?.rows.filter(row => row.host) ?? []
  const targets: MailTest[] = [...(smtp.data?.rows ?? []).map(row => ({ protocol: 'SMTP' as const, id: row.id, label: row.name })),
    ...configured.map(row => ({ protocol: 'IMAP' as const, id: row.account_id, label: row.display_name }))]
  const selected = targets.find(row => `${row.protocol}:${row.id}` === target) ?? targets[0]
  const health = core.isSuccess ? core.data : undefined
  const latestSync = Math.max(0, ...configured.map(row => row.last_sync ?? 0)) || null
  const sync = syncStatus(configured)
  const services: Status[] = [
    { icon: Activity, name: 'Rust Core', tone: core.isError ? 'red' : health ? 'green' : 'gray',
      label: core.isError ? '连接失败' : health ? '在线' : '检测中', detail: core.isError ? problem(core.error) : health ? '本地 IPC 健康查询通过' : '正在连接 Core',
      time: (core.errorUpdatedAt > core.dataUpdatedAt ? core.errorUpdatedAt : core.dataUpdatedAt) / 1000 || null },
    { icon: Link, name: 'MCP Server', tone: mcp.isError ? 'red' : mcp.isSuccess ? 'green' : 'gray',
      label: mcp.isError ? '检测失败' : mcp.isSuccess ? '查询通过' : '检测中', detail: mcp.isError ? problem(mcp.error) :
      mcp.isSuccess ? `stdio ${mcp.data.protocol} · 发现 ${mcp.data.toolCount} 个工具` : '正在读取工具并验证客户查询',
      time: Math.max(mcp.errorUpdatedAt, mcp.dataUpdatedAt) / 1000 || null },
    { icon: Mail, name: 'SMTP 发件', tone: smtp.isError ? 'red' : smtp.data?.rows.length ? 'teal' : 'gray',
      label: smtp.isError ? '读取失败' : smtp.isPending ? '检测中' : smtp.data?.rows.length ? '已配置' : '未配置',
      detail: smtp.isError ? mailError(smtp.error) : smtp.data?.rows.length ? `${smtp.data.rows.length} 个发件账户` : '活跃发件账户', time: smtp.dataUpdatedAt / 1000 || null },
    { icon: RefreshCw, name: 'IMAP 收件同步', tone: imap.isError ? 'red' : sync.tone,
      label: imap.isError ? '读取失败' : imap.isPending ? '检测中' : sync.label,
      detail: imap.isError ? mailError(imap.error) : `${configured.length} 个收件账户`, time: latestSync },
    { icon: Database, name: 'SQLite', tone: core.isError ? 'red' : health ? 'green' : 'gray',
      label: core.isError ? '待检查' : health ? '查询通过' : '检测中',
      detail: health ? `数据库 Schema ${health.schemaVersion}` : '等待 Core 健康查询', time: core.dataUpdatedAt / 1000 || null },
  ]
  const refreshing = core.isFetching || mcp.isFetching || smtp.isFetching || imap.isFetching
  function refresh() { void Promise.all([core.refetch(), mcp.refetch(), smtp.refetch(), imap.refetch()]) }

  return <div className="ops-workspace system-status-workspace">
    <header className="page-heading"><div><h1>系统状态</h1><p>本地 Core、MCP 和邮箱连接的检测结果</p></div>
      <button type="button" className="button primary" disabled={refreshing} onClick={refresh}><RefreshCw aria-hidden="true" />{refreshing ? '正在检测…' : '重新检测'}</button></header>
    <div className="ops-metrics" aria-label="服务摘要">{services.slice(0, 4).map(service => <article className="panel system-metric" key={service.name}>
      <div><span className="dashboard-symbol" data-tone={service.tone}><service.icon aria-hidden="true" /></span><OpsTag tone={service.tone}>{service.label}</OpsTag></div>
      <h2>{service.name}</h2><p>{service.detail}</p>
    </article>)}</div>
    <div className="ops-split system-grid">
      <div className="ops-column">
        <section className="panel ops-table"><div className="section-heading"><h2>服务连接状态</h2></div>
          <div className="table-wrap"><table aria-label="服务连接状态"><thead><tr><th>服务</th><th>检测结果</th><th>依据</th><th>记录时间</th></tr></thead>
            <tbody>{services.map(service => <tr key={service.name}><td><span className="system-service"><service.icon aria-hidden="true" />{service.name}</span></td>
              <td><OpsTag tone={service.tone}>{service.label}</OpsTag></td><td>{service.detail}</td><td>{dateLabel(service.time)}</td></tr>)}</tbody>
          </table></div>
        </section>
        <section className="panel system-environment"><div className="section-heading"><h2><Monitor aria-hidden="true" />运行环境</h2></div>
          {health ? <dl><div><dt>系统与架构</dt><dd>Windows · {health.architecture === 'x86_64' ? 'x64' : health.architecture}</dd></div>
            <div><dt>存储模式</dt><dd>{modeLabels[health.storageMode]}</dd></div><div><dt>Core 版本</dt><dd>{health.coreVersion}</dd></div>
            <div><dt>Core 进程 ID</dt><dd>{health.processId}</dd></div><div><dt>SQLite Schema</dt><dd>{health.schemaVersion}</dd></div>
            <div><dt>WebView2</dt><dd>{health.webviewVersion ?? '无法读取版本'}</dd></div><div><dt>数据目录</dt><dd><code>{health.dataRoot}</code></dd></div></dl>
            : <Empty>{core.isError ? 'Core 连接恢复后可查看运行环境。' : '正在读取运行环境…'}</Empty>}
        </section>
      </div>
      <div className="ops-column">
        <section className="panel system-notices"><div className="section-heading"><h2>近期检测与同步</h2></div>
          <ul className="ops-events">
            {(core.isSuccess || core.isError) && <li><span className="dashboard-symbol" data-tone={core.isSuccess ? 'green' : 'orange'}><Activity aria-hidden="true" /></span>
              <div><strong>{core.isSuccess ? 'Core IPC 与 SQLite 查询通过' : 'Core 健康查询失败'}</strong>
                <small>{core.isSuccess ? dateLabel(services[0].time) : problem(core.error)}</small></div></li>}
            {(mcp.isSuccess || mcp.isError) && <li><span className="dashboard-symbol" data-tone={mcp.isSuccess ? 'green' : 'orange'}><Link aria-hidden="true" /></span>
              <div><strong>{mcp.isSuccess ? 'MCP 本地只读客户查询完成' : 'MCP 本地查询失败'}</strong><small>{mcp.isSuccess ? services[1].detail : problem(mcp.error)}</small></div></li>}
            {lastTest && <li role="status"><span className="dashboard-symbol" data-tone={lastTest.ok ? 'green' : 'orange'}><Mail aria-hidden="true" /></span>
              <div><strong>{lastTest.protocol} · {lastTest.label}</strong><small>{lastTest.message}</small><small>{dateLabel(lastTest.time)}</small></div></li>}
            {configured.map(account => {
              const status = syncStatus([account])
              return <li key={account.account_id}><span className="dashboard-symbol" data-tone={status.tone}><RefreshCw aria-hidden="true" /></span>
                <div><strong>{account.display_name} · {status.label}</strong><small>{account.last_sync ? dateLabel(account.last_sync) : '暂无同步时间'}</small>
                  {status.tone === 'red' && account.outcome && <small>{mailError(account.outcome)}</small>}</div></li>
            })}
          </ul>
          {!configured.length && <p className="muted">暂无已配置 IMAP 账户的同步记录。</p>}
        </section>
        <section className="panel system-tools"><div className="section-heading"><h2>快捷诊断</h2></div>
          <div className="system-actions">
            <button type="button" className="button" disabled={core.isFetching} onClick={() => void core.refetch()}><Activity aria-hidden="true" />检测 Core IPC</button>
            <button type="button" className="button" disabled={mcp.isFetching} onClick={() => void mcp.refetch()}><Link aria-hidden="true" />测试 MCP 查询</button>
            <button type="button" className="button" onClick={audit}><ClipboardList aria-hidden="true" />查看操作审计</button>
            <button type="button" className="button" onClick={() => mail('IMAP')}><RefreshCw aria-hidden="true" />收件设置</button>
          </div>
          <div className="system-mail-test">
            {selected ? <><select aria-label="选择要检测的邮箱连接" value={`${selected.protocol}:${selected.id}`} disabled={mailCheck.isPending}
              onChange={event => setTarget(event.target.value)}>{targets.map(item => <option key={`${item.protocol}:${item.id}`} value={`${item.protocol}:${item.id}`}>
                {item.protocol} · {item.label}</option>)}</select>
              <button type="button" className="button" disabled={mailCheck.isPending} onClick={() => mailCheck.mutate(selected)}><Mail aria-hidden="true" />
                {mailCheck.isPending ? '连接检测中…' : '检查邮箱连接'}</button></>
              : <button type="button" className="button" onClick={() => mail('SMTP')}><Mail aria-hidden="true" />发件账户设置</button>}
          </div>
        </section>
        <UpdatePanel />
        <section className="panel system-tools"><div className="section-heading"><h2>故障报告</h2></div>
          <p className="muted">导出程序版本、系统信息和近期错误码。你可以先打开文件查看，再决定是否分享。</p>
          <div className="system-actions">
            <button type="button" className="button" disabled={report.isPending} onClick={() => report.mutate()}>
              <Download aria-hidden="true" />{report.isPending ? '正在生成…' : '导出诊断报告'}
            </button>
          </div>
          {report.isError && <p role="alert">{problem(report.error)}</p>}
          {report.data && <p className="support-report-path" role="status">报告已保存至 <code>{report.data}</code></p>}
        </section>
      </div>
    </div>
  </div>
}
