import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Field } from '../crm/components'
import { mailCall, mailError, type Account, type AgentMailPolicy, type MailCommand } from './api'

const purposes = {
  first_outreach: '首次开发信', outreach_followup: '开发跟进',
  quote_communication: '报价沟通', sample_communication: '样品沟通', other_business: '其他业务邮件',
}

type ReviewCommand = Extract<MailCommand, { action: 'set_agent_mail_policy' | 'register_mail_agent' | 'revoke_mail_agent' }>

export function AgentMail() {
  const [name, setName] = useState('')
  const [receipt, setReceipt] = useState('')
  const [issued, setIssued] = useState('')
  const [copied, setCopied] = useState(false)
  const cache = useQueryClient()
  const settings = useQuery({
    queryKey: ['agent-mail-settings'],
    queryFn: () => mailCall({ action: 'agent_mail_settings' }, 'agent_mail_settings'),
    refetchInterval: query => {
      const review = query.state.data?.review
      return review?.state === 'waiting' || (receipt && review?.id !== receipt) ? 750 : false
    },
  })
  const accounts = useQuery({ queryKey: ['mail-accounts'], queryFn: () => mailCall({ action: 'list_accounts' }, 'accounts') })
  const change = useMutation({
    mutationFn: (command: ReviewCommand) => mailCall(command, 'saved'),
    onSuccess: async (result, command) => {
      setReceipt(result.id)
      setIssued(command.action === 'register_mail_agent' ? command.token : '')
      setCopied(false)
      await cache.invalidateQueries({ queryKey: ['agent-mail-settings'] })
    },
  })
  const current = settings.data
  const review = current?.review
  const busy = change.isPending || review?.state === 'waiting'
  function register(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const token = Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2,'0')).join('')
    change.mutate({ action: 'register_mail_agent', name, token })
  }

  return <>
    {(settings.isError || change.isError || accounts.isError) &&
      <p className="alert error" role="alert">{mailError(settings.error ?? change.error ?? accounts.error)}</p>}
    {review && <p className={'alert ' + (review.state === 'failed' ? 'error' : 'warning')} role="status">
      {review.state === 'waiting' ? '请在 Windows 安全窗口核验当前账户。'
        : review.state === 'saved' ? '更改已保存。' : mailError(review.outcome ?? 'agent_policy_cancelled')}
    </p>}
    {current && <PolicyEditor key={current.policy.revision} policy={current.policy} accounts={accounts.data?.rows ?? []}
      available={current.automatic_available} busy={busy}
      save={policy => change.mutate({ action: 'set_agent_mail_policy', policy })} />}
    <section className="panel">
      <h2>已登记的 Agent</h2>
      <p className="muted">为 Codex、WorkBuddy 或其他 MCP 客户端分别登记。发送权限由工作台的策略统一管理。</p>
      {current?.agents.length ? <ul className="record-list">{current.agents.map(agent => <li key={agent.id}>
        <strong>{agent.name}</strong>
        {agent.revoked ? <span>已撤销</span> : <button className="button" disabled={busy}
          onClick={() => change.mutate({ action: 'revoke_mail_agent', id: agent.id })}>撤销身份</button>}
      </li>)}</ul> : <p className="muted">尚未登记 Agent。</p>}
      <form className="row-actions" onSubmit={register}>
        <Field label="Agent 名称"><input required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></Field>
        <button className="button" disabled={busy}>登记 Agent</button>
      </form>
      {issued && review?.id === receipt && review.state === 'saved' && <div className="inline-form">
        <Field label="MCP 进程环境变量 FTW_AGENT_TOKEN"><input value={issued} readOnly /></Field>
        <div className="row-actions"><button className="button" onClick={() => {
          void navigator.clipboard.writeText(issued).then(() => setCopied(true))
        }}>{copied ? '已复制' : '复制令牌'}</button>
        <button className="button" onClick={() => setIssued('')}>关闭</button></div>
      </div>}
    </section>
  </>
}

function PolicyEditor({ policy, accounts, available, busy, save }: {
  policy: AgentMailPolicy; accounts: Account[]; available: boolean; busy: boolean; save: (policy: AgentMailPolicy) => void
}) {
  const [value, setValue] = useState(policy)
  function toggle(key: 'accounts' | 'purposes', entry: string) {
    setValue(previous => ({ ...previous, [key]: previous[key].includes(entry)
      ? previous[key].filter(item => item !== entry) : [...previous[key], entry] }))
  }
  return <form className="panel" onSubmit={event => { event.preventDefault(); save(value) }}>
    <h2>发送模式</h2>
    <fieldset disabled={busy}>
      <Field label="Agent 发信模式"><select value={value.mode}
        onChange={event => setValue({ ...value, mode: event.target.value as AgentMailPolicy['mode'] })}>
        <option value="manual">人工逐封审批</option><option value="disabled">禁止 Agent 发送</option>
        <option value="automatic" disabled={!available}>持续授权自动发送</option>
      </select></Field>
      {value.mode === 'automatic' && <>
        <h3>允许使用的发件邮箱</h3>
        <div className="row-actions">{accounts.map(account => <label key={account.id}>
          <input type="checkbox" checked={value.accounts.includes(account.id)} onChange={() => toggle('accounts',account.id)} />{account.email}
        </label>)}</div>
        <h3>允许的邮件用途</h3>
        <div className="row-actions">{Object.entries(purposes).map(([code,label]) => <label key={code}>
          <input type="checkbox" checked={value.purposes.includes(code)} onChange={() => toggle('purposes',code)} />{label}
        </label>)}</div>
        <div className="form-grid">{([
          ['task_limit','每任务上限',1,20], ['daily_limit','每邮箱近 24 小时上限',1,30], ['interval_seconds','最短发送间隔（秒）',60,3600],
        ] as const).map(([key,label,min,max]) => <Field key={key} label={label}><input type="number" min={min} max={max}
          value={value[key]} required onChange={event => setValue({ ...value, [key]: event.target.valueAsNumber })} /></Field>)}</div>
      </>}
      <div className="form-footer"><button className="button primary">通过 Windows 核验并保存</button></div>
    </fieldset>
  </form>
}
