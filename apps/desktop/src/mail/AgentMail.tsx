import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Field } from '../crm/components'
import { chooseMailAssets, mailCall, mailError, type Account, type AgentMailPolicy, type MailCommand, type RegisteredAgent } from './api'

const purposes = {
  first_outreach: '首次开发信', outreach_followup: '开发跟进',
  quote_communication: '报价沟通', sample_communication: '样品沟通', other_business: '其他业务邮件',
}

type SettingsCommand = Extract<MailCommand, {
  action: 'set_agent_mail_policy' | 'revoke_mail_agent' | 'set_asset_access'
}>

export function AgentMail() {
  const [name, setName] = useState('')
  const [registering, setRegistering] = useState(false)
  const [registrationError, setRegistrationError] = useState('')
  const [policyNotice, setPolicyNotice] = useState('')
  const [tokens, setTokens] = useState<Record<string, string>>({})
  const cache = useQueryClient()
  const settings = useQuery({
    queryKey: ['agent-mail-settings'],
    queryFn: () => mailCall({ action: 'agent_mail_settings' }, 'agent_mail_settings'),
  })
  const accounts = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const assets = useQuery({ queryKey: ['mail-assets'], queryFn: () => mailCall({ action: 'list_assets' }, 'assets') })
  const importFiles = useMutation({
    mutationFn: chooseMailAssets,
    onSuccess: () => cache.invalidateQueries({ queryKey: ['mail-assets'] }),
  })
  const change = useMutation({
    mutationFn: (command: SettingsCommand) => mailCall(command, 'saved'),
    onMutate: () => setPolicyNotice(''),
    onSuccess: async (_, command) => {
      if (command.action === 'revoke_mail_agent') dismissToken(command.id)
      if (command.action === 'set_agent_mail_policy') setPolicyNotice('发送权限已保存并生效。')
      await Promise.all([cache.invalidateQueries({ queryKey: ['agent-mail-settings'] }),
        cache.invalidateQueries({ queryKey: ['mail-assets'] })])
    },
  })
  const current = settings.data

  async function register(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!name.trim() || registering) return
    setRegistering(true)
    setRegistrationError('')
    try {
      const token = Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, '0')).join('')
      const result = await mailCall({ action: 'register_mail_agent', name: name.trim(), token }, 'saved')
      setTokens(previous => ({ ...previous, [result.id]: token }))
      setName('')
      await cache.invalidateQueries({ queryKey: ['agent-mail-settings'] })
    } catch (error) {
      setRegistrationError(mailError(error))
    } finally {
      setRegistering(false)
    }
  }
  function dismissToken(id: string) {
    setTokens(previous => {
      const remaining = { ...previous }
      delete remaining[id]
      return remaining
    })
  }

  return <div className="agent-settings">
    {(settings.isError || change.isError || accounts.isError) &&
      <p className="alert error" role="alert">{mailError(settings.error ?? change.error ?? accounts.error)}</p>}
    <div className="agent-settings-layout">
      <section className="panel">
        <h2>Agent 登记</h2>
        <p className="muted">Codex、WorkBuddy 和其他本地 MCP 客户端可分别登记，每个 Agent 使用自己的令牌。</p>
        <form className="agent-register" onSubmit={register}>
          <Field label="Agent 名称"><input required maxLength={80} placeholder="例如 WorkBuddy" value={name} onChange={event => setName(event.target.value)} /></Field>
          <button className="button primary" disabled={registering || !name.trim()}>{registering ? '登记中…' : '登记 Agent'}</button>
        </form>
        {registrationError && <p className="alert error" role="alert">{registrationError}</p>}
        {current?.agents.length ? <ul className="record-list agent-list">{current.agents.map(agent => <li key={agent.id}>
          <strong>{agent.name}</strong>
          <span className="customer-status">{agent.revoked ? '已撤销' : current.policy.mode === 'automatic' && agent.automatic ? '可自动发信' : '已登记'}</span>
          {!agent.revoked && <button className="text-button" disabled={change.isPending}
            onClick={() => change.mutate({ action: 'revoke_mail_agent', id: agent.id })}>撤销身份</button>}
          {tokens[agent.id] && !agent.revoked && <AgentToken token={tokens[agent.id]} dismiss={() => dismissToken(agent.id)} />}
        </li>)}</ul> : <p className="muted">尚未登记 Agent。</p>}
      </section>
      {current && accounts.data && <PolicyEditor key={current.policy.revision} policy={current.policy} accounts={accounts.data.rows}
        agents={current.agents.filter(agent => !agent.revoked)}
        available={current.automatic_available} busy={change.isPending} notice={policyNotice}
        save={(policy, agent_ids) => change.mutate({ action: 'set_agent_mail_policy', policy, agent_ids })} />}
    </div>
    <section className="panel">
      <div className="section-heading"><h2>Agent 可用附件</h2><button className="button" disabled={importFiles.isPending}
        onClick={() => importFiles.mutate()}>导入 PDF 或图片</button></div>
      {(assets.isError || importFiles.isError) && <p className="alert error" role="alert">{mailError(assets.error ?? importFiles.error)}</p>}
      {assets.data?.rows.length ? <ul className="record-list">{assets.data.rows.map(asset => <li key={asset.id}>
        <strong>{asset.filename}</strong><span>{Math.ceil(asset.size / 1024)} KiB</span>
        <button className="button" disabled={change.isPending}
          onClick={() => change.mutate({ action: 'set_asset_access', id: asset.id, allowed: !asset.agent_allowed })}>
          {asset.agent_allowed ? '撤销 Agent 使用权' : '授权 Agent 使用'}
        </button>
      </li>)}</ul> : <p className="muted">尚无本地附件。</p>}
    </section>
  </div>
}

