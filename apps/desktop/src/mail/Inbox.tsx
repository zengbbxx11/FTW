import { Select } from '../ui/Select'
import { invoke } from '@tauri-apps/api/core'
import { AttachmentActions } from './AttachmentActions'
import { useDeferredValue, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, ClipboardCheck, Inbox, Link2, Mail, MailSearch as MailSearchIcon, Paperclip, RefreshCw, Reply, UserRound } from 'lucide-react'
import { dateLabel, request, type ContactQuery } from '../crm/api'
import { Empty, Field, Pagination } from '../crm/components'
import { ContactEditor } from '../crm/Contacts'
import { mailError } from './api'
import { MailSearch } from './MailWorkspace'
import { useSyncFrequency } from './syncSettings'

export type ImapAccount = {
  account_id: string; display_name: string; email_address: string; host: string | null
  port: number | null; username: string | null; last_sync: number | null; outcome: string | null
}
type Row = {
  id: string; account_id: string; contact_id: string | null; contact_name: string | null
  thread_id: string; sender: string; subject: string; received_at: number; kind: string; received: boolean; message_role: string; match_state: string; body_complete: boolean; download_outcome: string | null
  non_customer: boolean; handling_revision: number
}
type InboxFilter = 'all' | 'review' | 'unassociated' | 'non_customer'
type ReviewCandidate = { id: string; name: string; company: string; country: string; exact_sender: boolean; suggested_by?: string[] }
type Message = Row & { body: string; attachments: string[] }
export type ReplyTarget = Pick<Row, 'id' | 'account_id' | 'sender' | 'subject'> & { contact_id: string }
export type InboxReply =
  | { kind: 'body'; text: string; next_offset: number | null }
  | { kind: 'accounts'; rows: ImapAccount[] }
  | { kind: 'rows'; rows: Row[]; has_more: boolean }
  | { kind: 'review_candidates'; rows: ReviewCandidate[] }
  | { kind: 'message'; record: Message }
  | { kind: 'thread'; messages: Message[]; has_more: boolean }
  | { kind: 'saved'; id: string }
  | { kind: 'sync_started'; account_id: string }
  | { kind: 'connection_verified' }
  | { kind: 'reply_updates'; cursor: number; count: number }

export type InboxCommand =
  | { action: 'accounts' }
  | { action: 'configure'; account_id: string; host: string; port: number; username: string; password: string }
  | { action: 'test' | 'sync'; account_id: string }
  | { action: 'list'; account_id: string | null; contact_id: string | null; unmatched: boolean; offset: number; keyword?: string; filter?: InboxFilter }
  | { action: 'classify'; targets: { id: string; expected_revision: number }[]; non_customer: boolean }
  | { action: 'review_candidates'; id: string }
  | { action: 'review_link'; id: string; contact_id: string; expected_contact: string | null; expected_match: string; expected_handling_revision?: number }
  | { action: 'get'; id: string }
  | { action: 'thread'; id: string; offset: number }
  | { action: 'download_full'; id: string }
  | { action: 'read_body'; id: string; offset: number }
  | { action: 'link'; id: string; contact_id: string; expected_handling_revision?: number }
  | { action: 'reply_updates'; after: number | null }

export async function inboxCall<K extends InboxReply['kind']>(command: InboxCommand, expected: K): Promise<Extract<InboxReply, { kind: K }>> {
  const result = await invoke<InboxReply>('inbox_request', { command })
  if (result.kind !== expected) throw new Error('收到意外的收件服务响应。')
  return result as Extract<InboxReply, { kind: K }>
}
const call = inboxCall

const syncLabels: Record<string, string> = {
  running: '同步中', ok: '已同步', more_available: '本批已同步，仍有较早或新增来信可继续取回',
  interrupted: '上次同步中断', imap_timeout: '同步超时',
}

function ErrorBox({ error }: { error: unknown }) {
  return <p role="alert" className="alert error">{mailError(error)}</p>
}

export function useImapAccounts() {
  return useQuery({ queryKey: ['imap-accounts'], queryFn: () => call({ action: 'accounts' }, 'accounts') })
}

export function testImapAccount(accountId: string) {
  return call({ action: 'test', account_id: accountId }, 'connection_verified')
}

