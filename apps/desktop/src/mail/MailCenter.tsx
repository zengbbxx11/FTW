import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateLabel, request, sourceLabel, type Contact } from '../crm/api'
import { ConfirmButton, Empty, Field, Pagination } from '../crm/components'
import { ImapSettings, InboxView, MailThread } from './Inbox'
import { mailCall, mailError, states, type Attachment, type Draft, type DraftInput, type MailCommand } from './api'

type View = { kind: 'drafts' | 'accounts' | 'inbox' | 'imap' } | { kind: 'draft' | 'edit'; id: string }

function ErrorNotice({ error }: { error: unknown }) {
  return <p className="alert error" role="alert">{mailError(error)}</p>
}

export function MailCenter({ initialContact, inboxContact, initialInbox, pickContact, openContact }: {
  initialContact?: Contact; inboxContact?: string; initialInbox?: boolean; pickContact: () => void; openContact: (id: string) => void
}) {
  const [view, setView] = useState<View>({ kind: (initialInbox || inboxContact) ? 'inbox' : 'drafts' })
  const [compose, setCompose] = useState(initialContact !== undefined)
  const [offset, setOffset] = useState(0)
  const openDraft = (id: string) => { setCompose(false); setView({ kind: 'draft', id }) }
  const showList = () => { setCompose(false); setView({ kind: 'drafts' }) }
  const drafts = useQuery({
    queryKey: ['mail-drafts', offset], enabled: !compose && view.kind === 'drafts',
    queryFn: () => mailCall({ action: 'list_drafts', contact_id: null, offset }, 'drafts'),
    refetchInterval: !compose && view.kind === 'drafts' ? 4500 : false,
  })
  return <>
    <div className="page-heading"><h1>邮件中心</h1>
      <div className="row-actions">
        <button className="button" onClick={showList}>邮件草稿</button>
        <button className="button" onClick={() => { setCompose(false); setView({ kind: 'inbox' }) }}>收件箱</button>
        <button className="button" onClick={() => { setCompose(false); setView({ kind: 'accounts' }) }}>发件账户</button>
        <button className="button" onClick={() => { setCompose(false); setView({ kind: 'imap' }) }}>收件设置</button>
        <button className="button primary" onClick={initialContact ? () => setCompose(true) : pickContact}>从客户写邮件</button>
      </div>
    </div>
    {compose && initialContact ? <DraftForm contact={initialContact} saved={openDraft} close={showList} />
      : view.kind === 'accounts' ? <Accounts />
      : view.kind === 'imap' ? <ImapSettings />
      : view.kind === 'inbox' ? <InboxView contactId={inboxContact} openContact={openContact} />
      : view.kind === 'draft' || view.kind === 'edit' ?
        <DraftView key={view.id} id={view.id} editing={view.kind === 'edit'} openContact={openContact}
          edit={() => setView({ kind: 'edit', id: view.id })} saved={openDraft} back={showList} />
      : <>
        {drafts.isError && <ErrorNotice error={drafts.error} />}
        {drafts.isPending && <Empty>正在读取邮件草稿…</Empty>}
        {drafts.data?.kind === 'drafts' && (drafts.data.rows.length === 0 ? <Empty>当前没有邮件草稿。</Empty>
          : <><div className="table-wrap"><table><thead><tr><th>主题</th><th>收件邮箱</th><th>状态</th><th>来源</th></tr></thead>
            <tbody>{drafts.data.rows.map(row => <tr key={row.id}>
              <td><button className="text-button" onClick={() => openDraft(row.id)}>{row.subject}</button></td>
              <td>{row.recipient}</td><td>{states[row.state]}</td><td>{sourceLabel(row.source)}</td>
            </tr>)}</tbody></table></div><Pagination offset={offset} more={drafts.data.has_more} change={setOffset} /></>)}
      </>}
  </>
}

