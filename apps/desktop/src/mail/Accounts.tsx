import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, CircleAlert, CircleCheck, Clock, Mail, Pencil, Plus, RefreshCw, Save, Server, ShieldCheck, Trash2, X } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { ConfirmButton, Empty, Field } from '../crm/components'
import { ImapAccountSettings, useImapAccounts, type ImapAccount } from './Inbox'
import { mailCall, mailError, type Account, type MailCommand } from './api'

type AccountFormData = Omit<Extract<MailCommand, { action: 'update_account' }>['input'], 'id'>
type AccountTab = 'smtp' | 'imap'

function accountError(error: unknown) {
  return <p className="alert error" role="alert">{mailError(error)}</p>
}

function imapStatus(account: ImapAccount | undefined) {
  if (!account?.host) return 'IMAP 待配置'
  if (account.outcome === 'ok') return '上次同步完成'
  if (account.outcome === 'running') return '同步中'
  if (account.outcome === 'more_available') return '上批已同步，可继续收取'
  return account.outcome ? mailError(account.outcome) : 'IMAP 已配置，等待同步'
}

export function Accounts({ initialTab }: { initialTab: AccountTab }) {
  const cache = useQueryClient()
  const accounts = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const imap = useImapAccounts()
  const [selected, setSelected] = useState('')
  const [tab, setTab] = useState(initialTab)
  const [adding, setAdding] = useState(false)
  const [notice, setNotice] = useState('')
  const [verified, setVerified] = useState<Record<string, boolean | undefined>>({})
  const rows = accounts.data?.rows ?? []
  const current = rows.find(row => row.id === selected) ?? rows.find(row => row.is_default) ?? rows[0]
  const currentImap = imap.data?.rows.find(row => row.account_id === current?.id)
  const change = useMutation({
    mutationFn: (command: MailCommand) => mailCall(command, command.action === 'test_account' ? 'connection_verified' : 'saved'),
    onMutate: () => setNotice(''),
    onError: (_, command) => {
      if (command.action === 'test_account') setVerified(old => ({ ...old, [command.id]: false }))
    },
    onSuccess: async (result, command) => {
      if (command.action === 'test_account') {
        setVerified(old => ({ ...old, [command.id]: true }))
        setNotice('SMTP 连接和认证通过。')
        return
      }
      if (command.action === 'update_account') {
        const id = command.input.id
        setVerified(old => ({ ...old, [id]: undefined }))
      }
      if (command.action === 'delete_account') setSelected('')
      await Promise.all(['mail-accounts', 'imap-accounts', 'mail-drafts', 'mail-draft', 'agent-mail-settings', 'mail-queue-summary']
        .map(key => cache.invalidateQueries({ queryKey: [key] })))
      if (command.action === 'save_account' && result.kind === 'saved') {
        setSelected(result.id)
        setTab('smtp')
        setAdding(false)
      }
      setNotice(command.action === 'delete_account' ? '账户已删除，历史邮件和草稿记录仍可查看。'
        : command.action === 'set_default_sender' ? '已设置新草稿的默认发件账户。'
        : command.action === 'set_account_preferences' ? '签名和账户上限已保存。' : '邮箱账户设置已保存。')
    },
  })
  function select(id: string) {
    setAdding(false)
    setSelected(id)
    setNotice('')
    change.reset()
  }
  return <div className="account-workspace">
    <div className="account-feedback">
      {notice && <p role="status" className="review-notice">{notice}</p>}
      {change.isError && accountError(change.error)}
    </div>
    <div className="account-workspace-layout">
      <section className="panel account-picker">
        <div className="section-heading"><h2><Mail aria-hidden="true" />邮箱账户</h2>
          <button className="button" disabled={change.isPending} onClick={() => { setAdding(true); setNotice(''); change.reset() }}><Plus aria-hidden="true" />新增账户</button></div>
        {accounts.isError && accountError(accounts.error)}
        {accounts.isPending && <Empty>正在读取邮箱账户…</Empty>}
        {accounts.data && rows.length === 0 && <Empty>添加支持 SMTP 登录的邮箱账户。</Empty>}
        {rows.map(row => {
          const receive = imap.data?.rows.find(item => item.account_id === row.id)
          return <button type="button" key={row.id} className="account-picker-row" aria-pressed={!adding && current?.id === row.id}
            onClick={() => select(row.id)} disabled={change.isPending}>
            <span className="account-symbol"><Mail aria-hidden="true" /></span>
            <span><span className="account-row-title"><strong>{row.name}</strong>{row.is_default && <span className="mail-status" data-status="linked">默认</span>}</span><small>{row.email}</small>
              <small>{imap.isPending ? '正在读取收件状态…' : imap.isError ? '收件状态暂不可用' : imapStatus(receive)}</small></span>
          </button>
        })}
      </section>
      {adding ? <section className="panel account-form-panel"><h2><Plus aria-hidden="true" />新增 SMTP 账户</h2>
        <AccountForm busy={change.isPending} cancel={() => setAdding(false)} save={input => {
          if (input.password) change.mutate({ action: 'save_account', ...input, password: input.password })
        }} />
      </section> : current ? <>
        <section className="panel account-form-panel">
          <h2><Server aria-hidden="true" />账户设置</h2><p className="account-address muted">{current.email}</p>
          <div className="account-settings-tabs" role="group" aria-label="选择邮件协议">
            <button className="button" aria-pressed={tab === 'smtp'} onClick={() => setTab('smtp')}>SMTP 发件</button>
            <button className="button" aria-pressed={tab === 'imap'} onClick={() => setTab('imap')}>IMAP 收件</button>
          </div>
          {tab === 'smtp' ? <>
            <p className="account-check" data-verified={verified[current.id]}>{verified[current.id] === true ? <CircleCheck aria-hidden="true" /> : verified[current.id] === false ? <CircleAlert aria-hidden="true" /> : <Clock aria-hidden="true" />}
              {verified[current.id] === true ? '本次 SMTP 测试通过' : verified[current.id] === false ? '本次 SMTP 测试失败' : 'SMTP 已配置，尚未测试'}</p>
            <AccountForm key={[current.id, current.name, current.email, current.host, current.port, current.security, current.username].join('|')}
              account={current} busy={change.isPending} test={() => change.mutate({ action: 'test_account', id: current.id })}
              save={input => change.mutate({ action: 'update_account', input: { ...input, id: current.id } })} />
            <details className="account-remove"><summary>账户管理</summary><ConfirmButton
              label="删除这个账户？邮箱将停止收发，历史邮件与草稿会保留。" disabled={change.isPending}
              onConfirm={() => change.mutate({ action: 'delete_account', id: current.id })}><Trash2 aria-hidden="true" />删除账户</ConfirmButton></details>
          </> : <>
            {imap.isError && accountError(imap.error)}
            {currentImap ? <><p className="account-check"><Clock aria-hidden="true" />{imapStatus(currentImap)}
              {currentImap.last_sync ? ` · ${dateLabel(currentImap.last_sync)}` : ''}</p>
              <ImapAccountSettings key={currentImap.account_id} account={currentImap} /></> : <Empty>正在读取收件设置…</Empty>}
          </>}
        </section>
        <SignatureSettings key={current.id} account={current} busy={change.isPending}
          makeDefault={() => change.mutate({ action: 'set_default_sender', id: current.id })}
          save={(daily_limit, signature) => change.mutateAsync({ action: 'set_account_preferences', id: current.id, daily_limit, signature })} />
      </> : accounts.data && <section className="panel account-form-panel"><Empty>从左侧新增账户，配置收发与签名。</Empty></section>}
    </div>
  </div>
}