export function ImapAccountSettings({ account }: { account: ImapAccount }) {
  const cache = useQueryClient()
  const [notice, setNotice] = useState('')
  const mutate = useMutation({
    mutationFn: async (command: InboxCommand) => command.action === 'configure' ? call(command, 'saved') : call(command, 'connection_verified'),
    onMutate: () => setNotice(''),
    onSuccess: async result => {
      setNotice(result.kind === 'saved' ? '收件设置已保存。' : 'IMAP TLS、登录与 INBOX 访问均通过。')
      await cache.invalidateQueries({ queryKey: ['imap-accounts'] })
    },
  })
  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    mutate.mutate({
      action: 'configure', account_id: account.account_id, host: String(data.get('host')), port: Number(data.get('port')),
      username: String(data.get('username')), password: String(data.get('password')),
    }, { onSettled: () => { const entry = form.elements.namedItem('password'); if (entry instanceof HTMLInputElement) entry.value = '' } })
  }
  return <>
      <form className="account-settings-form" onSubmit={save}>
        <div className="account-form-fields">
          <Field label="IMAP 主机"><input name="host" placeholder="imap.example.com" defaultValue={account.host ?? ''} required /></Field>
          <Field label="TLS 端口"><input name="port" type="number" min="1" max="65535" defaultValue={account.port ?? 993} required /></Field>
          <Field label="IMAP 登录名"><input name="username" defaultValue={account.username ?? account.email_address} required /></Field>
          <Field label="IMAP 授权码或密码"><input type="password" name="password" autoComplete="new-password" required /></Field>
        </div>
        <div className="form-footer"><button className="button" type="button" disabled={!account.host || mutate.isPending}
          onClick={() => mutate.mutate({ action: 'test', account_id: account.account_id })}><RefreshCw aria-hidden="true" />测试 IMAP</button>
          <button className="button primary" disabled={mutate.isPending}>保存收件设置</button></div>
      </form>
    {notice && <p className="muted" role="status">{notice}</p>}
    {mutate.isError && <ErrorBox error={mutate.error} />}
  </>
}