function Accounts() {
  const cache = useQueryClient()
  const rows = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const [security, setSecurity] = useState<'tls' | 'starttls'>('tls')
  const [port, setPort] = useState(465)
  const [notice, setNotice] = useState('')
  const [editing, setEditing] = useState('')
  const [preferences, setPreferences] = useState('')
  const action = useMutation({
    mutationFn: async (command: MailCommand) => mailCall(command, command.action === 'test_account' ? 'connection_verified' : 'saved'),
    onMutate: () => setNotice(''),
    onSuccess: async (result, command) => {
      if (command.action === 'replace_password') setEditing('')
      if (command.action === 'set_account_preferences') setPreferences('')
      setNotice(result.kind === 'connection_verified' ? 'SMTP 连接和认证通过。' :
        command.action === 'set_suppression' ? '拒发规则已保存。' : '邮箱账户设置已保存。')
      await cache.invalidateQueries({ queryKey: ['mail-accounts'] })
    },
  })
  const create = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const field = (key: string) => String(data.get(key) ?? '')
    action.mutate({ action: 'save_account', name: field('name'), email: field('email'), host: field('host'),
      port, security, username: field('username'), password: field('password') }, {
      onSettled: () => { const password = form.elements.namedItem('password'); if (password instanceof HTMLInputElement) password.value = '' },
      onSuccess: () => form.reset(),
    })
  }
  return <>
    <section className="panel"><h2>发件账户</h2>
      {rows.isError && <ErrorNotice error={rows.error} />}
      {rows.data?.kind === 'accounts' && (rows.data.rows.length === 0 ? <p className="muted">尚未配置账户。</p> :
        <ul className="record-list">{rows.data.rows.map(account => <li key={account.id}>
          <strong>{account.name}</strong><span>{account.email}</span><small>{account.host}:{account.port}</small>
          <button className="button" disabled={action.isPending} onClick={() => action.mutate({ action: 'test_account', id: account.id })}>测试 SMTP</button>
          <button className="button" disabled={action.isPending} onClick={() => setEditing(account.id)}>更新授权码</button>
          <button className="button" disabled={action.isPending} onClick={() => setPreferences(account.id)}>发送上限与签名</button>
          {preferences === account.id && <form className="form-grid" onSubmit={event => {
            event.preventDefault()
            const form = new FormData(event.currentTarget)
            action.mutate({ action: 'set_account_preferences', id: account.id,
              daily_limit: Number(form.get('limit')), signature: String(form.get('signature') ?? '') })
          }}>
            <Field label="近 24 小时发送上限"><input name="limit" type="number" min="1" max="500" defaultValue={account.daily_limit ?? 30} required /></Field>
            <Field label="纯文本签名"><textarea name="signature" rows={3} defaultValue={account.signature ?? ''} /></Field>
            <div className="row-actions"><button className="button" disabled={action.isPending}>保存设置</button>
              <button type="button" className="button" onClick={() => setPreferences('')}>取消</button></div>
          </form>}
          {editing === account.id && <form className="row-actions" onSubmit={event => {
            event.preventDefault()
            const form = event.currentTarget
            const password = String(new FormData(form).get('password') ?? '')
            action.mutate({ action: 'replace_password', id: account.id, password }, { onSettled: () => form.reset() })
          }}>
            <Field label="新授权码或密码"><input autoFocus name="password" type="password" autoComplete="new-password" required /></Field>
            <button className="button" disabled={action.isPending}>保存授权码</button>
            <button type="button" className="button" onClick={() => setEditing('')}>取消</button>
          </form>}
        </li>)}</ul>)}
      {notice && <p role="status" className="muted">{notice}</p>}
      {action.isError && <ErrorNotice error={action.error} />}
    </section>
    <form onSubmit={create}><fieldset className="panel" disabled={action.isPending}>
      <h2>新增 SMTP 账户</h2>
      <div className="form-grid">
        <Field label="账户名称"><input name="name" required /></Field>
        <Field label="发件邮箱"><input name="email" type="email" required /></Field>
        <Field label="SMTP 主机"><input name="host" placeholder="smtp.example.com" required /></Field>
        <Field label="加密方式"><select value={security} onChange={e => {
          const value = e.target.value as typeof security
          setSecurity(value)
          setPort(value === 'tls' ? 465 : 587)
        }}><option value="tls">TLS</option><option value="starttls">STARTTLS（强制）</option></select></Field>
        <Field label="端口"><input type="number" min="1" max="65535" value={port || ''} onChange={e => setPort(e.target.valueAsNumber)} required /></Field>
        <Field label="登录名"><input name="username" required /></Field>
        <Field label="应用授权码或密码"><input name="password" type="password" autoComplete="new-password" required /></Field>
      </div>
      <div className="form-footer"><button className="button primary">保存发件账户</button></div>
    </fieldset></form>
    <form className="panel" onSubmit={event => {
      event.preventDefault()
      const data = new FormData(event.currentTarget)
      action.mutate({ action: 'set_suppression', target: String(data.get('target') ?? ''), blocked: data.get('operation') === 'block' })
    }}>
      <h2>拒发规则</h2>
      <div className="form-grid">
        <Field label="邮箱或域名"><input name="target" required /></Field>
        <Field label="操作"><select name="operation"><option value="block">加入拒发</option><option value="unblock">移除拒发</option></select></Field>
      </div>
      <div className="form-footer"><button className="button" disabled={action.isPending}>保存规则</button></div>
    </form>
  </>
}