function AccountForm({ account, busy, save, cancel, test }: {
  account?: Account; busy: boolean; save: (input: AccountFormData) => void; cancel?: () => void; test?: () => void
}) {
  const [security, setSecurity] = useState<'tls' | 'starttls'>(account?.security ?? 'tls')
  const [port, setPort] = useState(account?.port ?? 465)
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const fields = new FormData(form)
    const value = (name: string) => String(fields.get(name) ?? '')
    save({ name: value('name'), email: value('email'), host: value('host'), security, port,
      username: value('username'), password: value('password') || null })
    const password = form.elements.namedItem('password')
    if (password instanceof HTMLInputElement) password.value = ''
  }
  return <form className="account-settings-form" onSubmit={submit}>
    <fieldset disabled={busy}>
      <div className="account-form-fields">
        <Field label="账户名称"><input name="name" defaultValue={account?.name} required /></Field>
        <Field label="发件邮箱"><input name="email" type="email" defaultValue={account?.email} required /></Field>
        <Field label="SMTP 主机"><input name="host" defaultValue={account?.host} placeholder="smtp.example.com" required /></Field>
        <div className="account-form-pair">
          <Field label="加密方式"><select value={security} onChange={event => {
            const next = event.target.value as typeof security
            setSecurity(next)
            setPort(next === 'tls' ? 465 : 587)
          }}><option value="tls">TLS</option><option value="starttls">STARTTLS（强制）</option></select></Field>
          <Field label="端口"><input type="number" min="1" max="65535" value={port || ''} required onChange={event => setPort(event.target.valueAsNumber)} /></Field>
        </div>
        <Field label="SMTP 登录名"><input name="username" defaultValue={account?.username ?? account?.email} required /></Field>
        <Field label={account ? '新授权码或密码' : '应用授权码或密码'}>
          <input name="password" type="password" placeholder={account ? '留空保留原值' : undefined} autoComplete="new-password" required={!account} /></Field>
      </div>
      {account && <p className="muted">更换发件邮箱地址后，请重新配置 IMAP 和该邮箱的 Agent 授权。</p>}
      <div className="form-footer">
        {cancel && <button type="button" className="button" onClick={cancel}><X aria-hidden="true" />取消</button>}
        {test && <button type="button" className="button" onClick={test}><RefreshCw aria-hidden="true" />测试 SMTP</button>}
        <button type="submit" className="button primary"><Save aria-hidden="true" />{account ? '保存账户设置' : '添加账户'}</button>
      </div>
    </fieldset>
  </form>
}

