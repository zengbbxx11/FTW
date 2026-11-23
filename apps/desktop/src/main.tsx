import { useCallback, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { invoke } from '@tauri-apps/api/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ContactDetail, ContactEditor, ContactList } from './crm/Contacts'
import { LeadDetail, LeadEditor, LeadList } from './crm/Leads'
import { Timeline } from './crm/Timeline'
import type { Contact } from './crm/api'
import { MailCenter } from './mail/MailCenter'
import { Dashboard, DataTools } from './data/DataTools'
import { CoreChanges } from './data/CoreChanges'
import './styles.css'

type Health = {
  coreVersion: string
  schemaVersion: number
  storageMode: 'portable' | 'local_app_data' | 'development'
  dataRoot: string
  processId: number
}

type Issue = { code: string; detail: string }

type Check =
  | { status: 'loading' }
  | { status: 'ready'; health: Health; checkedAt: Date }
  | { status: 'error'; issue: Issue }

const modeLabels: Record<Health['storageMode'], string> = {
  portable: '便携模式',
  local_app_data: 'Windows 用户目录',
  development: '开发目录',
}

const errorLabels: Record<string, string> = {
  profile_unavailable: '无法打开数据目录',
  pipe_unavailable: '无法连接 Core',
  core_missing: '找不到 Core 程序',
  core_start_failed: 'Core 启动失败',
  core_timeout: 'Core 响应超时',
  core_unhealthy: 'Core 无法读取数据库',
}

function normalizeIssue(value: unknown): Issue {
  if (typeof value === 'object' && value !== null && 'code' in value && 'detail' in value) {
    return { code: String(value.code), detail: String(value.detail) }
  }
  return { code: 'unknown', detail: String(value) }
}

function SystemStatus() {
  const [check, setCheck] = useState<Check>({ status: 'loading' })

  const refresh = useCallback(async () => {
    setCheck({ status: 'loading' })
    try {
      const health = await invoke<Health>('core_health')
      setCheck({ status: 'ready', health, checkedAt: new Date() })
    } catch (error) {
      setCheck({ status: 'error', issue: normalizeIssue(error) })
    }
  }, [])

  useEffect(() => { void refresh() }, [refresh])

  return (
    <>
        <div className="page-heading">
          <h1>系统状态</h1>
          <button className="refresh" type="button" onClick={() => void refresh()} disabled={check.status === 'loading'}>
            {check.status === 'loading' ? '正在检查…' : '重新检查'}
          </button>
        </div>

        <section className="status-card" aria-label="Core 连接" aria-live="polite">
          <div className="status-header">
            <div className={'status-icon ' + check.status} aria-hidden="true">
              {check.status === 'ready' ? '✓' : check.status === 'error' ? '!' : '·'}
            </div>
            <div>
              <h2>
                {check.status === 'loading' && '正在连接 Core'}
                {check.status === 'ready' && 'Core 已就绪'}
                {check.status === 'error' && (errorLabels[check.issue.code] ?? '健康检查失败')}
              </h2>
              {check.status === 'ready' && (
                <p className="muted">上次检查 {check.checkedAt.toLocaleTimeString('zh-CN', { hour12: false })}</p>
              )}
            </div>
          </div>

          {check.status === 'ready' && (
            <dl className="details">
              <div className="detail-cell">
                <dt>Core 版本</dt>
                <dd>{check.health.coreVersion}</dd>
              </div>
              <div className="detail-cell">
                <dt>数据库版本</dt>
                <dd>{check.health.schemaVersion}</dd>
              </div>
              <div className="detail-cell">
                <dt>存储模式</dt>
                <dd>{modeLabels[check.health.storageMode]}</dd>
              </div>
              <div className="detail-cell">
                <dt>进程 ID</dt>
                <dd>{check.health.processId}</dd>
              </div>
              <div className="detail-cell data-path">
                <dt>数据目录</dt>
                <dd>{check.health.dataRoot}</dd>
              </div>
            </dl>
          )}

          {check.status === 'error' && (
            <p className="error-detail">{check.issue.detail}</p>
          )}
        </section>
    </>
  )
}

type Page =
  | { kind: 'contacts' | 'new-contact' | 'leads' | 'new-lead' | 'activities' | 'system' | 'dashboard' }
  | { kind: 'data'; initialTab?: 'csv' }
  | { kind: 'contact' | 'lead'; id: string }
  | { kind: 'mail'; contact?: Contact; inboxContact?: string; inboxOnly?: boolean }

const navigation: { kind: 'dashboard' | 'contacts' | 'leads' | 'activities' | 'mail' | 'system' | 'data'; label: string; sections: Page['kind'][]; icon: string }[] = [
  { kind: 'dashboard', label: '工作台', sections: ['dashboard'], icon: 'M3 10l9-7 9 7M5 9v11h14V9m-10 11v-7h6v7' },
  { kind: 'contacts', label: '客户', sections: ['contacts','contact','new-contact'], icon: 'M16 20v-2a4 4 0 0 0-4-4H7a4 4 0 0 0-4 4v2m14-6a4 4 0 0 1 4 4v2M9.5 10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7M17 4a3 3 0 0 1 0 6' },
  { kind: 'leads', label: '候选客户', sections: ['leads','lead','new-lead'], icon: 'M4 20v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2H4m7-10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7' },
  { kind: 'activities', label: '跟进记录', sections: ['activities'], icon: 'M6 3h12a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1m3 5h6m-6 4h6m-6 4h4' },
  { kind: 'mail', label: '邮件中心', sections: ['mail'], icon: 'M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Zm-1 2 9 6 9-6' },
  { kind: 'system', label: '系统状态', sections: ['system'], icon: 'M4 4h16v12H4zM9 21h6m-3-5v5m-5-9 2-3 3 2 4-4' },
  { kind: 'data', label: '数据与审计', sections: ['data'], icon: 'M20 6c0 2-3.6 3-8 3S4 8 4 6s3.6-3 8-3 8 1 8 3ZM4 6v12c0 2 3.6 3 8 3s8-1 8-3V6M4 12c0 2 3.6 3 8 3s8-1 8-3' },
]

function App() {
  const [page, setPage] = useState<Page>({ kind: 'contacts' })
  const openContact = (id: string) => setPage({ kind: 'contact', id })
  const compose = (contact: Contact) => setPage({ kind: 'mail', contact })
  const history = (id: string) => setPage({ kind: 'mail', inboxContact: id })
  const openLead = (id: string) => setPage({ kind: 'lead', id })
  const contacts = () => setPage({ kind: 'contacts' })
  const leads = () => setPage({ kind: 'leads' })

  return <>
    <a className="skip-link" href="#workbench">跳到工作区</a>
    <header className="app-header">
      <div className="brand"><div className="brand-mark" aria-hidden="true">F</div>
        <div className="brand-name">FTW <span>外贸智能工作台</span></div></div>
      <nav className="app-nav" aria-label="工作台">
        {navigation.map(({ kind, label, sections, icon }) => <button type="button" key={kind}
          onClick={() => setPage({ kind })} aria-current={sections.includes(page.kind) ? 'page' : undefined}>
          <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round">
            <path d={icon} />
          </svg><span>{label}</span>
        </button>)}
      </nav>
    </header>
    <main className="workspace" id="workbench" data-page={page.kind} tabIndex={-1}>
      {page.kind === 'dashboard' && <Dashboard mail={() => setPage({ kind: 'mail' })} inbox={() => setPage({ kind: 'mail', inboxOnly: true })} />}
      {page.kind === 'data' && <DataTools key={page.initialTab ?? 'backup'} initialTab={page.initialTab} />}
      {page.kind === 'contacts' && <ContactList compose={compose} history={history} importCsv={() => setPage({ kind: 'data', initialTab: 'csv' })} create={() => setPage({ kind: 'new-contact' })} />}
      {page.kind === 'contact' && <ContactDetail key={page.id} id={page.id} back={contacts} compose={compose} history={history} />}
      {page.kind === 'new-contact' && <ContactEditor saved={openContact} cancel={contacts} />}
      {page.kind === 'leads' && <LeadList open={openLead} create={() => setPage({ kind: 'new-lead' })} />}
      {page.kind === 'lead' && <LeadDetail key={page.id} id={page.id} back={leads} openContact={openContact} />}
      {page.kind === 'new-lead' && <LeadEditor saved={openLead} cancel={leads} />}
      {page.kind === 'activities' && <><div className="page-heading"><h1>跟进记录</h1></div><Timeline openContact={openContact} /></>}
      {page.kind === 'mail' && <MailCenter key={page.contact?.id ?? page.inboxContact ?? (page.inboxOnly ? 'inbox' : 'mail')} initialContact={page.contact} inboxContact={page.inboxContact} initialInbox={page.inboxOnly} pickContact={contacts} openContact={openContact} />}
      {page.kind === 'system' && <SystemStatus />}
    </main>
  </>
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
})

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}><CoreChanges /><App /></QueryClientProvider>,
)
