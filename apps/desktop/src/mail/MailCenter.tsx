import { useDeferredValue, useState } from 'react'
import { Archive, Ban, ChartColumn, CircleAlert, CircleHelp, Clock, FilePenLine, FolderOpen, Inbox, List, Mail, Paperclip, Pencil, Plus, RefreshCw, Send, Settings, ShieldCheck, Trash2, UserRound, type LucideIcon } from 'lucide-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateLabel, outreachLabels, request, sourceLabel, type Contact } from '../crm/api'
import { ConfirmButton, Empty, Field, Pagination } from '../crm/components'
import { useSavedViews } from '../crm/SavedViews'
import { AgentMail } from './AgentMail'
import { Accounts } from './Accounts'
import { InboxView, MailThread, type ReplyTarget } from './Inbox'
import { chooseMailAssets, mailCall, mailError, type Asset, type Draft, type DraftFolder, type DraftInput } from './api'
import { AttachmentActions } from './AttachmentActions'
import { MailSearch, MailStatus } from './MailWorkspace'
import { DraftTemplatePicker, Templates } from './Templates'

export type Folder = DraftFolder | 'inbox' | 'review' | 'records' | 'accounts' | 'imap' | 'agent' | 'suppression' | 'templates'
const folders: Record<Folder, { label: string; icon: LucideIcon }> = {
  inbox: { label: '收件箱', icon: Inbox },
  accepted: { label: '已提交 SMTP', icon: Send },
  drafts: { label: '草稿与审批', icon: FilePenLine },
  queue: { label: '发送队列', icon: Clock },
  submissions: { label: '全部提交记录', icon: List },
  failed: { label: '发送失败', icon: CircleAlert },
  unknown: { label: '结果未知', icon: CircleHelp },
  bounced: { label: '已退信', icon: Archive },
  review: { label: '回复待确认', icon: Mail },
  records: { label: '全部发信记录', icon: List },
  accounts: { label: '发件账户', icon: Settings },
  imap: { label: '收件设置', icon: RefreshCw },
  agent: { label: 'Agent 发信权限', icon: ShieldCheck },
  suppression: { label: '拒发名单', icon: Ban },
  templates: { label: '邮件模板', icon: FilePenLine },
}
const messageFolders: Folder[] = ['inbox', 'accepted', 'drafts', 'queue', 'failed', 'unknown', 'bounced', 'review', 'records']
const settingsFolders: Folder[] = ['accounts', 'imap', 'agent', 'suppression', 'templates']
const queueMetrics = [
  ['queue', '队列中', '等待或正在提交'], ['accepted', 'SMTP 已接受', '已由 SMTP 服务器接受'],
  ['failed', '发送失败', '检查错误原因'], ['unknown', '提交结果未知', '先核对发件记录'],
] as const

function isOutbound(folder: Folder): folder is DraftFolder | 'records' {
  return folder !== 'inbox' && folder !== 'review' && !settingsFolders.includes(folder)
}

function ErrorNotice({ error }: { error: unknown }) {
  return <p className="alert error" role="alert">{mailError(error)}</p>
}

