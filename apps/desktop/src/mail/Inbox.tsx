import { invoke } from '@tauri-apps/api/core'
import { useDeferredValue, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateLabel, request, type ContactQuery } from '../crm/api'
import { Empty, Field, Pagination } from '../crm/components'
import { mailError } from './api'
import { CustomerIcon } from '../crm/CustomerIcon'

type Account = {
  account_id: string; display_name: string; email_address: string; host: string | null
  port: number | null; username: string | null; last_sync: number | null; outcome: string | null
}
type Row = {
  id: string; account_id: string; contact_id: string | null; contact_name: string | null
  thread_id: string; sender: string; subject: string; received_at: number; kind: string; received: boolean; message_role: string; match_state: string; body_complete: boolean; download_outcome: string | null
}
type ReviewCandidate = { id: string; name: string; company: string; country: string; exact_sender: boolean }
type Message = Row & { body: string; attachments: string[] }
type Reply =
  | { kind: 'body'; text: string; next_offset: number | null }
  | { kind: 'accounts'; rows: Account[] }
  | { kind: 'rows'; rows: Row[]; has_more: boolean }
  | { kind: 'review_candidates'; rows: ReviewCandidate[] }
  | { kind: 'message'; record: Message }
  | { kind: 'thread'; messages: Message[]; has_more: boolean }
  | { kind: 'saved'; id: string }
  | { kind: 'sync_started'; account_id: string }
  | { kind: 'connection_verified' }

type Command =
  | { action: 'accounts' }
  | { action: 'configure'; account_id: string; host: string; port: number; username: string; password: string }
  | { action: 'test' | 'sync'; account_id: string }
  | { action: 'list'; account_id: string | null; contact_id: string | null; unmatched: boolean; offset: number }
  | { action: 'review_candidates'; id: string }
  | { action: 'review_link'; id: string; contact_id: string; expected_contact: string | null; expected_match: string }
  | { action: 'get'; id: string }
  | { action: 'thread'; id: string; offset: number }
  | { action: 'download_full'; id: string }
  | { action: 'read_body'; id: string; offset: number }
  | { action: 'link'; id: string; contact_id: string }

async function call<K extends Reply['kind']>(command: Command, expected: K): Promise<Extract<Reply, { kind: K }>> {
  const result = await invoke<Reply>('inbox_request', { command })
  if (result.kind !== expected) throw new Error('收到意外的收件服务响应。')
  return result as Extract<Reply, { kind: K }>
}

const syncLabels: Record<string, string> = {
  running: '同步中', ok: '已同步', more_available: '本批已同步，仍有较早或新增来信可继续取回',
  interrupted: '上次同步中断', imap_timeout: '同步超时',
}

function ErrorBox({ error }: { error: unknown }) {
  return <p role="alert" className="alert error">{mailError(error)}</p>
}

function useAccounts() {
  return useQuery({ queryKey: ['imap-accounts'], queryFn: () => call({ action: 'accounts' }, 'accounts') })
}

export function ImapSettings() {
  const accounts = useAccounts()
  const cache = useQueryClient()
  const [selected, setSelected] = useState('')
  const [notice, setNotice] = useState('')
  const account = accounts.data?.rows.find(row => row.account_id === selected)
  const mutate = useMutation({
    mutationFn: async (command: Command) => command.action === 'configure' ? call(command, 'saved') : call(command, 'connection_verified'),
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
      action: 'configure', account_id: selected, host: String(data.get('host')), port: Number(data.get('port')),
      username: String(data.get('username')), password: String(data.get('password')),
    }, { onSettled: () => { const entry = form.elements.namedItem('password'); if (entry instanceof HTMLInputElement) entry.value = '' } })
  }
  return <section className="panel">
    <h2>IMAP 收件设置</h2>
    {accounts.isError && <ErrorBox error={accounts.error} />}
    {accounts.data?.rows.length === 0 && <Empty>请先在「发件账户」添加邮箱。</Empty>}
    {accounts.data && accounts.data.rows.length > 0 && <>
      <Field label="邮箱账户"><select value={selected} onChange={e => { setSelected(e.target.value); setNotice(''); mutate.reset() }}>
        <option value="">请选择</option>{accounts.data.rows.map(row => <option value={row.account_id} key={row.account_id}>{row.display_name} · {row.email_address}</option>)}
      </select></Field>
      {account && <form className="inline-form" key={selected} onSubmit={save}>
        <div className="form-grid">
          <Field label="IMAP 主机"><input name="host" placeholder="imap.example.com" defaultValue={account.host ?? ''} required /></Field>
          <Field label="TLS 端口"><input name="port" type="number" min="1" max="65535" defaultValue={account.port ?? 993} required /></Field>
          <Field label="IMAP 登录名"><input name="username" defaultValue={account.username ?? account.email_address} required /></Field>
          <Field label="IMAP 授权码或密码"><input type="password" name="password" autoComplete="new-password" required /></Field>
        </div>
        <div className="form-footer"><button className="button" type="button" disabled={!account.host || mutate.isPending}
          onClick={() => mutate.mutate({ action: 'test', account_id: selected })}>测试 IMAP</button>
          <button className="button primary" disabled={mutate.isPending}>保存收件设置</button></div>
      </form>}
    </>}
    {notice && <p className="muted" role="status">{notice}</p>}
    {mutate.isError && <ErrorBox error={mutate.error} />}
  </section>
}

