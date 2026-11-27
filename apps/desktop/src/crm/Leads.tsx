import { Select } from '../ui/Select'
import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight, Building2, CircleCheck, ClipboardList, Ellipsis, FileSearch, Link, Pencil, Plus, Search, UserRoundPlus, X } from 'lucide-react'
import { dateLabel, leadStatuses, request, sourceLabel, type Duplicate, type Lead, type LeadInput, type LeadStatus, type LeadVersion } from './api'
import { ConfirmButton, Duplicates, Empty, Field, Pagination, Problem } from './components'
import { ActionMenu, IconButton, MenuAction } from '../ui/WorkbenchUI'
import { LeadEvidencePanel } from './LeadEvidence'

const blank: LeadInput = {
  display_name: '', company_name: '', country_region: '', email: '', website_url: '',
  business_summary: '', source_urls: [], research_notes: '', confidence: null,
}
const fields = [
  ['display_name', '候选客户名称（必填）', 'text'], ['company_name', '公司或店铺', 'text'],
  ['country_region', '国家或地区', 'text'], ['email', '邮箱', 'email'], ['website_url', '网站', 'url'],
] as const

function versionOf(record: Lead): LeadVersion {
  return { revision: record.record_revision, epoch: record.data_epoch }
}

export function LeadList({ open, create, openContact }: { open: (id: string) => void; create: () => void; openContact: (id: string) => void }) {
  const [status, setStatus] = useState<LeadStatus | ''>('')
  const [input, setInput] = useState('')
  const keyword = useDeferredValue(input.trim())
  const [source, setSource] = useState('')
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<string>()
  const query = useQuery({
    queryKey: ['leads', 'workspace', status, keyword, source, offset],
    queryFn: () => request({ action: 'lead_workspace', offset, review_status: status, keyword, source }, 'lead_workspace'),
  })
  const rows = query.data?.rows ?? []
  const active = rows.find(row => row.id === selected)
  const reset = () => { setOffset(0); setSelected(undefined) }
  return <section className="lead-workspace">
    <header className="page-heading"><div><h1>候选客户</h1><p>核对线索、调查资料和可能重复，再转入客户管理。</p></div>
      <button className="button primary" onClick={create}><Plus aria-hidden="true" />新增候选客户</button></header>
    <div className="lead-tabs" role="group" aria-label="按审核状态查看">
      {([['', '全部候选'], ...Object.entries(leadStatuses)] as [LeadStatus | '', string][]).map(([key, label]) =>
        <button className="button" key={key} aria-pressed={status === key} onClick={() => { setStatus(key); reset() }}>{label}</button>)}
    </div>
    <div className="lead-toolbar">
      <label className="followup-search"><Search aria-hidden="true" /><input type="search" aria-label="搜索候选客户" value={input} maxLength={320}
        placeholder="搜索名称、公司、网站、邮箱或国家" onChange={event => { setInput(event.target.value); reset() }} /></label>
      <Select aria-label="候选客户来源" value={source} onChange={event => { setSource(event.target.value); reset() }}>
        <option value="">全部来源</option><option value="agent">Agent</option><option value="manual">人工</option>
      </Select>
    </div>
    {query.isPending && <Empty>正在读取候选客户…</Empty>}
    {query.isError && <Problem error={query.error} />}
    {query.data && <div className="lead-layout" data-detail={Boolean(selected)}>
      <div className="lead-list-panel">
        {rows.length === 0 ? <Empty>当前没有符合条件的候选客户。</Empty> : <div className="table-wrap lead-grid"><table aria-label="候选客户记录">
          <thead><tr><th>候选企业 / 联系人</th><th>国家或地区</th><th>业务摘要</th><th>调查资料</th><th>审核状态</th><th>来源</th><th>操作</th></tr></thead>
          <tbody>{rows.map(row => <tr key={row.id} data-current={active?.id === row.id}>
            <td><button type="button" className="lead-row-name" aria-pressed={active?.id === row.id} onClick={() => setSelected(row.id)}>
              <span className="lead-avatar" aria-hidden="true">{Array.from(row.display_name)[0]?.toUpperCase()}</span>
              <span><strong title={row.display_name}>{row.display_name}</strong><small title={row.website_url || row.email}>{row.website_url || row.email || row.company_name}</small></span>
            </button></td>
            <td>{row.country_region || '—'}</td><td><span className="lead-summary-cell" title={row.business_summary}>{row.business_summary || '—'}</span></td>
            <td>{row.evidence_count ? <span className="lead-evidence-count"><Link aria-hidden="true" />{row.evidence_count} 项证据</span> : <small>未记录</small>}</td>
            <td><LeadStatusTag status={row.review_status} /></td><td>{sourceLabel(row.source)}</td>
            <td><ActionMenu trigger={<IconButton label={`${row.display_name}的操作`}><Ellipsis aria-hidden="true" /></IconButton>}>
              <MenuAction onSelect={() => setSelected(row.id)}><ClipboardList aria-hidden="true" />候选资料</MenuAction>
              <MenuAction onSelect={() => open(row.id)}><FileSearch aria-hidden="true" />查看调查证据</MenuAction>
            </ActionMenu></td>
          </tr>)}</tbody></table></div>}
        <footer className="lead-pagination"><small>{rows.length ? `第 ${offset + 1}–${offset + rows.length} 条` : '本页无记录'}</small>
          <Pagination offset={offset} more={query.data.has_more} change={next => { setOffset(next); setSelected(undefined) }} /></footer>
      </div>
      {selected && <aside className="lead-drawer" aria-label="候选客户详情">
        <header><h2>候选客户详情</h2><IconButton label="关闭候选详情" onClick={() => setSelected(undefined)}><X aria-hidden="true" /></IconButton></header>
        <LeadDetail key={selected} id={selected} back={() => setSelected(undefined)} openContact={openContact} openEvidence={() => open(selected)} />
      </aside>}
    </div>}
  </section>
}

