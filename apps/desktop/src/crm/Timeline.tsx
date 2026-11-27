import { Select } from '../ui/Select'
import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FileText, History, MessageSquare, Package, Phone, Plus, Reply, Search, Send, UsersRound, type LucideIcon } from 'lucide-react'
import { activityTypes, dateInput, dateLabel, editableActivityTypes, fromDateInput, request, sourceLabel,
  type Activity, type ActivityInput, type ActivityType, type Contact, type ContactQuery } from './api'
import { Empty, Field, Pagination, Problem } from './components'
import { CustomerIcon } from './CustomerIcon'
import { ToolPopover, ToolPopoverClose } from '../ui/WorkbenchUI'

const activityIcons = {
  note: FileText, call: Phone, meeting: UsersRound, chat: MessageSquare, sample: Package,
  other: History, email_sent: Send, email_received: Reply,
}
type EditActivity = { contactId: string; activity?: Activity }

export function Timeline({ contact, openContact, startNew = false, initialActivity }: {
  contact?: Contact; openContact?: (id: string) => void; startNew?: boolean; initialActivity?: string
}) {
  const [eventType, setEventType] = useState('')
  const [source, setSource] = useState('')
  const [offset, setOffset] = useState(0)
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState('')
  const [focused, setFocused] = useState(initialActivity)
  const [editor, setEditor] = useState<EditActivity | null>(startNew && contact?.archived_at === null ? { contactId: contact.id } : null)
  const contactId = contact?.id ?? null
  const keyword = useDeferredValue(search.trim())
  const query = useQuery({
    queryKey: ['activities', contactId, eventType, offset, keyword, source, focused],
    queryFn: () => request(focused ? { action: 'get_activity', id: focused }
      : { action: 'list_activities', contact_id: contactId, event_type: eventType, offset, keyword,
      source: source || undefined }, 'activities'),
  })
  const rows = query.data?.rows ?? []
  const current = rows.find(row => row.id === selected) ?? rows[0]
  return <div className={contact ? 'followup-scoped' : 'followup-workspace'}>
    {!contact && <header className="page-heading">
      <div><h1>跟进记录</h1><p>客户邮件、沟通与样品记录</p></div>
      {!editor && <FollowupCustomerPicker select={contactId => setEditor({ contactId })} />}
    </header>}
    <div className="followup-layout">
    <section className="panel followup-panel">
    <div className="section-heading">
      <h2>沟通时间线</h2>
      <div className="row-actions">
        {focused && <button type="button" className="button" onClick={() => setFocused(undefined)}><History aria-hidden="true" />全部跟进记录</button>}
        {!contact && !focused && <Select aria-label="筛选记录来源" value={source} onChange={event => { setSource(event.target.value); setOffset(0) }}>
          <option value="">所有来源</option><option value="manual">手动</option><option value="agent">Agent</option><option value="system">系统</option>
        </Select>}
        {!focused && <Select aria-label="筛选沟通方式" value={eventType} onChange={e => { setEventType(e.target.value); setOffset(0) }}>
          <option value="">全部方式</option>
          {Object.entries(activityTypes).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
        </Select>}
        {contact && contact.archived_at === null && editor === null && <button className="button primary" onClick={() => setEditor({ contactId: contact.id })}>
          <CustomerIcon name="plus" />新增跟进
        </button>}
      </div>
    </div>
    {!contact && !focused && <label className="followup-search"><Search aria-hidden="true" />
      <input type="search" aria-label="搜索跟进" placeholder="搜索客户、摘要或沟通内容" maxLength={160}
        value={search} onChange={event => { setSearch(event.target.value); setOffset(0) }} />
    </label>}
    {editor !== null && (
      <ActivityEditor key={editor.activity?.id ?? editor.contactId} activity={editor.activity}
        contactId={editor.contactId}
        close={() => setEditor(null)} />
    )}
    {query.isPending && <Empty>正在读取跟进…</Empty>}
    {query.isError && <Problem error={query.error} />}
    {query.data && <>
      {query.data.rows.length === 0 ? <Empty>{contact ? '尚无符合条件的跟进。' : '尚无符合条件的跟进。可在客户详情添加记录。'}</Empty>
        : <ol className="timeline followup-list">{query.data.rows.map(event => {
          const Icon = activityIcons[event.event_type] ?? History
          return <li key={event.id} data-selected={!contact && current?.id === event.id}>
            <time>{dateLabel(event.occurred_at)}</time>
            <span className="followup-icon" data-type={event.event_type}><Icon aria-hidden="true" /></span>
            <div className="followup-event">
              <div className="section-heading">
              <div>{contact ? <strong>{event.summary}</strong> : <button type="button" className="text-button" aria-pressed={current?.id === event.id}
                onClick={() => setSelected(event.id)}>{event.summary}</button>}
                <small>{activityTypes[event.event_type]} · {sourceLabel(event.source)}
                  {!contact && ' · ' + event.contact_name}{event.channel && ' · ' + event.channel}{event.followup_category && ' · ' + event.followup_category}</small></div>
              <div className="row-actions">
                {editor === null && event.source !== 'system' && (!contact || contact.archived_at === null) &&
                  <button className="text-button" onClick={() => setEditor({ contactId: event.contact_id, activity: event })}>
                    <CustomerIcon name="edit" />编辑跟进
                  </button>}
              </div>
              </div>
              {event.details && <p className="long-text">{event.details}</p>}
              {event.sample && <small>样品{event.sample.model && ` · ${event.sample.model}`}{event.sample.quantity !== null && ` · ${event.sample.quantity} 件`}
                {event.sample.batch_ref && ` · 批次 ${event.sample.batch_ref}`}</small>}
              {contact && event.customer_feedback && <p className="long-text">客户反馈 {event.customer_feedback}</p>}
            </div>
          </li>
        })}</ol>}
      <Pagination offset={offset} more={query.data.has_more} change={setOffset} />
    </>}
    </section>
    {!contact && current && <aside className="panel followup-detail" aria-label="沟通概要">
      <h2>沟通概要</h2>
      <div className="followup-customer"><span className="followup-icon"><UsersRound aria-hidden="true" /></span>
        <div><strong>{current.contact_name}</strong><small>{activityTypes[current.event_type]} · {sourceLabel(current.source)}</small></div>
      </div>
      <dl className="review-properties">
        <div><dt>摘要</dt><dd>{current.summary}</dd></div>
        <div><dt>发生时间</dt><dd>{dateLabel(current.occurred_at)}</dd></div>
        {current.channel && <div><dt>沟通渠道</dt><dd>{current.channel}</dd></div>}
        {current.followup_category && <div><dt>跟进类型</dt><dd>{current.followup_category}</dd></div>}
        {current.sample?.model && <div><dt>样品型号</dt><dd>{current.sample.model}</dd></div>}
        {current.sample?.quantity != null && <div><dt>样品数量</dt><dd>{current.sample.quantity} 件</dd></div>}
        {current.sample?.batch_ref && <div><dt>样品批次</dt><dd>{current.sample.batch_ref}</dd></div>}
      </dl>
      {current.details && <p className="followup-note">{current.details}</p>}
      {current.customer_feedback && <><h3>客户反馈</h3><p className="followup-note">{current.customer_feedback}</p></>}
      {openContact && <button className="button" onClick={() => openContact(current.contact_id)}><UsersRound aria-hidden="true" />打开客户详情</button>}
    </aside>}
    </div>
  </div>
}

export function FollowupCustomerPicker({ select, label = '新增跟进记录', title = '选择跟进客户', icon: Icon = Plus, disabled, primary = true }: {
  select: (id: string) => void; label?: string; title?: string; icon?: LucideIcon; disabled?: boolean; primary?: boolean
}) {
  const [text, setText] = useState('')
  const keyword = useDeferredValue(text.trim())
  const criteria: ContactQuery = {
    keyword, quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '',
    archive: 'active', sort: 'updated', offset: 0,
  }
  const customers = useQuery({
    queryKey: ['contacts', criteria], enabled: Boolean(keyword),
    queryFn: () => request({ action: 'list_contacts', query: criteria }, 'contacts'),
  })
  return <ToolPopover label={title} trigger={<button className={primary ? 'button primary' : 'button'} type="button" disabled={disabled}><Icon aria-hidden="true" />{label}</button>}>
    <h2 className="ftw-popup-heading">{title}</h2>
    <label className="followup-search"><Search aria-hidden="true" />
      <input type="search" aria-label="查找客户" placeholder="输入姓名、公司或邮箱" value={text} onChange={event => setText(event.target.value)} />
    </label>
    {keyword && customers.isPending && <p className="muted">正在查找客户…</p>}
    {customers.isError && <Problem error={customers.error} />}
    {customers.data?.rows.map(row => <ToolPopoverClose key={row.id} asChild><button type="button" className="followup-picker-item" onClick={() => select(row.id)}>
      <strong>{row.display_name}</strong><small>{row.company_name}</small>
    </button></ToolPopoverClose>)}
    {customers.data && !customers.data.rows.length && <p className="muted">没有符合条件的客户。</p>}
    {customers.data?.has_more && <p className="muted">继续输入以缩小搜索范围。</p>}
  </ToolPopover>
}

export function ActivityEditor({ activity, contactId, close, initialType, initialSample }: {
  activity?: Activity; contactId: string; close: () => void; initialType?: 'sample'; initialSample?: ActivityInput['sample']
}) {
  const [revision] = useState(activity?.revision)
  const [input, setInput] = useState<ActivityInput>({
    contact_id: contactId, event_type: activity?.event_type ?? initialType ?? 'note',
    occurred_at: activity?.occurred_at ?? Math.floor(Date.now() / 1000),
    summary: activity?.summary ?? '', details: activity?.details ?? '',
    channel: activity?.channel ?? '', followup_category: activity?.followup_category ?? '',
    customer_feedback: activity?.customer_feedback ?? '',
    counts_as_contact: activity?.counts_as_contact ?? null,
    sample: activity?.sample ?? initialSample,
  })
  const sample = input.sample ?? { model: '', quantity: null }
  const catalog = useQuery({ queryKey: ['crm-states'], staleTime: Infinity, enabled: input.event_type === 'sample',
    queryFn: () => request({ action: 'state_catalog' }, 'state_catalog') })
  const sampleStates = catalog.data?.options.filter(option => option.field === 'sample_status') ?? []
  const cache = useQueryClient()
  const mutation = useMutation({
    mutationFn: () => {
      const details = { ...input, sample: input.event_type === 'sample' ? sample : undefined }
      return activity && revision !== undefined
        ? request({ action: 'update_activity', id: activity.id, expected_revision: revision, input: details }, 'saved')
        : request({ action: 'save_activity', id: null, input: details }, 'saved')
    },
    onSuccess: async () => {
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['activities'] }), cache.invalidateQueries({ queryKey: ['contacts'] }),
        cache.invalidateQueries({ queryKey: ['contact', contactId] }), cache.invalidateQueries({ queryKey: ['sample-workspace'] }),
      ])
      close()
    },
  })
  return <form className="inline-form" onSubmit={event => { event.preventDefault(); mutation.mutate() }}>
    <fieldset disabled={mutation.isPending}>
      <div className="form-grid">
        <Field label="计入客户联系日期"><Select value={input.counts_as_contact == null ? '' : String(input.counts_as_contact)} onChange={e => setInput(current => ({ ...current, counts_as_contact: e.target.value === '' ? null : e.target.value === 'true' }))}>
          {activity?.counts_as_contact == null && <option value="">按跟进类型判断</option>}<option value="true">是</option><option value="false">否</option>
        </Select></Field>
        {!initialType && <Field label="沟通方式"><Select value={input.event_type} onChange={e => setInput(old => ({ ...old, event_type: e.target.value as ActivityType }))}>
          {editableActivityTypes.map(type => <option value={type} key={type}>{activityTypes[type]}</option>)}
        </Select></Field>}
        <Field label="发生时间"><input required type="datetime-local" value={dateInput(input.occurred_at || null)}
          onChange={e => setInput(old => ({ ...old, occurred_at: fromDateInput(e.target.value) ?? 0 }))} /></Field>
        <Field label="摘要（必填）"><input required autoFocus value={input.summary}
          onChange={e => setInput(old => ({ ...old, summary: e.target.value }))} /></Field>
        <Field label="沟通渠道"><input placeholder="阿里国际站、邮件等" value={input.channel} onChange={e => setInput(old => ({ ...old, channel: e.target.value }))} /></Field>
        <Field label="跟进类型"><input placeholder="样品物流、认证或需求" value={input.followup_category} onChange={e => setInput(old => ({ ...old, followup_category: e.target.value }))} /></Field>
        {input.event_type === 'sample' && <>
          <Field label="样品型号"><input name="sample-model" maxLength={160} value={sample.model}
            onChange={event => setInput(old => ({ ...old, sample: { ...sample, model: event.target.value } }))} /></Field>
          <Field label="样品数量"><input name="sample-quantity" type="number" min={1} max={1000000} step={1} value={sample.quantity ?? ''}
            onChange={event => setInput(old => ({ ...old, sample: { ...sample, quantity: event.target.value ? event.target.valueAsNumber : null } }))} /></Field>
          <Field label="批次编号"><input name="sample-batch" maxLength={100} placeholder="相同批次沿用同一编号" value={sample.batch_ref ?? ''}
            onChange={event => setInput(old => ({ ...old, sample: { ...sample, batch_ref: event.target.value } }))} /></Field>
          <Field label="本次样品进度"><Select value={sample.progress ?? ''}
            onChange={event => setInput(old => ({ ...old, sample: { ...sample, progress: event.target.value } }))}>
            <option value="">未记录进度</option>{sampleStates.map(option => <option key={option.code} value={option.code}>{option.label}</option>)}
          </Select></Field>
        </>}
        <Field label="客户反馈或需求"><textarea name="customer-feedback" rows={3} value={input.customer_feedback} onChange={e => setInput(old => ({ ...old, customer_feedback: e.target.value }))} /></Field>
        <Field label="详情"><textarea name="details" rows={3} value={input.details}
          onChange={e => setInput(old => ({ ...old, details: e.target.value }))} /></Field>
      </div>
      {mutation.isError && <Problem error={mutation.error} />}
      <div className="form-footer">
        <button className="button" type="button" onClick={close}>取消</button>
        <button className="button primary" type="submit" disabled={mutation.isPending}><CustomerIcon name="check" />{mutation.isPending ? '正在保存…' : '保存跟进'}</button>
      </div>
    </fieldset>
  </form>
}