export function InboxView({ contactId, initiallyReviewing, openContact }: { contactId?: string; initiallyReviewing?: boolean; openContact: (id: string) => void }) {
  const accounts = useAccounts()
  const [accountId, setAccount] = useState('')
  const [unmatched, setUnmatched] = useState(Boolean(!contactId && initiallyReviewing))
  const [reviewId, setReviewId] = useState('')
  const [notice, setNotice] = useState('')
  const [offset, setOffset] = useState(0)
  const [opened, setOpened] = useState<string | null>(null)
  const active = accounts.data?.rows.find(row => row.account_id === accountId)
  const cache = useQueryClient()
  const rows = useQuery({
    queryKey: ['inbox', accountId, contactId, unmatched, offset],
    queryFn: () => call({ action: 'list', account_id: accountId || null, contact_id: contactId ?? null, unmatched, offset }, 'rows'),
  })
  const sync = useMutation({
    mutationFn: () => call({ action: 'sync', account_id: accountId }, 'sync_started'),
    onSuccess: () => cache.invalidateQueries({ queryKey: ['imap-accounts'] }),
  })
  const page = rows.data?.rows ?? []
  const current = page.find(row => row.id === reviewId) ?? page[0]
  if (opened) return <MailThread id={opened} close={() => setOpened(null)} openContact={openContact} />
  return <section className="panel inbox-workspace">
    <div className="section-heading"><h2>{contactId ? '此客户的收件记录' : unmatched ? '客户回复匹配与待确认' : '收件箱'}</h2>
      <button className="button primary" disabled={!active?.host || sync.isPending || active.outcome === 'running'}
        onClick={() => sync.mutate()}>{active?.outcome === 'running' ? '同步中…' : '同步当前账户'}</button></div>
    <div className="filters">
      <Field label="邮箱账户"><select value={accountId} onChange={e => { setAccount(e.target.value); setOffset(0); sync.reset() }}>
        <option value="">全部邮箱</option>{accounts.data?.rows.map(row => <option key={row.account_id} value={row.account_id}>{row.display_name}</option>)}
      </select></Field>
      {!contactId && <div className="row-actions" aria-label="收件箱视图">
        <button type="button" className="button" aria-pressed={!unmatched} onClick={() => { setUnmatched(false); setOffset(0); setNotice('') }}>全部来信</button>
        <button type="button" className="button" aria-pressed={unmatched} onClick={() => { setUnmatched(true); setOffset(0); setNotice('') }}>待核对归属</button>
      </div>}
    </div>
    {active && <p className="muted">{active.host ? (active.outcome ? syncLabels[active.outcome] ?? mailError(active.outcome) : '尚未同步') : '请在收件设置中配置 IMAP'}
      {active.last_sync && <> · {dateLabel(active.last_sync)}</>}</p>}
    {accounts.isError && <ErrorBox error={accounts.error} />}
    {sync.isError && <ErrorBox error={sync.error} />}
    {rows.isError && <ErrorBox error={rows.error} />}
    {rows.isPending && <Empty>正在读取收件箱…</Empty>}
    {notice && <p role="status" className="review-notice">{notice}</p>}
    {rows.data?.rows.length === 0 && <Empty>{unmatched ? '当前没有待核对归属的来信。' : '当前没有符合条件的来信。'}</Empty>}
    {unmatched && current && <div className="review-layout">
      <section className="panel review-list" aria-label="待核对邮件">
        <h3>待核对邮件</h3>
        {page.map(row => <button className="review-list-item" key={row.id} aria-pressed={current.id === row.id}
          onClick={() => { setReviewId(row.id); setNotice('') }}>
          <span className="review-item-icon"><CustomerIcon name="mail" /></span>
          <span><strong>{row.subject || '（无主题）'}</strong><small>{row.sender || '无可识别发件地址'} · {dateLabel(row.received_at)}</small></span>
          <span className="review-badge">{row.message_role === 'automatic' ? '自动来信' : row.message_role === 'delivery_report' ? '投递报告' : row.contact_id ? '会话待确认' : '待关联'}</span>
        </button>)}
      </section>
      <ReviewMail key={current.id} row={current} openThread={() => setOpened(current.id)} saved={name => {
        setReviewId('')
        setNotice('已关联客户 ' + name)
        if (page.length === 1 && offset) setOffset(previous => previous - 20)
      }} />
    </div>}
    {!unmatched && !!rows.data?.rows.length && <div className="table-wrap"><table>
      <thead><tr><th>主题</th><th>发件邮箱</th><th>客户</th><th>收件时间</th></tr></thead>
      <tbody>{rows.data.rows.map(row => <tr key={row.id}>
        <td><button className="text-button" onClick={() => setOpened(row.id)}>{row.subject}</button></td><td>{row.sender || '—'}</td>
        <td>{row.contact_name ?? '待关联'}</td><td>{dateLabel(row.received_at)}</td>
      </tr>)}</tbody>
    </table></div>}
    {rows.data && <Pagination offset={offset} more={rows.data.has_more} change={setOffset} />}
  </section>
}