export function MailCenter({ initialContact, inboxContact, draftContact, initialDraftId, initialMessageId, initialInbox, initialFolder, reviewInbox, pickContact, openContact, openUnreplied, showAnalysis }: {
  initialContact?: Contact; inboxContact?: string; draftContact?: string; initialDraftId?: string; initialMessageId?: string; initialInbox?: boolean; reviewInbox?: boolean
  initialFolder?: Folder
  pickContact: () => void; openContact: (id: string) => void; openUnreplied: () => void; showAnalysis: () => void
}) {
  const [folder, setFolder] = useState<Folder>(initialFolder ?? (draftContact ? 'records' : reviewInbox && initialInbox ? 'review' : 'inbox'))
  const [compose, setCompose] = useState(initialContact !== undefined)
  const [replyTarget, setReplyTarget] = useState<ReplyTarget>()
  const [offset, setOffset] = useState(0)
  const [input, setInput] = useState('')
  const [selection, setSelection] = useState<{ id: string; editing: boolean } | undefined>(initialDraftId ? { id: initialDraftId, editing: false } : undefined)
  const [linkedMessage, setLinkedMessage] = useState(initialMessageId)
  const keyword = useDeferredValue(input.trim())
  const outbound = isOutbound(folder)
  const queueArea = !compose && (queueMetrics.some(([key]) => key === folder) || folder === 'bounced' || folder === 'submissions')
  const reviewArea = !compose && folder === 'review'
  const accountsArea = !compose && (folder === 'accounts' || folder === 'imap')
  const templatesArea = !compose && folder === 'templates'
  function selectFolder(next: Folder) {
    setFolder(next)
    setCompose(false)
    setReplyTarget(undefined)
    setSelection(undefined)
    setOffset(0)
    setInput('')
    setLinkedMessage(undefined)
  }
  function openDraft(id: string) {
    if (compose) selectFolder('drafts')
    setSelection({ id, editing: false })
  }
  function startReply(message: ReplyTarget) {
    setReplyTarget(message)
    setCompose(true)
  }
  const drafts = useQuery({
    queryKey: ['mail-drafts', draftContact ?? null, folder, keyword, offset], enabled: !compose && outbound,
    queryFn: () => mailCall({ action: 'list_drafts', contact_id: draftContact ?? null, offset, keyword,
      folder: isOutbound(folder) && folder !== 'records' ? folder : undefined }, 'drafts'),
  })
  const summary = useQuery({
    queryKey: ['mail-queue-summary', draftContact ?? null], enabled: queueArea,
    queryFn: () => mailCall({ action: 'queue_summary', contact_id: draftContact ?? null }, 'queue_summary'),
  })
  const agent = useQuery({
    queryKey: ['agent-mail-settings'], queryFn: () => mailCall({ action: 'agent_mail_settings' }, 'agent_mail_settings'),
  })
  const views = useSavedViews()
  const mode = agent.data?.policy.mode
  const approvalMode = mode === 'automatic'
    ? agent.data?.automatic_available ? '持续授权自动发送' : '安全账本不可用'
    : mode === 'manual' ? '逐封人工审批' : '已禁止 Agent 发送'
  const page = drafts.data?.rows ?? []
  const current = selection ?? (page[0] && { id: page[0].id, editing: false })
  return <div className="mail-workspace">
    <header className="page-heading mail-page-heading">
      <div><h1>{folder === 'agent' ? 'Agent 邮件发送设置' : accountsArea ? '邮箱账户与签名' : templatesArea ? '开发邮件模板' : queueArea ? '邮件发送队列与异常' : reviewArea ? '客户回复匹配' : '邮件中心'}</h1>
        <p>{folder === 'agent' ? '管理 MCP 发信身份、授权账户与用量限制' : accountsArea ? '管理 SMTP 发件、IMAP 收件和账户签名。' : templatesArea ? '按邮件用途整理模板，用客户资料生成预览。' : queueArea ? '查看队列、SMTP 提交结果和需要核对的发信记录。'
          : reviewArea ? '核对待确认的来信、匹配依据与 CRM 联系人。' : '收件、发件与客户开发记录'}</p></div>
      <div className="row-actions">
        {!compose && <button type="button" className="button" onClick={showAnalysis}><ChartColumn aria-hidden="true" />效果分析</button>}
        {queueArea && <>
          <button className="button" onClick={() => selectFolder('agent')}><ShieldCheck aria-hidden="true" />查看发信权限</button>
          <button className="button primary" disabled={summary.isFetching || drafts.isFetching}
            onClick={() => { void summary.refetch(); void drafts.refetch() }}><RefreshCw aria-hidden="true" />刷新队列</button>
        </>}
        {folder !== 'agent' && !compose && !queueArea && !reviewArea && !accountsArea && !templatesArea && <button className="button primary" onClick={initialContact ? () => setCompose(true) : pickContact}>
          <Pencil aria-hidden="true" />从客户写邮件
        </button>}
      </div>
    </header>
    {folder !== 'agent' && mode && !queueArea && !reviewArea && !accountsArea && !templatesArea && <div className="mail-policy-banner" data-automatic={mode === 'automatic' && agent.data?.automatic_available}>
      <ShieldCheck aria-hidden="true" /><strong>Agent 发信</strong><span>{approvalMode}</span>
      {mode === 'automatic' && <small>{agent.data?.policy.accounts.length ?? 0} 个授权账户</small>}
      <button className="text-button" onClick={() => selectFolder('agent')}>管理权限</button>
    </div>}
    <div className="mail-shell">
      <nav className="mail-folders" aria-label="邮件分类">
        {([['邮件分类', messageFolders], ['设置', settingsFolders]] as const).map(([label, items]) => <div key={label}>
          <span>{label}</span>
          {items.map(key => {
            const { icon: Icon, label } = folders[key]
            return <button type="button" key={key} aria-current={!compose && (folder === key || (key === 'queue' && folder === 'submissions')) ? 'page' : undefined} onClick={() => selectFolder(key)}>
              <Icon aria-hidden="true" />{label}
            </button>
          })}
          {label === '邮件分类' && views.data?.views.some(view => view.id === 'mail_no_reply' && !view.archived) &&
            <button type="button" onClick={openUnreplied}><UserRound aria-hidden="true" />已发未见回复</button>}
        </div>)}
      </nav>
      <div className="mail-main">
      {compose && replyTarget ? <ReplyComposer key={replyTarget.id} reply={replyTarget} saved={openDraft} close={() => selectFolder(folder)} />
        : compose && initialContact ? <DraftForm contact={initialContact} saved={openDraft} close={() => selectFolder(folder)} />
        : accountsArea ? <Accounts key={folder} initialTab={folder === 'imap' ? 'imap' : 'smtp'} />
        : templatesArea ? <Templates />
        : folder === 'agent' ? <AgentMail manageSuppression={() => selectFolder('suppression')} showRecords={() => selectFolder('records')} />
        : folder === 'suppression' ? <SuppressionSettings />
        : outbound ? <>
          {queueArea && <div className="mail-queue-metrics" aria-label="发送结果概览">
            {summary.isError && <ErrorNotice error={summary.error} />}
            {queueMetrics.map(([key, label, hint]) => {
              const Icon = folders[key].icon
              return <button type="button" key={key} data-metric={key} aria-pressed={folder === key} onClick={() => selectFolder(key)}>
                <span className="mail-queue-symbol"><Icon aria-hidden="true" /></span>
                <span><strong>{summary.data?.[key] ?? '—'}</strong><span>{label}</span><small>{hint}</small></span>
              </button>
            })}
          </div>}
          <div className={`mail-split${queueArea ? ' mail-queue-split' : ''}`} key={folder}>
          <section className="mail-list-pane" aria-label={folders[folder].label}>
            <header className="mail-list-heading">
              <div className="section-heading"><h2>{folders[folder].label}</h2>
                {queueArea && folder !== 'submissions' && <button className="text-button" onClick={() => selectFolder('submissions')}><List aria-hidden="true" />全部提交记录</button>}
              </div>
              <MailSearch label="搜索主题或收件邮箱" value={input} change={value => { setInput(value); setOffset(0); setSelection(undefined) }} />
              {draftContact && <button className="text-button" onClick={() => openContact(draftContact)}>返回客户资料</button>}
            </header>
            <div className="mail-list-scroll">
              {drafts.isError && <ErrorNotice error={drafts.error} />}
              {drafts.isPending && <Empty>正在读取发件记录…</Empty>}
              {drafts.data && !page.length && <Empty>当前没有符合条件的发件记录。</Empty>}
              {queueArea && page.length > 0 ? <table className="mail-queue-table" aria-label={folders[folder].label}>
                <thead><tr><th>邮件 / 收件人</th><th>提交状态</th><th>更新时间</th></tr></thead>
                <tbody>{page.map(row => <tr key={row.id} data-active={current?.id === row.id}>
                  <td><button type="button" className="text-button" aria-pressed={current?.id === row.id} onClick={() => openDraft(row.id)}>{row.subject || '（无主题）'}</button>
                    <small>{row.recipient}</small></td>
                  <td><MailStatus row={row} bounced={folder === 'bounced'} /></td>
                  <td>{dateLabel(row.updated_at)}<small>{row.agent_name || sourceLabel(row.source)}</small></td>
                </tr>)}</tbody>
              </table> : page.map(row => <button type="button" className="mail-list-item" aria-pressed={current?.id === row.id} key={row.id} onClick={() => openDraft(row.id)}>
                <span className="mail-list-avatar"><Send aria-hidden="true" /></span>
                <span className="mail-list-content">
                  <strong title={row.subject}>{row.subject || '（无主题）'}</strong>
                  <small title={row.recipient}>{row.recipient}</small><MailStatus row={row} bounced={folder === 'bounced'} />
                  <small>{dateLabel(row.updated_at)} · {row.agent_name || sourceLabel(row.source)}</small>
                </span>
              </button>)}
            </div>
            {drafts.data && <Pagination offset={offset} more={drafts.data.has_more} change={next => { setOffset(next); setSelection(undefined) }} />}
          </section>
          <div className="mail-reader">
            {current ? <DraftView key={current.id} id={current.id} editing={current.editing} openContact={openContact}
              edit={() => setSelection({ id: current.id, editing: true })} saved={openDraft} bounced={folder === 'bounced'} queueArea={queueArea} onReply={startReply} />
              : <Empty>选择一封发件记录查看详情。</Empty>}
          </div>
        </div></>
        : <InboxView key={folder} contactId={folder === 'inbox' ? inboxContact : undefined} initialMessageId={linkedMessage} initiallyReviewing={folder === 'review'} openContact={openContact} onReply={startReply} />}
      </div>
    </div>
  </div>
}