export function InboxView({ contactId, initialMessageId, initiallyReviewing, openContact, onReply }: { contactId?: string; initialMessageId?: string; initiallyReviewing?: boolean; openContact: (id: string) => void; onReply?: (message: ReplyTarget) => void }) {
  const accounts = useImapAccounts()
  const [frequency, setFrequency] = useSyncFrequency()
  const [accountId, setAccount] = useState('')
  const [filter, setFilter] = useState<InboxFilter>(initiallyReviewing ? 'review' : 'all')
  const unmatched = !contactId && filter === 'review'
  const [checked, setChecked] = useState<Record<string, number>>({})
  const [input, setInput] = useState('')
  const keyword = useDeferredValue(input.trim())
  const [selectedId, setSelectedId] = useState('')
  const [dismissLinked, setDismissLinked] = useState(false)
  const [manualReviewId, setManualReviewId] = useState('')
  const direct = manualReviewId || (dismissLinked ? '' : initialMessageId ?? '')
  const [notice, setNotice] = useState('')
  const [offset, setOffset] = useState(0)
  const [fullReview, setFullReview] = useState(false)
  const configured = accounts.data?.rows.filter(account => account.host) ?? []
  const active = accounts.data?.rows.find(row => row.account_id === accountId) ?? (!accountId && configured.length === 1 ? configured[0] : undefined)
  const cache = useQueryClient()
  const rows = useQuery({
    queryKey: ['inbox', accountId, contactId, unmatched, offset, keyword, filter],
    queryFn: () => call({ action: 'list', account_id: accountId || null, contact_id: contactId ?? null, unmatched, offset, keyword, filter }, 'rows'),
  })
  const linked = useQuery({ queryKey: ['inbox-record', direct], enabled: Boolean(direct),
    queryFn: () => call({ action: 'get', id: direct }, 'message') })
  const sync = useMutation({
    mutationFn: (id: string) => call({ action: 'sync', account_id: id }, 'sync_started'),
    onSuccess: () => cache.invalidateQueries({ queryKey: ['imap-accounts'] }),
  })
  const classify = useMutation({
    mutationFn: ({ targets, non_customer }: { targets: { id: string; expected_revision: number }[]; non_customer: boolean }) => call({ action: 'classify', targets, non_customer }, 'saved'),
    onSuccess: async (_, input) => {
      select(); setChecked({}); setNotice(input.non_customer ? '已标记为非客户邮件，可在非客户邮件中恢复。' : '已恢复，按当前客户线索重新分类。')
      await Promise.all(['inbox', 'inbox-record', 'inbox-thread', 'dashboard'].map(key => cache.invalidateQueries({ queryKey: [key] })))
    },
    onError: async () => { setChecked({}); await cache.invalidateQueries({ queryKey: ['inbox'] }) },
  })
  const page = rows.data?.rows ?? []
  const current = direct ? page.find(row => row.id === direct) ?? linked.data?.record : page.find(row => row.id === selectedId) ?? page[0]
  function select(id = '') {
    setManualReviewId('')
    setSelectedId(id)
    setDismissLinked(true)
    setFullReview(false)
    setNotice('')
  }
  function clearSelection() { select(); setChecked({}) }
  const targets = Object.entries(checked).map(([id, expected_revision]) => ({ id, expected_revision }))
  return <div className={`mail-split inbox-workspace${unmatched ? ' reply-review-workspace' : ''}`}>
    <section className="mail-list-pane" aria-label="收件列表">
      <header className="mail-list-heading">
        <div className="section-heading">
          <h2>{contactId ? '客户来信' : unmatched ? '客户来信待核对' : '收件箱'}</h2>
          {contactId && <button className="text-button" onClick={() => openContact(contactId)}>打开客户</button>}
          {unmatched && rows.data && <small>本页 {rows.data.rows.length} 封</small>}
        </div>
        <MailSearch label="搜索主题或发件人" value={input} change={value => { setInput(value); setOffset(0); clearSelection() }} />
        {!contactId && <Field label="来信范围"><Select value={filter} onChange={event => { setFilter(event.target.value as InboxFilter); setOffset(0); clearSelection() }}>
          <option value="all">全部来信</option><option value="review">客户来信待核对</option><option value="unassociated">普通未关联来信</option><option value="non_customer">非客户邮件</option>
        </Select></Field>}
        <div className="mail-inbox-tools">
          <Select aria-label="收件账户" value={accountId} onChange={e => { setAccount(e.target.value); setOffset(0); clearSelection(); sync.reset() }}>
            <option value="">全部邮箱</option>{accounts.data?.rows.map(row => <option key={row.account_id} value={row.account_id}>{row.display_name}</option>)}
          </Select>
          <button className="button" type="button" disabled={!active?.host || sync.isPending || active.outcome === 'running'}
            onClick={() => active && sync.mutate(active.account_id)}><RefreshCw aria-hidden="true" />{active?.outcome === 'running' ? '同步中…' : '同步'}</button>
        </div>
        <small className="mail-sync-status">{active?.host ? active.outcome ? syncLabels[active.outcome] ?? mailError(active.outcome) : '尚未同步'
          : configured.length ? '选择一个邮箱后同步' : '请先配置 IMAP'}
          {active?.last_sync ? ' · ' + dateLabel(active.last_sync) : ''}</small>
        <label className="mail-auto-sync">自动同步 <Select aria-label="自动收件同步频率" value={frequency} onChange={event => setFrequency(Number(event.target.value))}>
          <option value={0}>关闭</option><option value={5}>每 5 分钟</option><option value={10}>每 10 分钟</option>
          <option value={30}>每 30 分钟</option><option value={60}>每小时</option>
        </Select><small>FTW 运行时生效</small></label>
      </header>
      <div className="mail-list-scroll">
        {targets.length > 0 && <div className="mail-batch-actions"><small>已选 {targets.length} 封</small>
          <button type="button" className="button" disabled={classify.isPending} onClick={() => classify.mutate({ targets, non_customer: true })}>标记非客户</button>
          <button type="button" className="button" disabled={classify.isPending} onClick={() => classify.mutate({ targets, non_customer: false })}>恢复</button>
          <button type="button" className="text-button" onClick={() => setChecked({})}>清除选择</button></div>}
        {classify.isError && <ErrorBox error={classify.error} />}
        {accounts.isError && <ErrorBox error={accounts.error} />}
        {sync.isError && <ErrorBox error={sync.error} />}
        {rows.isError && <ErrorBox error={rows.error} />}
        {rows.isPending && <Empty>正在读取收件箱…</Empty>}
        {notice && <p role="status" className="review-notice">{notice}</p>}
        {rows.data && !page.length && <Empty>{unmatched ? '当前没有待核对归属的来信。' : '当前没有符合条件的来信。'}</Empty>}
        {page.map(row => <div className="mail-selectable-row" key={row.id}>
          <input type="checkbox" aria-label={`选择邮件 ${row.subject || '无主题'}`} checked={row.id in checked} disabled={!!row.contact_id && row.match_state !== 'contact' && !row.non_customer}
            onChange={event => setChecked(previous => { const next = { ...previous }; if (event.target.checked) next[row.id] = row.handling_revision; else delete next[row.id]; return next })} />
          <button type="button" className="mail-list-item" aria-pressed={current?.id === row.id} onClick={() => select(row.id)}>
          <span className="mail-list-avatar">{unmatched ? <MailSearchIcon aria-hidden="true" /> : <Mail aria-hidden="true" />}</span>
          <span className="mail-list-content">
            <strong title={row.subject}>{row.subject || '（无主题）'}</strong>
            <small title={row.sender}>{row.sender || '无可识别发件地址'}</small>
            <span className="mail-status" data-status={row.contact_id ? 'linked' : 'unmatched'}>
              {row.non_customer ? '非客户邮件' : row.message_role === 'automatic' ? '自动来信' : row.message_role === 'delivery_report' ? '投递报告'
                : row.match_state === 'contact' && unmatched ? '会话待确认' : row.contact_name ?? '待关联'}
            </span>
            <small>{dateLabel(row.received_at)}</small>
          </span>
        </button></div>)}
      </div>
      {rows.data && <Pagination offset={offset} more={rows.data.has_more} change={next => { setOffset(next); clearSelection() }} />}
    </section>
    <div className="mail-reader">
      {current && <div className="mail-handling-actions row-actions">
        {(current.non_customer || !current.contact_id || current.match_state === 'contact') && <button type="button" className="button" disabled={classify.isPending}
          onClick={() => classify.mutate({ targets: [{ id: current.id, expected_revision: current.handling_revision }], non_customer: !current.non_customer })}>{current.non_customer ? '恢复为普通来信' : '标记非客户邮件'}</button>}
        {!unmatched && !current.non_customer && (!current.contact_id || current.match_state === 'contact') && <button type="button" className="button" onClick={() => { setFilter('review'); setManualReviewId(current.id); }}>核对 / 新建客户</button>}
      </div>}
      {current ? unmatched && !fullReview ? <ReviewMail key={current.id} row={current} openThread={() => setFullReview(true)} saved={name => {
        select()
        setNotice('已关联客户 ' + name)
        if (page.length === 1 && offset) setOffset(previous => previous - 20)
      }} /> : <MailThread key={current.id} id={current.id} close={unmatched ? () => setFullReview(false) : undefined} openContact={openContact} onReply={onReply} />
        : direct && linked.isError ? <ErrorBox error={linked.error} />
          : <div className="mail-reader-empty"><Inbox aria-hidden="true" /><p>{direct ? '正在读取关联邮件…' : '选择来信查看正文与会话'}</p></div>}
    </div>
  </div>
}