function ReviewMail({ row, openThread, saved }: { row: Row; openThread: () => void; saved: (name: string) => void }) {
  const cache = useQueryClient()
  const [filter, setFilter] = useState('')
  const [selection, setSelection] = useState<ReviewCandidate | null>(null)
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
    }, 'saved'),
    onSuccess: (_, candidate) => {
      saved(candidate.name)
      return Promise.all(['inbox', 'inbox-record', 'inbox-thread', 'activities', 'contacts', 'contact', 'dashboard']
        .map(key => cache.invalidateQueries({ queryKey: [key] })))
    },
    onError: () => cache.invalidateQueries({ queryKey: ['inbox'] }),
  })
  const candidates = keyword ? (search.data?.rows ?? []).map(customer => ({
    id: customer.id, name: customer.display_name, company: customer.company_name, country: customer.country_region,
    exact_sender: matches.data?.rows.some(item => item.id === customer.id && item.exact_sender) ?? false,
  })) : matches.data?.rows ?? []
  return <section className="panel review-detail" aria-label="邮件核验">
    <div className="section-heading"><h3>{row.subject || '（无主题）'}</h3><button className="button" onClick={openThread}>打开完整会话</button></div>
    <dl className="review-properties">
      <div><dt>发件人</dt><dd>{row.sender || '—'}</dd></div>
      <div><dt>收到时间</dt><dd>{dateLabel(row.received_at)}</dd></div>
      {row.contact_name && <div><dt>当前关联</dt><dd>{row.contact_name}</dd></div>}
    </dl>
    {preview.isError && <ErrorBox error={preview.error} />}
    {preview.isPending && <p className="muted">正在读取正文预览…</p>}
    {preview.data && <div className="review-preview">
      <p className="mail-body">{preview.data.record.body || '没有可显示的纯文本正文。'}</p>
      {!preview.data.record.body_complete && <small>正文不完整，可在完整会话中下载原邮件。</small>}
    </div>}
    <Field label="查找其他客户"><input type="search" placeholder="姓名、公司或邮箱" value={filter}
      onChange={event => { setFilter(event.target.value); setSelection(null) }} /></Field>
    {(matches.isError || search.isError) && <ErrorBox error={search.error ?? matches.error} />}
    {((keyword && search.isPending) || (!keyword && matches.isPending)) && <small>正在查找客户…</small>}
    {candidates.length > 0 && <fieldset className="review-candidates" disabled={link.isPending}>
      <legend>{keyword ? '搜索结果' : '相关客户'}</legend>
      {candidates.map(candidate => <label key={candidate.id}><input type="radio" name="mail-review-candidate"
        checked={selection?.id === candidate.id} onChange={() => setSelection(candidate)} />
        <span><strong>{candidate.name}</strong><small>{[candidate.company, candidate.country].filter(Boolean).join(' · ')}</small></span>
        {candidate.exact_sender && <span className="review-badge">邮箱相同</span>}
      </label>)}
    </fieldset>}
    {!candidates.length && (keyword ? search.data : matches.data) && <small>{keyword ? '未找到符合条件的客户。' : '没有可靠的邮箱匹配，请搜索客户核对。'}</small>}
    <div className="form-footer"><button type="button" className="button primary" disabled={!selection || link.isPending}
      onClick={() => selection && link.mutate(selection)}>{link.isPending ? '正在关联…' : '关联所选客户'}</button></div>
    {link.isError && <ErrorBox error={link.error} />}
  </section>
}