function SignatureSettings({ account, busy, makeDefault, save }: {
  account: Account; busy: boolean; makeDefault: () => void; save: (limit: number, signature: string) => Promise<unknown>
}) {
  const [editing, setEditing] = useState(false)
  return <section className="panel account-signature">
    <h2><Pencil aria-hidden="true" />签名与发件偏好</h2>
    <dl className="review-properties">
      <div><dt>新建邮件</dt><dd>{account.is_default ? '默认发件账户' : <button className="button" disabled={busy} onClick={makeDefault}>设为默认</button>}</dd></div>
      <div><dt>近 24 小时上限</dt><dd>{account.daily_limit ?? 30} 封</dd></div>
    </dl>
    {editing ? <form className="account-signature-form" onSubmit={event => {
      event.preventDefault()
      const data = new FormData(event.currentTarget)
      void save(Number(data.get('limit')), String(data.get('signature') ?? '')).then(() => setEditing(false), () => {})
    }}>
      <Field label="纯文本签名"><textarea name="signature" rows={6} maxLength={2000} defaultValue={account.signature ?? ''} /></Field>
      <Field label="近 24 小时发送上限"><input name="limit" type="number" min="1" max="500" defaultValue={account.daily_limit ?? 30} required /></Field>
      <div className="form-footer"><button className="button" type="button" disabled={busy} onClick={() => setEditing(false)}>取消</button>
        <button className="button primary" disabled={busy} type="submit"><Check aria-hidden="true" />保存签名和上限</button></div>
    </form> : <>
      <div className="account-signature-preview"><h3>纯文本签名</h3>
        {account.signature ? <pre>{account.signature}</pre> : <p className="muted">尚未设置签名</p>}</div>
      <button className="button" disabled={busy} onClick={() => setEditing(true)}><Pencil aria-hidden="true" />编辑签名和上限</button>
    </>}
    <p className="account-signature-note"><ShieldCheck aria-hidden="true" />签名随此账户用于发送。Agent 权限继续在发信权限页管理。</p>
  </section>
}