function LeadStatusTag({ status }: { status: LeadStatus }) {
  return <span className="lead-status" data-status={status}>{leadStatuses[status]}</span>
}

export function LeadEditor({ record, saved, cancel }: { record?: Lead; saved: (id: string) => void; cancel: () => void }) {
  const [input, setInput] = useState(record?.input ?? blank)
  const [expected, setExpected] = useState(record ? versionOf(record) : null)
  const [duplicate, setDuplicate] = useState<Duplicate[]>([])
  const cache = useQueryClient()
  const mutation = useMutation({
    mutationFn: (allow: boolean) => request({
      action: 'save_lead', id: record?.id ?? null, allow_duplicate: allow, expected,
      input: { ...input, source_urls: input.source_urls.filter(url => url.trim()) },
    }, 'saved', 'duplicates'),
    onSuccess: async result => {
      if (result.kind === 'duplicates') { setDuplicate(result.matches); return }
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['leads'] }), cache.invalidateQueries({ queryKey: ['lead'] }),
      ])
      saved(result.id)
    },
  })
  const update = <K extends keyof LeadInput>(key: K, value: LeadInput[K]) => {
    setDuplicate([])
    setInput(current => ({ ...current, [key]: value }))
  }
  const stale = !!record && (record.record_revision !== expected?.revision || record.data_epoch !== expected.epoch)
  const promoted = record?.review_status === 'promoted'
  return <form className="lead-editor" onSubmit={e => { e.preventDefault(); if (!stale && !promoted) mutation.mutate(false) }}>
    <div className="page-heading"><div><h1>{record ? '编辑候选客户' : '新增候选客户'}</h1><p>记录线索、资料来源和待核实事项。</p></div>
      <button className="button" type="button" onClick={cancel}><X aria-hidden="true" />取消</button></div>
    {stale && <div className="alert warning" role="status">候选资料已有新版本。当前输入已保留，可复制所需内容后载入最新版本。
      <button className="button" type="button" disabled={mutation.isPending} onClick={() => {
        setInput(record.input); setExpected(versionOf(record)); setDuplicate([]); mutation.reset()
      }}>载入最新版本</button></div>}
    {promoted && <p className="muted">此候选已转为正式客户，原候选资料可供核对。</p>}
    <fieldset disabled={mutation.isPending}>
      <section className="panel">
        <div className="form-grid">
          {fields.map(([field, label, type]) => <Field label={label} key={field}><input name={field} type={type} value={input[field]} readOnly={promoted}
            required={field === 'display_name'} autoFocus={field === 'display_name'}
            onChange={e => update(field, e.target.value)} /></Field>)}
          <Field label="置信度（0–100）"><input type="number" min="0" max="100" step="1" value={input.confidence ?? ''} readOnly={promoted}
            onChange={e => update('confidence', e.target.value ? e.target.valueAsNumber : null)} /></Field>
          <Field label="业务摘要"><textarea name="business_summary" rows={3} value={input.business_summary} readOnly={promoted} onChange={e => update('business_summary', e.target.value)} /></Field>
          <Field label="调查备注"><textarea name="research_notes" rows={3} value={input.research_notes} readOnly={promoted} onChange={e => update('research_notes', e.target.value)} /></Field>
          <Field label="证据网址（每行一个）"><textarea rows={4} value={input.source_urls.join('\n')} readOnly={promoted}
            onChange={e => update('source_urls', e.target.value.split('\n'))} /></Field>
        </div>
      </section>
      {mutation.isError && <Problem error={mutation.error} />}
      {!stale && !promoted && <Duplicates matches={duplicate}><button className="button" type="button" onClick={() => mutation.mutate(true)}>确认继续保存</button></Duplicates>}
      <div className="form-footer"><button className="button primary" type="submit" disabled={stale || promoted}><CircleCheck aria-hidden="true" />{mutation.isPending ? '正在保存…' : '保存候选客户'}</button></div>
    </fieldset>
  </form>
}

