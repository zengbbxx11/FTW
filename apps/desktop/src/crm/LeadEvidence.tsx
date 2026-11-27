import { Select } from '../ui/Select'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy, Link, Plus, Save } from 'lucide-react'
import { IconButton } from '../ui/WorkbenchUI'
import { dateInput, dateLabel, fromDateInput, request, sourceLabel, type EvidenceStatus, type LeadEvidence, type LeadEvidenceInput } from './api'
import { Empty, Field, Pagination, Problem } from './components'

const statuses: Record<EvidenceStatus, string> = { pending: '待核实', verified: '已核实', rejected: '未采信' }
const tones = { pending: 'orange', verified: 'green', rejected: 'gray' } as const
const blank: LeadEvidenceInput = { subject: '', url: '', note: '', observed_at: null }

export function LeadEvidencePanel({ id, notes, contact, readOnly }: {
  id: string; notes?: string; contact?: boolean; readOnly?: boolean
}) {
  const [offset, setOffset] = useState(0)
  const [draft, setDraft] = useState<LeadEvidenceInput | null>(null)
  const [notice, setNotice] = useState('')
  const cache = useQueryClient()
  const query = useQuery({
    queryKey: [contact ? 'contact' : 'lead', id, 'evidence', offset],
    queryFn: () => request(contact ? { action: 'list_contact_evidence', contact_id: id, offset }
      : { action: 'list_lead_evidence', lead_id: id, offset }, 'lead_evidence'),
  })
  function refresh() {
    return Promise.all(['contact', 'lead', 'leads', 'audit'].map(key => cache.invalidateQueries({ queryKey: [key] })))
  }
  const add = useMutation({
    mutationFn: (input: LeadEvidenceInput) => request(contact ? { action: 'add_contact_evidence', contact_id: id, input }
      : { action: 'add_lead_evidence', lead_id: id, input }, 'saved'),
    onSuccess: () => { setDraft(null); setOffset(0); setNotice('已添加待核实证据。'); return refresh() },
  })
  const review = useMutation({
    mutationFn: ({ row, status, epoch }: { row: LeadEvidence; status: EvidenceStatus; epoch: string }) =>
      request({ action: 'review_lead_evidence', id: row.id, revision: row.revision, epoch, status }, 'saved'),
    onSuccess: () => { setNotice('已保存核验状态。'); return refresh() },
    onError: () => cache.invalidateQueries({ queryKey: [contact ? 'contact' : 'lead', id, 'evidence'] }),
  })
  function copy(url: string) {
    void navigator.clipboard.writeText(url).then(() => setNotice('已复制来源网址。'), () => setNotice('无法写入剪贴板。'))
  }
  return <section className="panel lead-sources">
    <div className="section-heading"><h3><Link aria-hidden="true" />调查证据</h3>
      {!draft && !readOnly && <button type="button" className="button" onClick={() => { add.reset(); setDraft(blank) }}><Plus aria-hidden="true" />添加证据</button>}
    </div>
    {query.data && <p className="muted">{query.data.total} 项证据 · {query.data.verified} 项已核实 · {query.data.pending} 项待核实</p>}
    {draft && !readOnly && <form className="lead-evidence-form" onSubmit={event => { event.preventDefault(); add.mutate(draft) }}>
      <Field label="证据主题"><input autoFocus required maxLength={300} value={draft.subject} onChange={event => setDraft({ ...draft, subject: event.target.value })} /></Field>
      <Field label="来源网址"><input type="url" maxLength={1000} value={draft.url} onChange={event => setDraft({ ...draft, url: event.target.value })} /></Field>
      <Field label="调查时间"><input type="datetime-local" value={dateInput(draft.observed_at)} onChange={event => setDraft({ ...draft, observed_at: fromDateInput(event.target.value) })} /></Field>
      <Field label="证据说明"><textarea required={!draft.url.trim()} rows={2} maxLength={2000} value={draft.note} onChange={event => setDraft({ ...draft, note: event.target.value })} /></Field>
      {add.isError && <Problem error={add.error} />}
      <div className="form-footer"><button type="button" className="button" onClick={() => setDraft(null)} disabled={add.isPending}>取消</button>
        <button type="submit" className="button primary" disabled={add.isPending}><Save aria-hidden="true" />保存证据</button></div>
    </form>}
    {query.isError && <Problem error={query.error} />}
    {review.isError && <Problem error={review.error} />}
    {query.isPending && <Empty>正在读取证据…</Empty>}
    {query.data && (query.data.rows.length ? <ul className="lead-evidence-rows">
      {query.data.rows.map(row => <li key={row.id}><div className="lead-evidence-item">
        <strong>{row.subject}</strong>
        <span className="muted">{sourceLabel(row.source)} · {row.observed_at != null ? '调查于 ' + dateLabel(row.observed_at) : '记录于 ' + dateLabel(row.created_at)}</span>
        {row.candidate && <small className="muted">继承自候选「{row.candidate}」</small>}
        {row.url && <div className="lead-evidence-link"><span title={row.url}>{row.url}</span><IconButton label={'复制来源网址 ' + row.subject} onClick={() => copy(row.url)}><Copy aria-hidden="true" /></IconButton></div>}
        {row.note && <p className="long-text">{row.note}</p>}
        {row.request_id && <small className="muted">MCP 请求 {row.request_id}</small>}
      </div><div className="lead-evidence-review">
        <Select className="ops-tag" data-tone={tones[row.status]} aria-label={'核验状态 ' + row.subject}
          value={row.status} disabled={readOnly || review.isPending} onChange={event => {
          if (query.data) review.mutate({ row, status: event.target.value as EvidenceStatus, epoch: query.data.epoch })
        }}>
          {Object.entries(statuses).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
        </Select>
        {row.reviewed_at && <small className="muted">{row.review_source === 'agent' ? 'Agent 核验于' : '人工核验于'} {dateLabel(row.reviewed_at)}</small>}
        {row.review_source === 'agent' && !readOnly && <button type="button" className="button" disabled={review.isPending}
          onClick={() => { if (query.data) review.mutate({ row, status: row.status, epoch: query.data.epoch }) }}>人工确认</button>}
      </div></li>)}
    </ul> : <Empty>尚未记录调查证据。</Empty>)}
    {query.data && (offset > 0 || query.data.has_more) && <Pagination offset={offset} more={query.data.has_more} change={setOffset} />}
    {notice && <small role="status">{notice}</small>}
    {notes && <div className="lead-research-note"><h3>调查备注</h3><p>{notes}</p></div>}
  </section>
}
