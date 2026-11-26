import { useCallback, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { invoke } from '@tauri-apps/api/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Activity, Bot, Database, History, LayoutDashboard, Mail, Search, UserSearch, UsersRound, type LucideIcon } from 'lucide-react'
import { AppShell } from './AppShell'
import { ContactDetail, ContactEditor, ContactList } from './crm/Contacts'
import { CustomerQuality } from './crm/Quality'
import { CompanyContacts } from './crm/CompanyContacts'
import { LeadDetail, LeadEditor, LeadList } from './crm/Leads'
import { Timeline } from './crm/Timeline'
import type { Contact } from './crm/api'
import { MailCenter, type Folder } from './mail/MailCenter'
import type { CustomerMailFile } from './mail/api'
import { SampleWorkspace } from './crm/Samples'
import { DataTools, type DataTab } from './data/DataTools'
import { Dashboard, type DashboardLink } from './data/Dashboard'
import { GlobalSearch } from './data/GlobalSearch'
import { AgentRuns, type AgentRecordLink } from './data/AgentRuns'
import { MailAnalytics, type MailReportLink } from './data/MailAnalytics'
import { SystemStatus } from './data/SystemStatus'
import type { AuditTarget, SearchTarget } from './data/api'
import { CoreChanges } from './data/CoreChanges'
import './styles.css'
import './data/workbench.css'
import './data/operations.css'
import './crm/insights.css'

let lastWebviewFailure = 0
function reportWebviewFailure(failure: 'script_error' | 'unhandled_rejection') {
  const now = Date.now()
  if (now - lastWebviewFailure < 5000) return
  lastWebviewFailure = now
  void invoke('report_webview_failure', { failure }).catch(() => {})
}
window.addEventListener('error', () => reportWebviewFailure('script_error'))
window.addEventListener('unhandledrejection', () => reportWebviewFailure('unhandled_rejection'))

type Page =
  | { kind: 'contacts'; initialView?: 'mail_no_reply' }
  | { kind: 'customer-quality'; duplicates?: boolean }
  | { kind: 'new-contact' | 'leads' | 'new-lead' | 'system' | 'dashboard' | 'samples' | 'search' | 'agent-runs' | 'mail-analysis' }
  | { kind: 'activities'; activityId?: string }
  | { kind: 'data'; initialTab?: DataTab; fileContact?: string; fileDocument?: string }
  | { kind: 'contact'; id: string; entry?: 'edit' | 'edit-channels' | 'edit-business'; returnTo?: { kind: 'company'; id: string } | { kind: 'customer-quality' } }
  | { kind: 'company' | 'lead'; id: string }
  | { kind: 'mail'; contact?: Contact; inboxContact?: string; draftContact?: string; draftId?: string; inboxMessage?: string; inboxOnly?: boolean; reviewInbox?: boolean; folder?: Folder }

const navigation: { kind: 'dashboard' | 'contacts' | 'leads' | 'activities' | 'mail' | 'system' | 'data' | 'search' | 'agent-runs'; label: string; sections: Page['kind'][]; icon: LucideIcon }[] = [
  { kind: 'dashboard', label: '工作台', sections: ['dashboard'], icon: LayoutDashboard },
  { kind: 'contacts', label: '客户管理', sections: ['contacts','customer-quality','contact','company','new-contact','samples'], icon: UsersRound },
  { kind: 'leads', label: '候选客户', sections: ['leads','lead','new-lead'], icon: UserSearch },
  { kind: 'activities', label: '跟进记录', sections: ['activities'], icon: History },
  { kind: 'mail', label: '邮件中心', sections: ['mail', 'mail-analysis'], icon: Mail },
  { kind: 'agent-runs', label: 'Agent 任务', sections: ['agent-runs'], icon: Bot },
  { kind: 'system', label: '系统状态', sections: ['system'], icon: Activity },
  { kind: 'data', label: '数据与审计', sections: ['data'], icon: Database },
  { kind: 'search', label: '全局搜索', sections: ['search'], icon: Search },
]

const dashboardPages: Record<DashboardLink, Page> = {
  customers: { kind: 'contacts' }, sent: { kind: 'mail', folder: 'accepted' }, waiting: { kind: 'contacts', initialView: 'mail_no_reply' },
  mail: { kind: 'mail' }, activities: { kind: 'activities' }, agent: { kind: 'agent-runs' }, analytics: { kind: 'mail-analysis' }, new: { kind: 'new-contact' },
  csv: { kind: 'data', initialTab: 'csv' }, review: { kind: 'mail', folder: 'review' }, approvals: { kind: 'mail', folder: 'drafts' },
  sync: { kind: 'mail', folder: 'imap' },
}

function App() {
  const [page, setPage] = useState<Page>({ kind: 'contacts' })
  const [searchFocus, setSearchFocus] = useState(0)
  const [searchKeyword, setSearchKeyword] = useState('')
  const openSearch = useCallback(() => { setPage({ kind: 'search' }); setSearchFocus(value => value + 1) }, [])
  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if (!event.isComposing && event.ctrlKey && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (!event.repeat) openSearch()
      }
    }
    window.addEventListener('keydown', shortcut)
    return () => window.removeEventListener('keydown', shortcut)
  }, [openSearch])
  const openContact = (id: string) => setPage({ kind: 'contact', id })
  const openCompany = (id: string) => setPage({ kind: 'company', id })
  const openActivity = (id: string) => setPage({ kind: 'activities', activityId: id })
  const compose = (contact: Contact) => setPage({ kind: 'mail', contact })
  const history = (id: string) => setPage({ kind: 'mail', inboxContact: id })
  const files = (id: string) => setPage({ kind: 'data', initialTab: 'files', fileContact: id })
  const openFileMail = (file: CustomerMailFile) => setPage(file.origin === 'inbox'
    ? { kind: 'mail', inboxContact: file.contact_id, inboxMessage: file.message_id }
    : { kind: 'mail', draftContact: file.contact_id, draftId: file.message_id })
  const openLead = (id: string) => setPage({ kind: 'lead', id })
  const contacts = () => setPage({ kind: 'contacts' })
  const leads = () => setPage({ kind: 'leads' })
  const showAnalysis = () => setPage({ kind: 'mail-analysis' })
  const openDraft = (id: string) => setPage({ kind: 'mail', draftId: id, folder: 'records' })
  function openAgentRecord(kind: AgentRecordLink, id: string) {
    if (kind === 'contact') return openContact(id)
    if (kind === 'lead') return openLead(id)
    if (kind === 'activity') return openActivity(id)
    return openDraft(id)
  }
  function openMailReport(kind: MailReportLink) {
    if (kind === 'waiting') return setPage({ kind: 'contacts', initialView: 'mail_no_reply' })
    setPage({ kind: 'mail', folder: kind === 'mail' ? 'inbox' : kind === 'sync' ? 'imap' : kind })
  }
  function openSearchTarget(target: SearchTarget) {
    switch (target.record_type) {
      case 'customer': return openContact(target.id)
      case 'activity': return openActivity(target.id)
      case 'inbox': case 'inbox_attachment': return setPage({ kind: 'mail', inboxMessage: target.id })
      case 'draft': case 'outbound_attachment': return openDraft(target.id)
      case 'document': return setPage({ kind: 'data', initialTab: 'files', fileContact: target.contact_id, fileDocument: target.id })
    }
  }

  function openAuditTarget(target: AuditTarget) {
    if (target.record_type === 'lead') return openLead(target.id)
    return openSearchTarget({ record_type: target.record_type, id: target.id })
  }

  return <AppShell page={page.kind} navigate={kind => kind === 'search' ? openSearch() : setPage({ kind })}
    navigation={navigation.map(({ sections, ...item }) => ({ ...item, active: sections.includes(page.kind) }))}>
      {page.kind === 'dashboard' && <Dashboard navigate={link => setPage(dashboardPages[link])} openActivity={openActivity} />}
      {page.kind === 'agent-runs' && <AgentRuns back={() => setPage({ kind: 'dashboard' })} open={openAgentRecord}
        changes={() => setPage({ kind: 'data', initialTab: 'agent-changes' })} />}
      {page.kind === 'mail-analysis' && <MailAnalytics open={openMailReport} />}
      {page.kind === 'search' && <GlobalSearch focus={searchFocus} keyword={searchKeyword} setKeyword={setSearchKeyword} open={openSearchTarget} />}
      {page.kind === 'data' && <DataTools key={`${page.initialTab ?? 'overview'}:${page.fileContact ?? ''}:${page.fileDocument ?? ''}`} initialTab={page.initialTab} fileContact={page.fileContact} fileDocument={page.fileDocument}
        openContact={openContact} openFileMail={openFileMail} openAuditTarget={openAuditTarget} />}
      {page.kind === 'contacts' && <ContactList key={page.initialView ?? 'default'} initialView={page.initialView}
        compose={compose} history={history} files={files} company={openCompany} quality={duplicates => setPage({ kind: 'customer-quality', duplicates })} samples={() => setPage({ kind: 'samples' })}
        importCsv={() => setPage({ kind: 'data', initialTab: 'csv' })} create={() => setPage({ kind: 'new-contact' })} />}
      {page.kind === 'customer-quality' && <CustomerQuality key={String(Boolean(page.duplicates))} back={contacts} open={openContact}
        duplicatesOnly={page.duplicates} edit={(id, area) => setPage({ kind: 'contact', id, entry: area === 'basic' ? 'edit' : `edit-${area}`, returnTo: { kind: 'customer-quality' } })}
        openMail={(id, draftId) => setPage({ kind: 'mail', draftContact: id, draftId })} />}
      {page.kind === 'company' && <CompanyContacts key={page.id} id={page.id} back={contacts} history={history}
        open={id => setPage({ kind: 'contact', id, returnTo: { kind: 'company', id: page.id } })} />}
      {page.kind === 'samples' && <SampleWorkspace back={contacts} openContact={openContact} openActivity={openActivity} />}
      {page.kind === 'contact' && <ContactDetail key={page.id + (page.entry ?? '')} id={page.id} entry={page.entry}
        back={() => page.returnTo ? setPage(page.returnTo) : contacts()} openRelated={openContact} compose={compose} history={history} files={files} company={openCompany} />}
      {page.kind === 'new-contact' && <ContactEditor saved={openContact} cancel={contacts} />}
      {page.kind === 'leads' && <LeadList open={openLead} openContact={openContact} create={() => setPage({ kind: 'new-lead' })} />}
      {page.kind === 'lead' && <LeadDetail key={page.id} id={page.id} back={leads} openContact={openContact} />}
      {page.kind === 'new-lead' && <LeadEditor saved={openLead} cancel={leads} />}
      {page.kind === 'activities' && <Timeline key={page.activityId ?? 'activities'} openContact={openContact} initialActivity={page.activityId} />}
      {page.kind === 'mail' && <MailCenter key={page.contact?.id ?? page.inboxMessage ?? page.draftId ?? page.inboxContact ?? page.draftContact ?? page.folder ?? (page.inboxOnly ? 'inbox' : 'mail')}
        initialContact={page.contact} inboxContact={page.inboxContact} draftContact={page.draftContact} initialInbox={page.inboxOnly}
        initialMessageId={page.inboxMessage} initialDraftId={page.draftId} initialFolder={page.folder}
        reviewInbox={page.reviewInbox} pickContact={contacts} openContact={openContact} showAnalysis={showAnalysis}
        openUnreplied={() => setPage({ kind: 'contacts', initialView: 'mail_no_reply' })} />}
      {page.kind === 'system' && <SystemStatus audit={() => setPage({ kind: 'data', initialTab: 'audit' })}
        mail={protocol => setPage({ kind: 'mail', folder: protocol === 'IMAP' ? 'imap' : 'accounts' })} />}
  </AppShell>
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
})

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}><CoreChanges /><App /></QueryClientProvider>,
)
