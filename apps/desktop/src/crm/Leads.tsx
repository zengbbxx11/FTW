import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { leadStatuses, request, sourceLabel, type Duplicate, type Lead, type LeadInput } from './api'
import { ConfirmButton, Duplicates, Empty, Field, Pagination, Problem } from './components'

const blank: LeadInput = {
  display_name: '', company_name: '', country_region: '', email: '', website_url: '',
  business_summary: '', source_urls: [], research_notes: '', confidence: null,
}
const fields = [
  ['display_name', '候选客户名称（必填）', 'text'], ['company_name', '公司或店铺', 'text'],
  ['country_region', '国家或地区', 'text'], ['email', '邮箱', 'email'], ['website_url', '网站', 'url'],
] as const

export function LeadList({ open, create }: { open: (id: string) => void; create: () => void }) {
  const [status, setStatus] = useState('pending')
  const [offset, setOffset] = useState(0)
  const query = useQuery({
    queryKey: ['leads', status, offset],
    queryFn: () => request({ action: 'list_leads', offset, review_status: status }, 'leads'),
  })
  return <>
    <div className="page-heading"><h1>候选客户</h1><button className="button primary" onClick={create}>新增候选客户</button></div>
    <div className="filters">
      <Field label="审核状态"><select value={status} onChange={e => { setStatus(e.target.value); setOffset(0) }}>
        <option value="">全部状态</option>{Object.entries(leadStatuses).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
      </select></Field>
    </div>
    {query.isPending && <Empty>正在读取候选客户…</Empty>}
    {query.isError && <Problem error={query.error} />}
    {query.data && <>
      {query.data.rows.length === 0 ? <Empty>当前没有符合条件的候选客户。</Empty>
        : <div className="table-wrap"><table><thead><tr><th>候选客户</th><th>国家或地区</th><th>邮箱</th><th>审核状态</th><th>记录来源</th></tr></thead>
          <tbody>{query.data.rows.map(row => <tr key={row.id}>
            <td><button className="text-button" onClick={() => open(row.id)}>{row.display_name}</button>{row.company_name && <small>{row.company_name}</small>}</td>
            <td>{row.country_region || '—'}</td><td>{row.email || '—'}</td>
            <td>{leadStatuses[row.review_status]}</td><td>{sourceLabel(row.source)}</td>
          </tr>)}</tbody></table></div>}
      <Pagination offset={offset} more={query.data.has_more} change={setOffset} />
    </>}
  </>
}

export function LeadEditor({ record, saved, cancel }: { record?: Lead; saved: (id: string) => void; cancel: () => void }) {
  const [input, setInput] = useState(record?.input ?? blank)
  const [duplicate, setDuplicate] = useState<Duplicate[]>([])
  const cache = useQueryClient()
  const mutation = useMutation({
    mutationFn: (allow: boolean) => request({
      action: 'save_lead', id: record?.id ?? null, allow_duplicate: allow,
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
  return <form onSubmit={e => { e.preventDefault(); mutation.mutate(false) }}>
    <div className="page-heading"><h1>{record ? '编辑候选客户' : '新增候选客户'}</h1>
      <button className="button" type="button" onClick={cancel}>取消</button></div>
    <fieldset disabled={mutation.isPending}>
      <section className="panel">
        <div className="form-grid">
          {fields.map(([field, label, type]) => <Field label={label} key={field}><input type={type} value={input[field]}
            required={field === 'display_name'} autoFocus={field === 'display_name'}
            onChange={e => update(field, e.target.value)} /></Field>)}
          <Field label="置信度（0–100）"><input type="number" min="0" max="100" step="1" value={input.confidence ?? ''}
            onChange={e => update('confidence', e.target.value ? e.target.valueAsNumber : null)} /></Field>
          <Field label="业务摘要"><textarea rows={3} value={input.business_summary} onChange={e => update('business_summary', e.target.value)} /></Field>
          <Field label="调查备注"><textarea rows={3} value={input.research_notes} onChange={e => update('research_notes', e.target.value)} /></Field>
          <Field label="证据网址（每行一个）"><textarea rows={4} value={input.source_urls.join('\n')}
            onChange={e => update('source_urls', e.target.value.split('\n'))} /></Field>
        </div>
      </section>
      {mutation.isError && <Problem error={mutation.error} />}
      <Duplicates matches={duplicate}><button className="button" type="button" onClick={() => mutation.mutate(true)}>确认继续保存</button></Duplicates>
      <div className="form-footer"><button className="button primary" type="submit">{mutation.isPending ? '正在保存…' : '保存候选客户'}</button></div>
    </fieldset>
  </form>
}

export function LeadDetail({ id, back, openContact }: { id: string; back: () => void; openContact: (id: string) => void }) {
  const [editing, setEditing] = useState(false)
  const [duplicate, setDuplicate] = useState<Duplicate[]>([])
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['lead', id], queryFn: () => request({ action: 'get_lead', id }, 'lead') })
  const review = useMutation({
    mutationFn: (rejected: boolean) => request({ action: 'set_lead_review', id, rejected }, 'saved'),
    onSuccess: async () => {
      setDuplicate([])
      await Promise.all([cache.invalidateQueries({ queryKey: ['leads'] }), cache.invalidateQueries({ queryKey: ['lead', id] })])
    },
  })
  const promote = useMutation({
    mutationFn: (allow: boolean) => request({ action: 'promote_lead', id, allow_duplicate: allow }, 'saved', 'duplicates'),
    onSuccess: async result => {
      if (result.kind === 'duplicates') { setDuplicate(result.matches); return }
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['leads'] }), cache.invalidateQueries({ queryKey: ['lead', id] }),
        cache.invalidateQueries({ queryKey: ['contacts'] }),
      ])
      openContact(result.id)
    },
  })
  if (!query.data) return <><button className="button" onClick={back}>返回候选列表</button>
    {query.isError ? <Problem error={query.error} /> : <Empty>正在读取候选客户…</Empty>}</>
  const { record, duplicates } = query.data
  const { input } = record
  if (editing) return <LeadEditor record={record} cancel={() => setEditing(false)} saved={() => { setDuplicate([]); setEditing(false) }} />

  const pending = review.isPending || promote.isPending
  return <>
    <button className="text-button back-link" onClick={back}>返回候选列表</button>
    <div className="page-heading">
      <div><h1>{input.display_name}</h1><p>{leadStatuses[record.review_status]} · {sourceLabel(record.source)}</p></div>
      <div className="row-actions">
        {record.review_status !== 'promoted' && <button className="button" disabled={pending} onClick={() => setEditing(true)}>编辑候选</button>}
        {record.review_status === 'pending' && <>
          <ConfirmButton label="拒绝这条候选客户？记录仍会保留。" disabled={pending}
            onConfirm={() => review.mutate(true)}>拒绝</ConfirmButton>
          <ConfirmButton label="转为正式客户？调查备注和置信度留在候选记录中。" disabled={pending}
            onConfirm={() => promote.mutate(false)}>转为正式客户</ConfirmButton>
        </>}
        {record.review_status === 'rejected' && <button className="button" disabled={pending} onClick={() => review.mutate(false)}>重新审核</button>}
        {record.promoted_contact_id && <button className="button primary" onClick={() => openContact(record.promoted_contact_id!)}>打开正式客户</button>}
      </div>
    </div>
    {review.isError && <Problem error={review.error} />}
    {promote.isError && <Problem error={promote.error} />}
    <Duplicates matches={duplicate.length ? duplicate : duplicates}>
      {duplicate.length > 0 && <button className="button" disabled={pending} onClick={() => promote.mutate(true)}>确认继续转正</button>}
    </Duplicates>
    <section className="panel"><h2>候选资料</h2>
      <dl className="profile-grid">
        {fields.filter(([key]) => key !== 'display_name').map(([key, label]) => input[key] && <div key={key}><dt>{label}</dt><dd>{input[key]}</dd></div>)}
        {input.business_summary && <div><dt>业务摘要</dt><dd>{input.business_summary}</dd></div>}
        {input.research_notes && <div><dt>调查备注</dt><dd>{input.research_notes}</dd></div>}
        {input.confidence !== null && <div><dt>置信度</dt><dd>{input.confidence} / 100</dd></div>}
      </dl>
      {input.source_urls.length > 0 && <><h3>证据网址</h3><ul className="source-list">
        {input.source_urls.map((url, index) => <li key={index}>{url}</li>)}
      </ul></>}
    </section>
  </>
}