export function LeadDetail({ id, back, openContact, openEvidence }: { id: string; back: () => void; openContact: (id: string) => void; openEvidence?: () => void }) {
  const [editing, setEditing] = useState(false)
  const [duplicate, setDuplicate] = useState<{ matches: Duplicate[]; expected: LeadVersion }>()
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['lead', id], queryFn: () => request({ action: 'get_lead', id }, 'lead') })
  const review = useMutation({
    mutationFn: ({ rejected, expected }: { rejected: boolean; expected: LeadVersion }) => request({ action: 'set_lead_review', id, rejected, expected }, 'saved'),
    onSuccess: async () => {
      setDuplicate(undefined)
      await Promise.all([cache.invalidateQueries({ queryKey: ['leads'] }), cache.invalidateQueries({ queryKey: ['lead', id] })])
    },
  })
  const promote = useMutation({
    mutationFn: ({ allow, expected }: { allow: boolean; expected: LeadVersion }) => request({ action: 'promote_lead', id, allow_duplicate: allow, expected }, 'saved', 'duplicates'),
    onSuccess: async (result, { expected }) => {
      if (result.kind === 'duplicates') { setDuplicate({ matches: result.matches, expected }); return }
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['leads'] }), cache.invalidateQueries({ queryKey: ['lead', id] }),
        cache.invalidateQueries({ queryKey: ['contacts'] }),
      ])
      openContact(result.id)
    },
  })
  if (!query.data) return <>{!openEvidence && <button className="button" onClick={back}>返回候选列表</button>}
    {query.isError ? <Problem error={query.error} /> : <Empty>正在读取候选客户…</Empty>}</>
  const { record, duplicates } = query.data
  const { input } = record
  const expected = versionOf(record)
  const versionKey = `${expected.epoch}:${expected.revision}`
  const confirmedMatches = duplicate?.expected.revision === expected.revision && duplicate.expected.epoch === expected.epoch ? duplicate : undefined
  if (editing) return <LeadEditor record={record} cancel={() => setEditing(false)} saved={() => { setDuplicate(undefined); setEditing(false) }} />

  const pending = review.isPending || promote.isPending
  return <div className="lead-evidence-workspace" data-embedded={Boolean(openEvidence)}>
    {!openEvidence && <header className="page-heading"><div>
      <button className="text-button back-link" onClick={back}><ArrowLeft aria-hidden="true" />返回候选列表</button>
      <h1>客户尽调与证据</h1><p>核对来源和调查备注，再决定是否转入正式客户。</p></div>
      <div className="row-actions">
        {record.review_status !== 'promoted' && <button className="button" disabled={pending} onClick={() => setEditing(true)}><Pencil aria-hidden="true" />编辑候选</button>}
      </div>
    </header>}
    {review.isError && <Problem error={review.error} />}
    {promote.isError && <Problem error={promote.error} />}
    <div className="lead-evidence-layout">
      <section className="panel lead-profile">
        <div className="lead-identity"><span className="lead-avatar" aria-hidden="true"><Building2 /></span>
          <div><h2>{input.display_name}</h2><p>{[input.company_name === input.display_name ? '' : input.company_name, input.country_region, sourceLabel(record.source)].filter(Boolean).join(' · ')}</p></div>
          <LeadStatusTag status={record.review_status} /></div>
        <h3>候选资料</h3>
        <dl className="review-properties">
          {fields.filter(([key]) => key !== 'display_name').map(([key, label]) => input[key] && <div key={key}><dt>{label}</dt><dd>{input[key]}</dd></div>)}
          {record.created_at > 0 && <div><dt>发现时间</dt><dd>{dateLabel(record.created_at)}</dd></div>}
          {input.confidence !== null && <div><dt>记录置信度</dt><dd>{input.confidence} / 100</dd></div>}
        </dl>
        {input.business_summary && <div className="lead-research-note"><h3>业务摘要</h3><p>{input.business_summary}</p></div>}
        {openEvidence && <button className="button" onClick={openEvidence}><FileSearch aria-hidden="true" />查看调查证据<ArrowRight aria-hidden="true" /></button>}
      </section>
      <div className="lead-evidence-panels">
        <LeadEvidencePanel id={id} notes={input.research_notes} />
        <section className="panel lead-review-controls"><h3><ClipboardList aria-hidden="true" />审核与入库</h3>
          <p className="muted">核对联系人身份和联系方式。转正后可从候选记录追溯调查资料。</p>
          <Duplicates matches={confirmedMatches?.matches ?? duplicates}>
            {confirmedMatches && <button className="button" disabled={pending} onClick={() => promote.mutate({ allow: true, expected: confirmedMatches.expected })}>确认继续转正</button>}
          </Duplicates>
          {!duplicates.length && !confirmedMatches && record.review_status === 'pending' && <p className="lead-match-clear"><CircleCheck aria-hidden="true" />当前查重规则未发现匹配。</p>}
          <div className="row-actions">
            {record.review_status === 'pending' && <>
              <ConfirmButton key={`review:${versionKey}`} label="拒绝这条候选客户？记录仍会保留。" disabled={pending} onConfirm={() => review.mutate({ rejected: true, expected })}>拒绝</ConfirmButton>
              <ConfirmButton key={`promote:${versionKey}`} primary label="转为正式客户？调查备注和置信度留在候选记录中。" disabled={pending} onConfirm={() => promote.mutate({ allow: false, expected })}>
                <UserRoundPlus aria-hidden="true" />转为正式客户
              </ConfirmButton>
            </>}
            {record.review_status === 'rejected' && <button className="button" disabled={pending} onClick={() => review.mutate({ rejected: false, expected })}>重新审核</button>}
            {record.promoted_contact_id && <button className="button primary" onClick={() => openContact(record.promoted_contact_id!)}>打开正式客户</button>}
          </div>
        </section>
      </div>
    </div>
  </div>
}