function SuppressionSettings() {
  const [offset, setOffset] = useState(0)
  const [input, setInput] = useState('')
  const [notice, setNotice] = useState('')
  const cache = useQueryClient()
  const rules = useQuery({ queryKey: ['mail-suppression', offset], queryFn: () => mailCall({ action: 'list_suppression', offset }, 'suppression') })
  const change = useMutation({
    mutationFn: ({ target, blocked }: { target: string; blocked: boolean }) => mailCall({ action: 'set_suppression', target, blocked }, 'saved'),
    onMutate: () => setNotice(''),
    onSuccess: async (_, { blocked }) => {
      if (blocked) { setInput(''); setOffset(0) }
      else if (offset && rules.data?.rows.length === 1) setOffset(Math.max(0, offset - 20))
      setNotice(blocked ? '已保存拒发规则。' : '已移出拒发名单。')
      await cache.invalidateQueries({ queryKey: ['mail-suppression'] })
    },
  })
  return <section className="panel mail-settings-panel">
    <h2><Ban aria-hidden="true" />拒发名单</h2>
    <p className="muted">按邮箱地址或域名阻止发送。Core 在入队和发送前应用此名单。</p>
    <form className="mail-suppression-form" onSubmit={event => { event.preventDefault(); change.mutate({ target: input.trim(), blocked: true }) }}>
      <Field label="邮箱或域名"><input value={input} onChange={event => setInput(event.target.value)} maxLength={320} required /></Field>
      <button className="button primary" disabled={!input.trim() || change.isPending}><Plus aria-hidden="true" />加入拒发名单</button>
    </form>
    {notice && <p role="status" className="muted">{notice}</p>}
    {(rules.isError || change.isError) && <ErrorNotice error={rules.error ?? change.error} />}
    {rules.isPending && <Empty>正在读取拒发名单…</Empty>}
    {rules.data && !rules.data.rows.length && <Empty>当前没有拒发规则。</Empty>}
    {rules.data && <><ul className="record-list">{rules.data.rows.map(target => <li key={target}>
      <strong>{target}</strong>
      <ConfirmButton label={`将 ${target} 移出拒发名单？`} disabled={change.isPending}
        onConfirm={() => change.mutate({ target, blocked: false })}>移出</ConfirmButton>
    </li>)}</ul><Pagination offset={offset} more={rules.data.has_more} change={setOffset} /></>}
  </section>
}