function ReviewMail({ row, openThread, saved }: { row: Row; openThread: () => void; saved: (name: string) => void }) {
  const cache = useQueryClient()
  const [filter, setFilter] = useState('')
  const [selection, setSelection] = useState<ReviewCandidate | null>(null)
  const [creating, setCreating] = useState(false)
  const keyword = useDeferredValue(filter.trim())
  const preview = useQuery({ queryKey: ['inbox-record', row.id], queryFn: () => call({ action: 'get', id: row.id }, 'message') })
  const matches = useQuery({ queryKey: ['contacts', 'inbox-review', row.id], queryFn: () => call({ action: 'review_candidates', id: row.id }, 'review_candidates') })
  const criteria: ContactQuery = {
    keyword, quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '',
    archive: 'active', sort: 'name', offset: 0,
  }
  const search = useQuery({ queryKey: ['contacts', criteria], enabled: Boolean(keyword),
    queryFn: () => request({ action: 'list_contacts', query: criteria }, 'contacts') })
  const link = useMutation({
    mutationFn: (candidate: ReviewCandidate) => call({
      action: 'review_link', id: row.id, contact_id: candidate.id,
      expected_contact: row.contact_id, expected_match: row.match_state,
      expected_handling_revision: row.handling_revision,
    }, 'saved'),
    onSuccess: (_, candidate) => {
      saved(candidate.name)
      return Promise.all(['inbox', 'inbox-record', 'inbox-thread', 'activities', 'contacts', 'contact', 'dashboard']
        .map(key => cache.invalidateQueries({ queryKey: [key] })))
    },
    onError: () => Promise.all(['inbox', 'inbox-record', 'contacts'].map(key => cache.invalidateQueries({ queryKey: [key] }))),
  })
  const candidates: ReviewCandidate[] = keyword ? (search.data?.rows ?? []).map(customer => ({
    id: customer.id, name: customer.display_name, company: customer.company_name, country: customer.country_region,
    exact_sender: matches.data?.rows.some(item => item.id === customer.id && item.exact_sender) ?? false,
    suggested_by: matches.data?.rows.find(item => item.id === customer.id)?.suggested_by,
  })) : matches.data?.rows ?? []
  const exactMatches = matches.data?.rows.filter(candidate => candidate.exact_sender).length
  if (creating) return <ContactEditor key={row.id} initialInput={{ methods: row.sender ? [{ kind: 'email', value: row.sender, label: '来信发件人', is_primary: true, is_verified: false }] : [] }}
    cancel={() => setCreating(false)} saved={id => {
      setCreating(false)
      const candidate = { id, name: '新建客户', company: '', country: '', exact_sender: true }
      setSelection(candidate)
      link.mutate(candidate)
    }} />
  return <section className="panel review-detail reply-review-detail" aria-label="邮件核验">
    <div className="section-heading"><div><span className="mail-status" data-status="unmatched"><ClipboardCheck aria-hidden="true" />待确认</span>
      <h3>{row.subject || '（无主题）'}</h3></div><button className="button" onClick={openThread}><Mail aria-hidden="true" />打开完整会话</button></div>
    <dl className="review-properties">
      <div><dt>发件人</dt><dd>{row.sender || '—'}</dd></div>
      <div><dt>收到时间</dt><dd>{dateLabel(row.received_at)}</dd></div>
      {row.message_role !== 'correspondence' && <div><dt>邮件类型</dt><dd>{row.message_role === 'automatic' ? '自动来信' : row.message_role === 'delivery_report' ? '投递报告' : '待核对'}</dd></div>}
      {row.contact_name && <div><dt>当前关联</dt><dd>{row.contact_name}</dd></div>}
    </dl>
    {preview.isError && <ErrorBox error={preview.error} />}
    {preview.isPending && <p className="muted">正在读取正文预览…</p>}
    {preview.data && <div className="review-preview">
      <p className="mail-body">{preview.data.record.body || '没有可显示的纯文本正文。'}</p>
      {!preview.data.record.body_complete && <small>正文不完整，可在完整会话中下载原邮件。</small>}
    </div>}
    <div className="reply-match-guidance"><h4>匹配依据</h4>
      {matches.data && <p>{exactMatches === 1 ? '有一位客户登记了相同的发件邮箱。请核对正文与业务背景。'
        : exactMatches ? `${exactMatches === 21 ? '至少 ' : ''}${exactMatches} 位客户登记了相同的发件邮箱，请确认属于哪一位。`
        : row.contact_name ? '此邮件已有初步归属。可以核对当前客户或搜索其他客户。'
        : '没有相同的发件邮箱。可按姓名、公司或邮箱查找客户。'}</p>}
      {row.message_role !== 'correspondence' && <p>自动来信和投递报告独立记录。</p>}
    </div>
    <Field label="查找其他客户"><input type="search" placeholder="姓名、公司或邮箱" value={filter}
      onChange={event => { setFilter(event.target.value); setSelection(null) }} /></Field>
    <button type="button" className="button" disabled={link.isPending} onClick={() => setCreating(true)}><UserRound aria-hidden="true" />从发件人新建客户</button>
    {(matches.isError || search.isError) && <ErrorBox error={search.error ?? matches.error} />}
    {((keyword && search.isPending) || (!keyword && matches.isPending)) && <small>正在查找客户…</small>}
    {candidates.length > 0 && <fieldset className="review-candidates" disabled={link.isPending}>
      <legend>{keyword ? '搜索结果' : '相关客户'}</legend>
      {candidates.map(candidate => <label key={candidate.id}><input type="radio" name="mail-review-candidate" aria-label={`关联 ${candidate.name}`}
        checked={selection?.id === candidate.id} onChange={() => setSelection(candidate)} />
        <span><strong>{candidate.name}</strong><small>{[candidate.company, candidate.country].filter(Boolean).join(' · ')}</small></span>
        {candidate.exact_sender ? <span className="review-badge">邮箱相同</span>
          : candidate.id === row.contact_id ? <span className="review-badge">当前归属</span> : null}
        {!!candidate.suggested_by?.length && <span className="review-badge">建议人 {candidate.suggested_by.join('、')}</span>}
      </label>)}
    </fieldset>}
    {!candidates.length && (keyword ? search.data : matches.data) && <p className="muted">{keyword ? '未找到符合条件的客户。' : '没有可靠的客户匹配。'}可继续搜索已有客户、从发件人新建客户，或标记为非客户邮件。</p>}
    <div className="form-footer"><button type="button" className="button primary" disabled={!selection || link.isPending}
      onClick={() => selection && link.mutate(selection)}><Link2 aria-hidden="true" />{link.isPending ? '正在关联…' : '关联所选客户'}</button></div>
    {link.isError && <ErrorBox error={link.error} />}
  </section>
}