function DraftForm({ contact, existing, saved, close }: {
  contact: Contact; existing?: Draft; saved: (id: string) => void; close: () => void
}) {
  const emails = contact.input.methods.filter(method => method.kind === 'email')
  const [account, setAccount] = useState(existing?.account_id ?? '')
  const [recipient, setRecipient] = useState(existing?.recipient ?? (emails.find(m => m.is_primary) ?? emails[0])?.value ?? '')
  const [subject, setSubject] = useState(existing?.subject ?? '')
  const [body, setBody] = useState(existing?.body ?? '')
  const [files, setFiles] = useState<Attachment[]>([])
  const [replace, setReplace] = useState(false)
  const [fileError, setFileError] = useState('')
  const [readingFiles, setReadingFiles] = useState(false)
  const cache = useQueryClient()
  const accounts = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const senderRows = accounts.data?.kind === 'accounts' ? accounts.data.rows : []
  const sender = account || senderRows[0]?.id || ''
  const currentRecipient = emails.find(item => item.value.toLowerCase() === recipient.toLowerCase())?.value ?? recipient
  const save = useMutation({
    mutationFn: () => {
      const input: DraftInput = { contact_id: contact.id, account_id: sender, recipient: currentRecipient,
        subject, body, attachments: files, replace_attachments: replace }
      return mailCall({ action: 'save_draft', id: existing?.id ?? null, input }, 'saved')
    },
    onSuccess: async result => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['mail-drafts'] }), cache.invalidateQueries({ queryKey: ['mail-draft', result.id] })])
      saved(result.id)
    },
  })
  const selectFiles = async (input: FileList | null) => {
    const selected = [...(input ?? [])]
    setFileError('')
    if (selected.length > 4 || selected.reduce((sum, file) => sum + file.size, 0) > 384 * 1024) {
      setFileError('最多选择 4 个附件，总大小上限 384 KiB。')
      return
    }
    if (!selected.length) return
    setReadingFiles(true)
    try {
      const parts = await Promise.all(selected.map(file => new Promise<Attachment>((resolve, reject) => {
        const reader = new FileReader()
        reader.onerror = reject
        reader.onload = () => {
          const encoded = String(reader.result)
          resolve({ filename: file.name, content_base64: encoded.slice(encoded.indexOf(',') + 1) })
        }
        reader.readAsDataURL(file)
      })))
      setFiles(parts)
      setReplace(true)
    } catch { setFileError('附件读取失败。') }
    finally { setReadingFiles(false) }
  }
  return <form onSubmit={event => { event.preventDefault(); save.mutate() }}>
    <fieldset className="panel" disabled={save.isPending}>
      <div className="section-heading"><h2>{existing ? '编辑邮件草稿' : '写邮件'} · {contact.input.display_name}</h2>
        <button type="button" className="button" onClick={close}>取消</button></div>
      {accounts.isError && <ErrorNotice error={accounts.error} />}
      {accounts.data && senderRows.length === 0 && <p className="alert warning">请先在「发件账户」配置 SMTP。</p>}
      {emails.length === 0 && <p className="alert warning">请先在客户资料中添加邮箱。</p>}
      <div className="form-grid">
        <Field label="发件账户"><select value={sender} onChange={e => setAccount(e.target.value)} required>
          <option value="" disabled>选择账户</option>{senderRows.map(row => <option key={row.id} value={row.id}>{row.name} · {row.email}</option>)}</select></Field>
        <Field label="收件邮箱"><select value={currentRecipient} onChange={e => setRecipient(e.target.value)} required>
          <option value="" disabled>选择邮箱</option>{emails.map(row => <option key={row.value} value={row.value}>{row.value}</option>)}</select></Field>
        <Field label="主题"><input value={subject} onChange={e => setSubject(e.target.value)} required /></Field>
      </div>
      <Field label="正文"><textarea rows={10} value={body} onChange={e => setBody(e.target.value)} required /></Field>
      {senderRows.find(row => row.id === sender)?.signature && <button type="button" className="button" onClick={() => {
        const signature = senderRows.find(row => row.id === sender)?.signature
        if (signature && !body.trimEnd().endsWith(signature)) setBody(body.trimEnd() + (body.trim() ? '\n\n' : '') + signature)
      }}>插入账户签名</button>}
      <Field label="附件"><input type="file" multiple disabled={readingFiles} onChange={e => void selectFiles(e.target.files)} /></Field>
      {(replace ? files : existing?.attachments ?? []).map((file, i) => <small key={i}>{file.filename}</small>)}
      {(files.length > 0 || Boolean(existing?.attachments.length && !replace)) && <button className="text-button" type="button"
        onClick={() => { setFiles([]); setReplace(true); setFileError('') }}>移除所有附件</button>}
      {fileError && <ErrorNotice error={fileError} />}
      {save.isError && <ErrorNotice error={save.error} />}
      <div className="form-footer"><button className="button primary" disabled={!sender || !emails.length || readingFiles || !!fileError}>保存草稿</button></div>
    </fieldset>
  </form>
}

