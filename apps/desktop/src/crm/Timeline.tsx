import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { activityTypes, dateInput, dateLabel, editableActivityTypes, fromDateInput, request, sourceLabel,
  type Activity, type ActivityInput, type ActivityType, type Contact } from './api'
import { Empty, Field, Pagination, Problem } from './components'

export function Timeline({ contact, openContact }: { contact?: Contact; openContact?: (id: string) => void }) {
  const [eventType, setEventType] = useState('')
  const [offset, setOffset] = useState(0)
  const [editor, setEditor] = useState<Activity | 'new' | null>(null)
  const contactId = contact?.id ?? null
  const query = useQuery({
    queryKey: ['activities', contactId, eventType, offset],
    queryFn: () => request({ action: 'list_activities', contact_id: contactId, event_type: eventType, offset }, 'activities'),
  })
  return <section className="panel">
    <div className="section-heading">
      <h2>{contact ? '沟通时间线' : '跟进列表'}</h2>
      <div className="row-actions">
        <select aria-label="筛选沟通方式" value={eventType} onChange={e => { setEventType(e.target.value); setOffset(0) }}>
          <option value="">全部方式</option>
          {Object.entries(activityTypes).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
        </select>
        {contact && contact.archived_at === null && editor === null && <button className="button primary" onClick={() => setEditor('new')}>新增跟进</button>}
      </div>
    </div>
    {editor !== null && (
      <ActivityEditor activity={editor === 'new' ? undefined : editor}
        contactId={editor === 'new' ? contact!.id : editor.contact_id}
        close={() => setEditor(null)} />
    )}
    {query.isPending && <Empty>正在读取跟进…</Empty>}
    {query.isError && <Problem error={query.error} />}
    {query.data && <>
      {query.data.rows.length === 0 ? <Empty>{contact ? '尚无符合条件的跟进。' : '尚无符合条件的跟进。可在客户详情添加记录。'}</Empty>
        : <ol className="timeline">{query.data.rows.map(event => (
          <li key={event.id}>
            <div className="section-heading">
              <div><strong>{event.summary}</strong>
                <small>{dateLabel(event.occurred_at)} · {activityTypes[event.event_type]} · {sourceLabel(event.source)}</small></div>
              <div className="row-actions">
                {openContact && <button className="text-button" onClick={() => openContact(event.contact_id)}>{event.contact_name}</button>}
                {editor === null && event.source !== 'system' && (!contact || contact.archived_at === null) &&
                  <button className="text-button" onClick={() => setEditor(event)}>编辑跟进</button>}
              </div>
            </div>
            {event.details && <p className="long-text">{event.details}</p>}
          </li>
        ))}</ol>}
      <Pagination offset={offset} more={query.data.has_more} change={setOffset} />
    </>}
  </section>
}

function ActivityEditor({ activity, contactId, close }: {
  activity?: Activity; contactId: string; close: () => void
}) {
  const [input, setInput] = useState<ActivityInput>({
    contact_id: contactId, event_type: activity?.event_type ?? 'note',
    occurred_at: activity?.occurred_at ?? Math.floor(Date.now() / 1000),
    summary: activity?.summary ?? '', details: activity?.details ?? '',
  })
  const cache = useQueryClient()
  const mutation = useMutation({
    mutationFn: () => request({ action: 'save_activity', id: activity?.id ?? null, input }, 'saved'),
    onSuccess: async () => {
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['activities'] }), cache.invalidateQueries({ queryKey: ['contacts'] }),
        cache.invalidateQueries({ queryKey: ['contact', contactId] }),
      ])
      close()
    },
  })
  return <form className="inline-form" onSubmit={event => { event.preventDefault(); mutation.mutate() }}>
    <fieldset disabled={mutation.isPending}>
      <div className="form-grid">
        <Field label="沟通方式"><select value={input.event_type} onChange={e => setInput(old => ({ ...old, event_type: e.target.value as ActivityType }))}>
          {editableActivityTypes.map(type => <option value={type} key={type}>{activityTypes[type]}</option>)}
        </select></Field>
        <Field label="发生时间"><input required type="datetime-local" value={dateInput(input.occurred_at || null)}
          onChange={e => setInput(old => ({ ...old, occurred_at: fromDateInput(e.target.value) ?? 0 }))} /></Field>
        <Field label="摘要（必填）"><input required autoFocus value={input.summary}
          onChange={e => setInput(old => ({ ...old, summary: e.target.value }))} /></Field>
        <Field label="详情"><textarea rows={3} value={input.details}
          onChange={e => setInput(old => ({ ...old, details: e.target.value }))} /></Field>
      </div>
      {mutation.isError && <Problem error={mutation.error} />}
      <div className="form-footer">
        <button className="button" type="button" onClick={close}>取消</button>
        <button className="button primary" type="submit">{mutation.isPending ? '正在保存…' : '保存跟进'}</button>
      </div>
    </fieldset>
  </form>
}