function ReplyComposer({ reply, saved, close }: { reply: ReplyTarget; saved: (id: string) => void; close: () => void }) {
  const contact = useQuery({ queryKey: ['contact', reply.contact_id], queryFn: () => request({ action: 'get_contact', id: reply.contact_id }, 'contact') })
  if (contact.isError) return <><ErrorNotice error={contact.error} /><button type="button" className="button" onClick={close}>返回收件箱</button></>
  return contact.data ? <DraftForm contact={contact.data.record} reply={reply} saved={saved} close={close} /> : <Empty>正在读取客户…</Empty>
}

function DraftForm({ contact, existing, reply, saved, close }: {
  contact: Contact; existing?: Draft; reply?: ReplyTarget; saved: (id: string) => void; close: () => void
}) {
  const emails = contact.input.methods.filter(method => method.kind === 'email')
  const [account, setAccount] = useState(existing?.account_id ?? reply?.account_id ?? '')
  const [recipient, setRecipient] = useState(existing?.recipient ?? reply?.sender ?? (emails.find(m => m.is_primary) ?? emails[0])?.value ?? '')
  const [subject, setSubject] = useState(existing?.subject ?? '')
  const [body, setBody] = useState(existing?.body ?? '')
  const [revision] = useState(existing?.revision ?? null)
  const [files, setFiles] = useState<Asset[]>([])
  const [replace, setReplace] = useState(false)
  const [fileError, setFileError] = useState('')
  const [readingFiles, setReadingFiles] = useState(false)
  const [showDocuments, setShowDocuments] = useState(false)
  const [documentOffset, setDocumentOffset] = useState(0)
  const [choosingTemplate, setChoosingTemplate] = useState(false)
  const cache = useQueryClient()
  const accounts = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const overview = useQuery({ queryKey: ['development-overview', contact.id], queryFn: () => request({ action: 'development_overview', id: contact.id }, 'development_overview') })
  const documents = useQuery({ queryKey: ['customer-files', contact.id, 'draft', documentOffset], enabled: showDocuments,
    queryFn: () => mailCall({ action: 'file_workspace', contact_id: contact.id, keyword: '', direction: 'document', offset: documentOffset }, 'file_workspace') })
  const senderRows = accounts.data?.kind === 'accounts' ? accounts.data.rows : []
  const sender = account ? (senderRows.some(row => row.id === account) ? account : '') : senderRows.find(row => row.is_default)?.id ?? senderRows[0]?.id ?? ''
  const currentRecipient = emails.find(item => item.value.toLowerCase() === recipient.toLowerCase())?.value ?? recipient
  const fixedReply = Boolean(reply || existing?.reply_to_inbox_id)
  const save = useMutation({
    mutationFn: () => {
      const input: DraftInput = { contact_id: contact.id, account_id: sender, recipient: currentRecipient,
        subject, body, attachments: [], replace_attachments: replace }
      const assetIds = files.map(file => file.id)
      if (reply) return mailCall({ action: 'create_reply_draft', input: { inbox_message_id: reply.id, subject: subject.trim() || null, body }, asset_ids: assetIds }, 'saved')
      if (existing && revision !== null) {
        return mailCall({ action: 'update_draft', id: existing.id, expected_revision: revision, input, asset_ids: replace ? assetIds : null }, 'saved')
      }
      return replace
        ? mailCall({ action: 'save_draft_with_assets', id: null, input, asset_ids: assetIds }, 'saved')
        : mailCall({ action: 'save_draft', id: null, input }, 'saved')
    },
    onSuccess: async result => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['mail-drafts'] }), cache.invalidateQueries({ queryKey: ['mail-draft', result.id] })])
      saved(result.id)
    },
  })
  async function currentFiles() {
    if (!existing || replace || revision === null) return files
    return (await mailCall({ action: 'prepare_draft_assets', id: existing.id, revision }, 'assets')).rows
  }
  function setAttachments(next: Asset[]) {
    if (next.length > 4 || next.reduce((sum, item) => sum + item.size, 0) > 10 * 1024 * 1024) throw new Error('attachment_limit')
    setFiles(next)
    setReplace(true)
  }
  const selectFiles = async () => {
    setFileError('')
    setReadingFiles(true)
    try {
      const current = await currentFiles()
      const selected = await chooseMailAssets()
      if (selected.length) setAttachments([...current, ...selected.filter(item => !current.some(row => row.id === item.id))])
    } catch (error) { setFileError(mailError(error)) }
    finally { setReadingFiles(false) }
  }
  async function removeFile(index: number) {
    setFileError(''); setReadingFiles(true)
    try { const current = await currentFiles(); setAttachments(current.filter((_, item) => item !== index)) }
    catch (error) { setFileError(mailError(error)) }
    finally { setReadingFiles(false) }
  }
  async function addDocument(id: string) {
    setFileError(''); setReadingFiles(true)
    try {
      const current = await currentFiles()
      const result = await mailCall({ action: 'customer_document_asset', contact_id: contact.id, document_id: id }, 'assets')
      setAttachments([...current, ...result.rows.filter(item => !current.some(row => row.id === item.id))])
    } catch (error) { setFileError(mailError(error)) }
    finally { setReadingFiles(false) }
  }
  return <form onSubmit={event => { event.preventDefault(); save.mutate() }}>
    <fieldset className="panel" disabled={save.isPending}>
      <div className="section-heading"><h2>{existing ? '编辑邮件草稿' : reply ? '回复邮件' : '写邮件'} · {contact.input.display_name}</h2>
        <div className="row-actions"><button type="button" className="button" aria-expanded={choosingTemplate} onClick={() => setChoosingTemplate(value => !value)}>
          <FilePenLine aria-hidden="true" />选择模板</button><button type="button" className="button" onClick={close}>取消</button></div></div>
      {accounts.isError && <ErrorNotice error={accounts.error} />}
      {accounts.data && senderRows.length === 0 && <p className="alert warning">请先在「发件账户」配置 SMTP。</p>}
      {emails.length === 0 && <p className="alert warning">请先在客户资料中添加邮箱。</p>}
      {overview.isError && <p className="alert warning" role="status">无法读取相关联系人，请稍后在客户管理中核对。</p>}
      {overview.data && overview.data.record.related_contacts.length > 0 && <div className="alert warning">
        <strong>相关公司或同域联系人</strong>
        <p>请结合历史判断是否需要联系；不同联系人可以分别开发。</p>
        <ul>{overview.data.record.related_contacts.slice(0, 5).map(row => <li key={row.id}>
          {row.display_name} · {row.company_name || '公司未填写'} · {row.reason === 'email_domain' ? '邮箱域名相同' : row.reason === 'company_profile' ? '同公司档案' : '公司名称相同'} · {outreachLabels[row.mail_status]} · 最近联系 {dateLabel(row.last_contact_at)}
        </li>)}</ul>{(overview.data.record.related_contacts.length > 5 || overview.data.record.related_has_more) && <small>更多关联联系人可在客户详情或客户管理中查看。</small>}
      </div>}
      {reply && <p className="muted">原邮件 · {reply.subject || '无主题'}</p>}
      {choosingTemplate && <DraftTemplatePicker contactId={contact.id} hasText={Boolean(subject.trim() || body.trim())}
        onClose={() => setChoosingTemplate(false)} onApply={data => { setSubject(data.subject); setBody(data.body); setChoosingTemplate(false) }} />}
      <div className="form-grid">
        <Field label="发件账户"><select value={sender} onChange={e => setAccount(e.target.value)} disabled={fixedReply} required>
          <option value="" disabled>选择账户</option>{senderRows.map(row => <option key={row.id} value={row.id}>{row.name} · {row.email}</option>)}</select></Field>
        <Field label="收件邮箱"><select value={currentRecipient} onChange={e => setRecipient(e.target.value)} disabled={fixedReply} required>
          <option value="" disabled>选择邮箱</option>{emails.map(row => <option key={row.value} value={row.value}>{row.value}</option>)}</select></Field>
        <Field label="主题"><input value={subject} onChange={e => setSubject(e.target.value)} required={!reply} placeholder={reply ? '留空沿用原邮件主题' : undefined} /></Field>
      </div>
      <Field label="正文"><textarea rows={10} value={body} onChange={e => setBody(e.target.value)} required /></Field>
      {senderRows.find(row => row.id === sender)?.signature && <button type="button" className="button" onClick={() => {
        const signature = senderRows.find(row => row.id === sender)?.signature
        if (signature && !body.trimEnd().endsWith(signature)) setBody(body.trimEnd() + (body.trim() ? '\n\n' : '') + signature)
      }}>插入账户签名</button>}
      <div className="row-actions">
        <button className="button" type="button" disabled={readingFiles} onClick={() => void selectFiles()}>
          <Paperclip aria-hidden="true" />{readingFiles ? '正在读取附件…' : '添加本地附件'}
        </button><button type="button" className="button" onClick={() => setShowDocuments(value => !value)} aria-expanded={showDocuments}><FolderOpen aria-hidden="true" />选择客户文件</button>
        <small>PDF、JPEG、PNG 或 WebP，最多 4 件，合计 10 MiB</small>
      </div>
      {showDocuments && <div className="draft-documents">
        {documents.isPending && <small>正在读取客户文件…</small>}
        {documents.isError && <ErrorNotice error={documents.error} />}
        {documents.data && <>{documents.data.rows.length ? <ul>{documents.data.rows.map(file => <li key={file.document_id}>
          <span>{file.filename}</span><button type="button" className="text-button" disabled={!file.document_id || readingFiles} onClick={() => file.document_id && void addDocument(file.document_id)}><Plus aria-hidden="true" />添加</button>
        </li>)}</ul> : <small>该客户尚无文件。</small>}
          <div className="row-actions">{documentOffset > 0 && <button type="button" className="button" onClick={() => setDocumentOffset(Math.max(0, documentOffset - 20))}>上一页</button>}
            {documents.data.has_more && <button type="button" className="button" onClick={() => setDocumentOffset(documentOffset + 20)}>下一页</button>}</div></>}
      </div>}
      {(replace ? files : existing?.attachments ?? []).map((file, i) => <div className="draft-attachment" key={`${file.sha256}:${i}`}>
        <small>{file.filename} · {file.size.toLocaleString()} 字节</small>
        <button type="button" className="text-button" disabled={readingFiles} aria-label={`移除附件 ${file.filename}`} onClick={() => void removeFile(i)}><Trash2 aria-hidden="true" />移除</button>
      </div>)}
      {(replace ? files.length > 0 : Boolean(existing?.attachments.length)) && <button type="button" className="text-button" disabled={readingFiles}
        onClick={() => { setFiles([]); setReplace(true); setFileError('') }}><Trash2 aria-hidden="true" />移除全部附件</button>}
      {fileError && <ErrorNotice error={fileError} />}
      {save.isError && <ErrorNotice error={save.error} />}
      <div className="form-footer"><button className="button primary" disabled={!sender || !emails.length || readingFiles || !!fileError}>保存草稿</button></div>
    </fieldset>
  </form>
}