function AgentToken({ token, dismiss }: { token: string; dismiss: () => void }) {
  const [notice, setNotice] = useState('')
  return <div className="inline-form w-full">
    <Field label="MCP 进程环境变量 FTW_AGENT_TOKEN"><input value={token} readOnly onFocus={event => event.currentTarget.select()} /></Field>
    <div className="row-actions"><button className="button" type="button" onClick={() => {
      void navigator.clipboard.writeText(token).then(() => setNotice('已复制'), () => setNotice('请选中令牌手动复制。'))
    }}>复制令牌</button>
    <button className="button" type="button" onClick={dismiss}>完成</button>
    {notice && <span role="status">{notice}</span>}</div>
  </div>
}

function PolicyEditor({ policy, accounts, agents, available, busy, notice, save }: {
  policy: AgentMailPolicy; accounts: Account[]; agents: RegisteredAgent[]; available: boolean; busy: boolean; notice: string; save: (policy: AgentMailPolicy, ids: string[]) => void
}) {
  const firstSetup = policy.revision === 1 && policy.mode === 'manual' && available
  const [value, setValue] = useState<AgentMailPolicy>(() => firstSetup ? {
    ...policy, mode: 'automatic', purposes: Object.keys(purposes), task_limit: 20, daily_limit: 30, interval_seconds: 60,
  } : policy)
  const [ids, setIds] = useState<string[] | null>(() => firstSetup ? null : agents.filter(agent => agent.automatic).map(agent => agent.id))
  const [accountIds, setAccountIds] = useState<string[] | null>(() => firstSetup ? null : policy.accounts)
  const selected = (ids ?? agents.map(agent => agent.id)).filter(id => agents.some(agent => agent.id === id))
  const selectedAccounts = (accountIds ?? accounts.map(account => account.id)).filter(id => accounts.some(account => account.id === id))
  const incomplete = value.mode === 'automatic' && (!selected.length || !selectedAccounts.length || !value.purposes.length)
  function togglePurpose(entry: string) {
    setValue(previous => ({ ...previous, purposes: previous.purposes.includes(entry)
      ? previous.purposes.filter(item => item !== entry) : [...previous.purposes, entry] }))
  }
  return <form className="panel" onSubmit={event => { event.preventDefault(); if (!incomplete) save({ ...value, accounts: selectedAccounts }, selected) }}>
    <h2>发送模式</h2>
    <p className="muted">当前生效：{policy.mode === 'automatic' ? '持续授权自动发送' : policy.mode === 'manual' ? '人工逐封审批' : '禁止 Agent 发送'}。点击保存后，新设置立即生效。</p>
    {notice && <p role="status">{notice}</p>}
    <fieldset disabled={busy}>
      <div className="agent-mode-options" role="group" aria-label="Agent 发信模式">
        {([
          ['automatic', '持续授权自动发送', '允许所选 Agent 在配置范围内发送'],
          ['manual', '人工逐封审批', '草稿经工作台审核后发送'],
          ['disabled', '禁止 Agent 发送', 'Agent 可起草，发送由人工操作'],
        ] as const).map(([mode, label, hint]) => <label key={mode} className="agent-mode-option" data-selected={value.mode === mode}>
          <input type="radio" name="mail-mode" value={mode} checked={value.mode === mode}
            disabled={mode === 'automatic' && !available} onChange={() => setValue({ ...value, mode })} />
          <strong>{label}</strong><small>{hint}</small>
        </label>)}
      </div>
      {value.mode === 'automatic' && <>
        <h3>允许自动发信的 Agent</h3>
        <div className="row-actions">{agents.map(agent => <label key={agent.id}>
          <input type="checkbox" checked={selected.includes(agent.id)} onChange={event =>
            setIds(event.target.checked ? [...selected, agent.id] : selected.filter(id => id !== agent.id))} />{agent.name}
        </label>)}</div>
        {!agents.length && <p className="muted">请先登记要授权的 Agent。</p>}
        <h3>允许使用的发件邮箱</h3>
        <div className="row-actions">{accounts.map(account => <label key={account.id}>
          <input type="checkbox" checked={selectedAccounts.includes(account.id)} onChange={event =>
            setAccountIds(event.target.checked ? [...selectedAccounts, account.id] : selectedAccounts.filter(id => id !== account.id))} />{account.email}
        </label>)}</div>
        <h3>允许的邮件用途</h3>
        <div className="row-actions">{Object.entries(purposes).map(([code,label]) => <label key={code}>
          <input type="checkbox" checked={value.purposes.includes(code)} onChange={() => togglePurpose(code)} />{label}
        </label>)}</div>
        <div className="form-grid">{([
          ['task_limit','每任务上限',1,20], ['daily_limit','每邮箱近 24 小时上限',1,30], ['interval_seconds','最短发送间隔（秒）',60,3600],
        ] as const).map(([key,label,min,max]) => <Field key={key} label={label}><input type="number" min={min} max={max}
          value={value[key]} required onChange={event => setValue({ ...value, [key]: event.target.valueAsNumber })} /></Field>)}</div>
      </>}
      <div className="form-footer"><button className="button primary" disabled={incomplete}>保存发送权限</button></div>
    </fieldset>
  </form>
}