export function MailThread({ id, threadId, close, openContact, onReply }: {
  id?: string; threadId?: string; close?: () => void; openContact: (id: string) => void; onReply?: (message: ReplyTarget) => void
}) {
  const [offset, setOffset] = useState(0)
  const [filter, setFilter] = useState('')
  const [selected, setContact] = useState('')
  const [linking, setLinking] = useState(false)
  const cache = useQueryClient()
  const inbox = useQuery({ queryKey: ['inbox-record', id], enabled: Boolean(id), queryFn: () => call({ action: 'get', id: id! }, 'message') })
  const key = threadId ?? inbox.data?.record.thread_id
  const thread = useQuery({ queryKey: ['inbox-thread', key, offset], enabled: Boolean(key),
    queryFn: () => call({ action: 'thread', id: key!, offset }, 'thread') })
  const criteria: ContactQuery = {
    keyword: filter, quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '',
    archive: 'active', sort: 'name', offset: 0,
  }
  const customer = useQuery({ queryKey: ['contacts', criteria], enabled: linking,
    queryFn: () => request({ action: 'list_contacts', query: criteria }, 'contacts') })
  const link = useMutation({
    mutationFn: () => call({ action: 'link', id: id!, contact_id: selected, expected_handling_revision: inbox.data?.record.handling_revision }, 'saved'),
    onSuccess: async () => {
      setLinking(false)
      setOffset(0)
      await Promise.all(['inbox', 'inbox-record', 'inbox-thread', 'activities', 'contacts', 'contact'].map(k =>
        cache.invalidateQueries({ queryKey: [k] })))
    },
    onError: () => Promise.all(['inbox', 'inbox-record'].map(key => cache.invalidateQueries({ queryKey: [key] }))),
  })
  const record = inbox.data?.record
  const otherMessages = thread.data?.messages.filter(message => message.id !== record?.id) ?? []
  return <section className="panel mail-reading-panel">
    <div className="section-heading">
      <div>{close && <button className="text-button back-link" onClick={close}><ArrowLeft aria-hidden="true" />返回邮件详情</button>}
        {record?.contact_name && <span className="mail-status" data-status="linked">已关联 CRM · {record.contact_name}</span>}
        <h2>{record?.subject || '邮件往来'}</h2>
        {record && <p className="muted">{record.sender || '无可识别发件地址'} · {dateLabel(record.received_at)}</p>}
      </div>
      {record && <div className="row-actions">
        {record.contact_id && <button className="button" onClick={() => openContact(record.contact_id!)}><UserRound aria-hidden="true" />客户档案</button>}
        <button className="button" onClick={() => setLinking(!linking)} aria-expanded={linking}><Link2 aria-hidden="true" />{record.contact_id ? '更正关联' : '关联客户'}</button>
      </div>}
    </div>
    {linking && <div className="inline-form">
      <div className="form-grid">
        <Field label="查找客户"><input value={filter} onChange={event => { setFilter(event.target.value); setContact('') }} /></Field>
        <Field label="选择客户"><Select value={selected} onChange={event => setContact(event.target.value)}>
          <option value="">请选择</option>{customer.data?.rows.map(row => <option value={row.id} key={row.id}>{row.display_name} {row.company_name}</option>)}
        </Select></Field>
      </div>
      {customer.isError && <ErrorBox error={customer.error} />}
      <div className="form-footer"><button className="button primary" disabled={!selected || link.isPending} onClick={() => link.mutate()}>保存关联</button></div>
      {link.isError && <ErrorBox error={link.error} />}
    </div>}
    {id && inbox.isError && <ErrorBox error={inbox.error} />}
    {thread.isError && <ErrorBox error={thread.error} />}
    {key && thread.isPending && <Empty>正在读取会话…</Empty>}
    {record && <MailMessage message={record} selected onReply={onReply} />}
    {thread.data && (otherMessages.length > 0 || thread.data.has_more || offset > 0) && <details className="mail-conversation" open={!record}>
      <summary><Mail aria-hidden="true" />{record ? '同一会话的其他邮件' : '会话邮件'}</summary>
      {otherMessages.map(message => <MailMessage message={message} key={message.id} onReply={onReply} />)}
      <Pagination offset={offset} more={thread.data.has_more} change={setOffset} pageSize={10} />
    </details>}
  </section>
}

