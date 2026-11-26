import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowRight, Ban, Bot, Check, CircleCheck, ClipboardCheck, FileUp, LockKeyhole, Mail, ShieldCheck } from 'lucide-react'
import { Field } from '../crm/components'
import { chooseMailAssets, mailCall, mailError, mailPurposes as purposes, type Account, type AgentMailPolicy, type MailCommand, type RegisteredAgent } from './api'

type SettingsCommand = Extract<MailCommand, {
  action: 'set_agent_mail_policy' | 'revoke_mail_agent' | 'set_asset_access' | 'set_agent_paused'
}>

export function AgentMail({ manageSuppression, showRecords }: { manageSuppression: () => void; showRecords: () => void }) {
  const [policyNotice, setPolicyNotice] = useState('')
  const cache = useQueryClient()
  const settings = useQuery({
    queryKey: ['agent-mail-settings'],
    refetchInterval: 30_000,
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
      if (command.action === 'set_agent_mail_policy') setPolicyNotice('发送权限已保存并生效。')
      if (command.action === 'set_agent_paused') setPolicyNotice(command.paused ? 'Agent 已暂停。' : 'Agent 已恢复。')
      await Promise.all([cache.invalidateQueries({ queryKey: ['agent-mail-settings'] }),
        cache.invalidateQueries({ queryKey: ['mail-assets'] })])
    },
  })
  const current = settings.data

  return <div className="agent-settings">
    {(settings.isError || change.isError || accounts.isError) &&
      <p className="alert error" role="alert">{mailError(settings.error ?? change.error ?? accounts.error)}</p>}
    {policyNotice && <p className="agent-policy-notice" role="status"><CircleCheck aria-hidden="true" />{policyNotice}</p>}
    <div className="agent-settings-layout">
      {current && accounts.data ? <PolicyEditor key={current.policy.revision} policy={current.policy} accounts={accounts.data.rows}
        agents={current.agents.filter(agent => !agent.revoked)}
        available={current.automatic_available} busy={change.isPending}
        save={(policy, agent_ids) => change.mutate({ action: 'set_agent_mail_policy', policy, agent_ids })} />
        : settings.isPending || accounts.isPending ? <section className="panel">正在读取发信权限…</section> : null}
      <aside className="agent-rules" aria-label="发信规则与记录">
        <section className="panel">
          <h2><ShieldCheck aria-hidden="true" />Core 发信规则</h2>
          <ul>{[
            '逐封核对授权账户、用途、收件人和内容快照',
            '首次开发按邮箱去重，限制任务用量和实际发送频率',
            '拒发规则在入队和发送前生效',
            '提交结果未知时保留记录，等待人工核对',
            '队列、SMTP 结果与 Agent 来源写入审计',
          ].map(rule => <li key={rule}><CircleCheck aria-hidden="true" />{rule}</li>)}</ul>
          <button className="button" onClick={manageSuppression}><Ban aria-hidden="true" />查看拒发名单</button>
        </section>
        <section className="panel">
          <h2><LockKeyhole aria-hidden="true" />工作台授权</h2>
          <p className="muted">由工作台用户登记身份并保存发送权限。Agent 按保存时的账户、用途、上限和速率执行。</p>
          <button className="text-button" onClick={showRecords}>查看发信记录<ArrowRight aria-hidden="true" /></button>
        </section>
      </aside>
    </div>
    <div className="agent-administration">
      <section className="panel">
        <h2><Bot aria-hidden="true" />本机 Agent 接入</h2>
        <p className="muted">当前 Windows 用户运行的本地 MCP 客户端直接使用“本机 Agent”身份，无需登记或令牌。自动发信仍需在上方授权该身份、发件邮箱和发送范围。</p>
        <p className="muted">如果此前只授权了 WorkBuddy，请在上方勾选“本机 Agent”并保存发送权限。</p>
        {current?.agents.length ? <ul className="record-list agent-list">{current.agents.map(agent => <li key={agent.id}>
          <strong>{agent.name}</strong>
          <span className="customer-status">{agent.revoked ? '已撤销' : agent.paused ? '已暂停' : current.policy.mode === 'automatic' && agent.automatic ? '可自动发信' : '已登记'}</span>
          <small>{agent.last_seen_at ? `最近连接 ${new Date(agent.last_seen_at * 1000).toLocaleString()}` : '尚未连接'}</small>
          {!agent.revoked && <button className="text-button" disabled={change.isPending}
            onClick={() => change.mutate({ action: 'set_agent_paused', id: agent.id, paused: !agent.paused })}>
            {agent.paused ? '恢复接入' : '暂停接入'}
          </button>}
          {!agent.revoked && agent.id !== 'local-mcp' && <button className="text-button" disabled={change.isPending}
            onClick={() => change.mutate({ action: 'revoke_mail_agent', id: agent.id })}>撤销身份</button>}
        </li>)}</ul> : <p className="muted">尚未登记 Agent。</p>}
      </section>
    <section className="panel">
      <div className="section-heading"><h2>Agent 可用附件</h2><button className="button" disabled={importFiles.isPending}
        onClick={() => importFiles.mutate()}><FileUp aria-hidden="true" />导入 PDF 或图片</button></div>
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
  </div>
}