export function MailThread({ id, threadId, close, openContact }: {
  id?: string; threadId?: string; close: () => void; openContact: (id: string) => void
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
    mutationFn: () => call({ action: 'link', id: id!, contact_id: selected }, 'saved'),
    onSuccess: async () => {
      setLinking(false)
      setOffset(0)
      await Promise.all(['inbox', 'inbox-record', 'inbox-thread', 'activities', 'contacts', 'contact'].map(k =>
        cache.invalidateQueries({ queryKey: [k] })))
    },
  })
  const record = inbox.data?.record
  return <section className="panel">
    <div className="section-heading">
      <div><button className="text-button back-link" onClick={close}>返回邮件列表</button><h2>{record?.subject ?? '邮件往来'}</h2></div>
      {record && <div className="row-actions">
        {record.contact_id && <button className="button" onClick={() => openContact(record.contact_id!)}>{record.contact_name ?? '查看客户'}</button>}
        <button className="button" onClick={() => setLinking(!linking)}>{record.contact_id ? '更正关联' : '关联客户'}</button>
      </div>}
    </div>
    {linking && <div className="inline-form">
      <div className="form-grid">
        <Field label="查找客户"><input value={filter} onChange={event => { setFilter(event.target.value); setContact('') }} /></Field>
        <Field label="选择客户"><select value={selected} onChange={event => setContact(event.target.value)}>
          <option value="">请选择</option>{customer.data?.rows.map(row => <option value={row.id} key={row.id}>{row.display_name} {row.company_name}</option>)}
        </select></Field>
      </div>
      {customer.isError && <ErrorBox error={customer.error} />}
      <div className="form-footer"><button className="button primary" disabled={!selected || link.isPending} onClick={() => link.mutate()}>保存关联</button></div>
      {link.isError && <ErrorBox error={link.error} />}
    </div>}
    {id && inbox.isError && <ErrorBox error={inbox.error} />}
    {thread.isError && <ErrorBox error={thread.error} />}
    {key && thread.isPending && <Empty>正在读取会话…</Empty>}
    {thread.data?.messages.map(message => <article className="inline-form" key={message.id}>
      <h3>{message.subject}</h3><small>{message.received ? '收件' : '发件'} · {message.sender} · {dateLabel(message.received_at)}</small>
      {message.message_role === 'delivery_report' && <p className="muted">邮件服务投递报告</p>}
      {message.message_role === 'automatic' && <p className="muted">自动来信</p>}
      {message.match_state === 'contact' && <p className="muted">已找到同邮箱联系人，所属会话待确认。</p>}
      <p className="mail-body">{message.body || '此邮件没有可显示的纯文本内容。'}</p>
      {message.received && <InboxContents message={message} />}
    </article>)}
    {thread.data && <Pagination offset={offset} more={thread.data.has_more} change={setOffset} pageSize={10} />}
  </section>
}

function InboxContents({ message }: { message: Message }) {
  const cache = useQueryClient()
  const [offset, setOffset] = useState(0)
  const [expanded, setExpanded] = useState(false)
  const [saved, setSaved] = useState('')
  const body = useQuery({
    queryKey: ['inbox-body',message.id,offset], enabled: expanded,
    queryFn: () => call({ action: 'read_body', id: message.id, offset }, 'body'),
  })
  const download = useMutation({
    mutationFn: () => call({ action: 'download_full', id: message.id }, 'saved'),
    onSuccess: () => cache.invalidateQueries({ queryKey: ['inbox-thread'] }),
  })
  const save = useMutation({
    mutationFn: ({filename,index}:{filename:string;index:number}) =>
      invoke<boolean>('save_inbox_attachment',{ id: message.id, filename, index }),
    onSuccess: (written,part) => { if (written) setSaved(part.filename) },
  })
  const pending = message.download_outcome === 'running'
  return <>
    {download.isError && <ErrorBox error={download.error} />}
    {!message.body_complete && <>
      {message.download_outcome && !['running','ok'].includes(message.download_outcome)
        && <ErrorBox error={message.download_outcome} />}
      <button className="button" disabled={download.isPending || pending} onClick={() => download.mutate()}>
        {pending ? '正在下载完整邮件…' : '下载完整邮件（最大 16 MiB）'}
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
      {message.attachments.length > 0 && <ul className="record-list">{message.attachments.map((filename,index) => <li key={index}>
        <span>{filename}</span><button className="button" disabled={save.isPending} onClick={() => save.mutate({filename,index})}>另存为</button>
      </li>)}</ul>}
      {saved && <p className="muted" role="status">已保存 {saved}</p>}
      {save.isError && <ErrorBox error={save.error} />}
    </>}
  </>
}