function DraftView({ id, editing, edit, saved, openContact, bounced, queueArea, onReply }: {
  id: string; editing: boolean; edit: () => void; saved: (id: string) => void; openContact: (id: string) => void; bounced: boolean; queueArea: boolean; onReply: (message: ReplyTarget) => void
}) {
  const cache = useQueryClient()
  const [history, setHistory] = useState(false)
  const draft = useQuery({
    queryKey: ['mail-draft', id], queryFn: () => mailCall({ action: 'get_draft', id }, 'draft'),
    refetchInterval: query => query.state.data?.record.approval_attempt?.state === 'waiting' ? 2000 : false,
  })
  const record = draft.data?.kind === 'draft' ? draft.data.record : undefined
  const contact = useQuery({
    queryKey: ['contact', record?.contact_id], enabled: editing && Boolean(record),
    queryFn: () => request({ action: 'get_contact', id: record!.contact_id }, 'contact'),
  })
  const transition = useMutation({
    mutationFn: async (action: 'request_approval' | 'open_approval' | 'send_approved') => {
      if (action === 'request_approval') return mailCall({ action, id }, 'saved')
      if (action === 'open_approval') return mailCall({ action, id }, 'approval_opened')
      return mailCall({ action, id }, 'queued')
    },
    onSuccess: async () => {
      saved(id)
      await Promise.all([cache.invalidateQueries({ queryKey: ['mail-drafts'] }), cache.invalidateQueries({ queryKey: ['mail-draft', id] }),
        cache.invalidateQueries({ queryKey: ['mail-queue-summary'] })])
    },
  })
  if (draft.isError) return <ErrorNotice error={draft.error} />
  if (!record) return <Empty>正在读取草稿…</Empty>
  if (history && record.thread_id) return <MailThread threadId={record.thread_id} close={() => setHistory(false)} openContact={openContact} onReply={onReply} />
  if (editing) return contact.isError ? <ErrorNotice error={contact.error} /> : contact.data ?
    <DraftForm contact={contact.data.record} existing={record} saved={saved} close={() => saved(id)} /> : <Empty>正在读取客户…</Empty>

  const expired = record.approval_expired
  const attempt = record.approval_attempt
  const waiting = attempt?.state === 'waiting'
  return <section className="panel mail-reading-panel">
    <div className="section-heading">
      <div><MailStatus row={record} bounced={bounced} /><h2>{record.subject}</h2>
        <p className="muted">{record.reply_to_inbox_id ? '会话回复 · ' : ''}{record.has_reply ? '已有回复 · ' : ''}修订 {record.revision} · {record.agent_name || sourceLabel(record.source)}</p></div>
      <div className="row-actions">
        <button className="button" onClick={() => openContact(record.contact_id)}><UserRound aria-hidden="true" />打开客户</button>
        {record.thread_id && <button className="button" onClick={() => setHistory(true)}><Mail aria-hidden="true" />邮件往来</button>}
        {['draft', 'pending_approval', 'approved'].includes(record.state) && <button className="button" onClick={edit}><Pencil aria-hidden="true" />编辑草稿</button>}
        {record.account_active && record.state === 'draft' && <button className="button primary" disabled={transition.isPending} onClick={() => transition.mutate('request_approval')}>提交审核</button>}
        {record.account_active && (record.state === 'pending_approval' || (record.state === 'approved' && expired)) &&
          <button className="button primary" disabled={transition.isPending || waiting} onClick={() => transition.mutate('open_approval')}>{expired ? '重新审核' : '人工批准'}</button>}
        {record.account_active && record.state === 'approved' && !expired &&
          <ConfirmButton label="将该批准版本加入发件队列？" disabled={transition.isPending}
            onConfirm={() => transition.mutate('send_approved')}>发送已批准版本</ConfirmButton>}
      </div>
    </div>
    {waiting && <p role="status" className="alert warning">等待你在 FTW 的 Windows 审批窗口确认。</p>}
    {!record.account_active && ['draft', 'pending_approval', 'approved'].includes(record.state) &&
      <p role="status" className="alert warning">发件账户已删除。请编辑草稿并选择其他发件账户。</p>}
    {attempt?.state === 'cancelled' && attempt.revision === record.revision && <p role="status">已取消此次审批。</p>}
    {attempt?.state === 'approved' && attempt.revision === record.revision && record.state === 'approved' && !expired && <p role="status">当前修订已批准，可以提交发件。</p>}
    {attempt?.outcome && (attempt.state === 'failed' || attempt.state === 'interrupted') && <ErrorNotice error={attempt.outcome} />}
    {transition.isError && <ErrorNotice error={transition.error} />}
    <dl className={queueArea ? 'review-properties mail-job-properties' : 'profile-grid'}>
      <div><dt>发件人</dt><dd>{record.sender}</dd></div>
      <div><dt>收件人</dt><dd>{record.recipient}</dd></div>
      {queueArea && <>
        <div><dt>更新时间</dt><dd>{dateLabel(record.updated_at)}</dd></div>
        {record.job_id && <div><dt>发送编号</dt><dd>{record.job_id}</dd></div>}
        {record.task_id && <div><dt>Agent 任务</dt><dd>{record.task_id}</dd></div>}
        {record.outcome && record.state !== 'smtp_accepted' && <div><dt>提交结果</dt><dd>{mailError(record.outcome)}</dd></div>}
      </>}
      {!queueArea && record.expires_at !== null && <div><dt>批准有效至</dt><dd>{dateLabel(record.expires_at)}</dd></div>}
    </dl>
    {record.state === 'delivery_unknown' && <p className="alert warning">请先核对邮箱服务记录，再决定后续处理。</p>}
    {queueArea ? <details className="mail-conversation"><summary><Mail aria-hidden="true" />查看邮件正文</summary><p className="mail-body">{record.body}</p></details>
      : <><h3>正文</h3><p className="mail-body">{record.body}</p></>}
    {record.attachments.length > 0 && <><h3>附件</h3><ul className="source-list">{record.attachments.map((file, index) =>
      <li key={file.filename + file.sha256 + index}>{file.filename} · {file.size.toLocaleString()} 字节 <small>SHA-256 {file.sha256}</small>
        <AttachmentActions source="outbound" id={record.id} index={index} filename={file.filename} /></li>)}</ul></>}
    {!queueArea && record.outcome && record.state !== 'smtp_accepted' && <ErrorNotice error={record.outcome} />}
    {record.state === 'smtp_accepted' && <p className="muted">SMTP 服务器已接受提交。送达与回复状态以邮件服务为准。</p>}
  </section>
}