function DraftView({ id, editing, edit, back, saved, openContact }: {
  id: string; editing: boolean; edit: () => void; back: () => void; saved: (id: string) => void; openContact: (id: string) => void
}) {
  const cache = useQueryClient()
  const [prompt, setPrompt] = useState(false)
  const [history, setHistory] = useState(false)
  const draft = useQuery({
    queryKey: ['mail-draft', id], queryFn: () => mailCall({ action: 'get_draft', id }, 'draft'),
    refetchInterval: editing ? false : 1800,
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
    onSuccess: async result => {
      if (result.kind === 'approval_opened') setPrompt(true)
      await Promise.all([cache.invalidateQueries({ queryKey: ['mail-drafts'] }), cache.invalidateQueries({ queryKey: ['mail-draft', id] })])
    },
  })
  if (draft.isError) return <><button className="button" onClick={back}>返回草稿</button><ErrorNotice error={draft.error} /></>
  if (!record) return <Empty>正在读取草稿…</Empty>
  if (history && record.thread_id) return <MailThread threadId={record.thread_id} close={() => setHistory(false)} openContact={openContact} />
  if (editing) return contact.isError ? <ErrorNotice error={contact.error} /> : contact.data ?
    <DraftForm contact={contact.data.record} existing={record} saved={saved} close={() => saved(id)} /> : <Empty>正在读取客户…</Empty>

  const expired = record.expires_at !== null && Date.now() / 1000 >= record.expires_at
  return <section className="panel">
    <button className="text-button back-link" onClick={back}>返回邮件草稿</button>
    <div className="section-heading">
      <div><h2>{record.subject}</h2><p className="muted">{states[record.state]}{record.has_reply ? ' · 已有回复' : ''} · 修订 {record.revision} · {sourceLabel(record.source)}</p></div>
      <div className="row-actions">
        <button className="button" onClick={() => openContact(record.contact_id)}>打开客户</button>
        {record.thread_id && <button className="button" onClick={() => setHistory(true)}>邮件往来</button>}
        {['draft', 'pending_approval', 'approved'].includes(record.state) && <button className="button" onClick={edit}>编辑草稿</button>}
        {record.state === 'draft' && <button className="button primary" disabled={transition.isPending} onClick={() => transition.mutate('request_approval')}>提交审核</button>}
        {(record.state === 'pending_approval' || (record.state === 'approved' && expired)) &&
          <button className="button primary" disabled={transition.isPending} onClick={() => transition.mutate('open_approval')}>人工批准</button>}
        {record.state === 'approved' && !expired &&
          <ConfirmButton label="将该批准版本加入发件队列？" disabled={transition.isPending}
            onConfirm={() => transition.mutate('send_approved')}>发送已批准版本</ConfirmButton>}
      </div>
    </div>
    {prompt && record.state === 'pending_approval' && <p className="alert warning">请在 FTW 的 Windows 审批窗口确认。取消后可重新发起审批。</p>}
    {transition.isError && <ErrorNotice error={transition.error} />}
    <dl className="profile-grid">
      <div><dt>发件人</dt><dd>{record.sender}</dd></div>
      <div><dt>收件人</dt><dd>{record.recipient}</dd></div>
      {record.expires_at !== null && <div><dt>批准有效至</dt><dd>{dateLabel(record.expires_at)}</dd></div>}
    </dl>
    <h3>正文</h3><p className="mail-body">{record.body}</p>
    {record.attachments.length > 0 && <><h3>附件</h3><ul className="source-list">{record.attachments.map(file =>
      <li key={file.filename + file.sha256}>{file.filename} · {file.size.toLocaleString()} 字节 <small>SHA-256 {file.sha256}</small></li>)}</ul></>}
    {record.outcome && record.state !== 'smtp_accepted' && <ErrorNotice error={record.outcome} />}
    {record.state === 'smtp_accepted' && <p className="muted">SMTP 服务器已接受提交。送达与回复状态以邮件服务为准。</p>}
  </section>
}