function PolicyEditor({ policy, accounts, agents, available, busy, save }: {
  policy: AgentMailPolicy; accounts: Account[]; agents: RegisteredAgent[]; available: boolean; busy: boolean; save: (policy: AgentMailPolicy, ids: string[]) => void
}) {
  const firstSetup = policy.revision === 1 && policy.mode === 'manual' && available
  const [value, setValue] = useState(policy)
  const [ids, setIds] = useState<string[] | null>(() => firstSetup ? null : agents.filter(agent => agent.automatic).map(agent => agent.id))
  const [accountIds, setAccountIds] = useState<string[] | null>(() => firstSetup ? null : policy.accounts)
  const selected = (ids ?? agents.map(agent => agent.id)).filter(id => agents.some(agent => agent.id === id))
  const selectedAccounts = (accountIds ?? accounts.map(account => account.id)).filter(id => accounts.some(account => account.id === id))
  const incomplete = value.mode === 'automatic' && (!available || !selected.length || !selectedAccounts.length || !value.purposes.length)
  function selectMode(mode: AgentMailPolicy['mode']) {
    setValue(previous => {
      if (mode === 'automatic' && firstSetup && !previous.purposes.length) {
        return { ...previous, mode, purposes: Object.keys(purposes), task_limit: 20, daily_limit: 30, interval_seconds: 60 }
      }
      return { ...previous, mode }
    })
  }
  function togglePurpose(entry: string) {
    setValue(previous => ({ ...previous, purposes: previous.purposes.includes(entry)
      ? previous.purposes.filter(item => item !== entry) : [...previous.purposes, entry] }))
  }
  return <form className="panel agent-policy-form" onSubmit={event => { event.preventDefault(); if (!incomplete) save({ ...value, accounts: selectedAccounts }, selected) }}>
    <h2><Mail aria-hidden="true" />发送模式</h2>
    <p className="muted">当前生效：{policy.mode === 'automatic' ? '持续授权自动发送' : policy.mode === 'manual' ? '人工逐封审批' : '禁止 Agent 发送'}。点击保存后，新设置立即生效。</p>
    <fieldset disabled={busy}>
      <div className="agent-mode-options" role="group" aria-label="Agent 发信模式">
        {([
          ['automatic', '持续授权自动发送', '授权范围内可直接提交', ShieldCheck],
          ['manual', '人工逐封审批', '经工作台批准后提交', ClipboardCheck],
          ['disabled', '禁止 Agent 发送', '保留 Agent 起草能力', Ban],
        ] as const).map(([mode, label, hint, Icon]) => <label key={mode} className="agent-mode-option" data-selected={value.mode === mode}>
          <Icon aria-hidden="true" /><input type="radio" name="mail-mode" value={mode} checked={value.mode === mode}
            disabled={mode === 'automatic' && !available} onChange={() => selectMode(mode)} />
          <strong>{label}</strong><small>{hint}</small>
        </label>)}
      </div>
      {value.mode === 'automatic' && <>
        <h3>允许自动发信的 Agent</h3>
        <div className="agent-policy-pills">{agents.map(agent => <label key={agent.id}>
          <input type="checkbox" checked={selected.includes(agent.id)} onChange={event =>
            setIds(event.target.checked ? [...selected, agent.id] : selected.filter(id => id !== agent.id))} />{agent.name}
        </label>)}</div>
        {!agents.length && <p className="muted">请先登记要授权的 Agent。</p>}
        <h3>允许使用的发件邮箱</h3>
        <div className="agent-policy-pills">{accounts.map(account => <label key={account.id}>
          <input type="checkbox" checked={selectedAccounts.includes(account.id)} onChange={event =>
            setAccountIds(event.target.checked ? [...selectedAccounts, account.id] : selectedAccounts.filter(id => id !== account.id))} />{account.email}
        </label>)}</div>
        {!accounts.length && <p className="muted">请先添加发件账户。</p>}
        <h3>允许的邮件用途</h3>
        <div className="agent-policy-pills">{Object.entries(purposes).map(([code,label]) => <label key={code}>
          <input type="checkbox" checked={value.purposes.includes(code)} onChange={() => togglePurpose(code)} />{label}
        </label>)}</div>
        <div className="form-grid">{([
          ['task_limit','每任务上限',1,20], ['daily_limit','每邮箱近 24 小时上限',1,30], ['interval_seconds','最短发送间隔（秒）',60,3600],
        ] as const).map(([key,label,min,max]) => <Field key={key} label={label}><input type="number" min={min} max={max}
          value={value[key]} required onChange={event => setValue({ ...value, [key]: event.target.valueAsNumber })} /></Field>)}</div>
      </>}
      <div className="form-footer"><button className="button primary" disabled={incomplete || busy}>
        <Check aria-hidden="true" />{busy ? '正在保存…' : '保存发送权限'}
      </button></div>
    </fieldset>
  </form>
}