function MailMessage({ message, selected = false, onReply }: { message: Message; selected?: boolean; onReply?: (message: ReplyTarget) => void }) {
  const contactId = message.contact_id
  return <article className="mail-message" data-selected={selected}>
    {!selected && <><h3>{message.subject}</h3><small>{message.received ? '收件' : '发件'} · {message.sender} · {dateLabel(message.received_at)}</small></>}
    {message.message_role === 'delivery_report' && <p className="muted">邮件服务投递报告</p>}
    {message.message_role === 'automatic' && <p className="muted">自动来信</p>}
    {message.match_state === 'contact' && <p className="muted">已找到同邮箱联系人，所属会话待确认。</p>}
    <p className="mail-body">{message.body || '此邮件没有可显示的纯文本内容。'}</p>
    {message.received && <InboxContents message={message} />}
    {onReply && contactId && message.received && message.message_role === 'correspondence' && ['thread', 'manual'].includes(message.match_state) &&
      <button type="button" className="button" onClick={() => onReply({ ...message, contact_id: contactId })}><Reply aria-hidden="true" />回复此邮件</button>}
  </article>
}

function InboxContents({ message }: { message: Message }) {
  const cache = useQueryClient()
  const [offset, setOffset] = useState(0)
  const [expanded, setExpanded] = useState(false)
  const body = useQuery({
    queryKey: ['inbox-body',message.id,offset], enabled: expanded,
    queryFn: () => call({ action: 'read_body', id: message.id, offset }, 'body'),
  })
  const download = useMutation({
    mutationFn: () => call({ action: 'download_full', id: message.id }, 'saved'),
    onSuccess: () => Promise.all([
      cache.invalidateQueries({ queryKey: ['inbox-thread'] }),
      cache.invalidateQueries({ queryKey: ['inbox-record', message.id] }),
    ]),
  })
  const pending = message.download_outcome === 'running'
  return <>
    {download.isError && <ErrorBox error={download.error} />}
    {!message.body_complete && <>
      {message.download_outcome && !['running','ok'].includes(message.download_outcome)
        && <ErrorBox error={message.download_outcome} />}
      <button className="button" disabled={download.isPending || pending} onClick={() => download.mutate()}>
        <Paperclip aria-hidden="true" />{pending ? '正在下载完整邮件…' : '下载完整邮件（最大 16 MiB）'}
      </button></>}
    {message.body_complete && <>
      <button className="button" onClick={() => { setExpanded(value => !value); setOffset(0) }}>{expanded ? '关闭完整正文' : '读取完整正文'}</button>
      {expanded && <>
        {body.isError && <ErrorBox error={body.error} />}
        {body.isPending && <p className="muted">正在读取正文…</p>}
        {body.data && <>
          <p className="mail-body">{body.data.text || '没有纯文本正文。'}</p>
          <div className="row-actions">{offset > 0 && <button className="button" onClick={() => setOffset(0)}>回到开头</button>}
            {body.data.next_offset !== null && <button className="button" onClick={() => setOffset(body.data.next_offset!)}>继续读取</button>}
          </div>
        </>}
      </>}
    </>}
    {message.attachments.length > 0 && <div className="mail-attachments">
      <h3>附件（{message.attachments.length}）</h3>
      <ul className="record-list">{message.attachments.map((filename, index) => <li key={index}>
        <span><Paperclip aria-hidden="true" />{filename}</span>
        {message.body_complete && <AttachmentActions source="inbox" id={message.id} index={index} filename={filename} />}
      </li>)}</ul>
    </div>}
  </>
}
