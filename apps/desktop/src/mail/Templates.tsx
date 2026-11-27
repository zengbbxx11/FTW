import { Select } from '../ui/Select'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Archive, Eye, FilePenLine, FileText, Plus, RotateCcw, Save, ShieldCheck, X } from 'lucide-react'
import { ConfirmButton, Empty, Field, Pagination } from '../crm/components'
import { FollowupCustomerPicker } from '../crm/Timeline'
import { mailCall, mailError, mailPurposes, type Purpose, type Template, type TemplateInput, type TemplatePreview } from './api'

const variables: Record<string, string> = {
  contact_name: '客户姓名', company_name: '公司名称', country_region: '国家或地区', product_interest: '客户的产品兴趣',
}

function TemplateError({ error }: { error: unknown }) {
  return <p className="alert error" role="alert">{mailError(error)}</p>
}

function useTemplates(purpose: Purpose | '', offset: number, archived = false) {
  return useQuery({ queryKey: ['mail-templates', purpose, offset, archived],
    queryFn: () => mailCall({ action: 'list_templates', purpose, offset, archived }, 'templates') })
}

export function Templates() {
  const [purpose, setPurpose] = useState<Purpose | ''>('')
  const [archived, setArchived] = useState(false)
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState<{ id: string; fallback?: Template }>()
  const [creating, setCreating] = useState(false)
  const [notice, setNotice] = useState('')
  const query = useTemplates(purpose, offset, archived)
  const detail = useQuery({ queryKey: ['mail-templates', 'detail', selection?.id], enabled: Boolean(selection),
    queryFn: () => mailCall({ action: 'get_template', id: selection!.id }, 'template') })
  const rows = query.data?.rows ?? []
  const current = selection ? [selection.fallback, rows.find(row => row.id === selection.id), detail.data?.record]
    .reduce<Template | undefined>((latest, row) => row && (!latest || row.revision >= latest.revision) ? row : latest, undefined) : rows[0]
  const cache = useQueryClient()
  function resetList() { setOffset(0); setSelection(undefined); setCreating(false); setNotice('') }
  const archive = useMutation({
    mutationFn: (template: Template) => mailCall({ action: 'set_template_archive', id: template.id,
      expected_revision: template.revision, archived: template.archived_at == null }, 'saved'),
    onSuccess: async (_, template) => {
      if (rows.length === 1 && offset) setOffset(previous => Math.max(0, previous - 20))
      setSelection(undefined)
      await cache.invalidateQueries({ queryKey: ['mail-templates'] })
      setNotice(template.archived_at == null ? '模板已归档。' : '模板已恢复。')
    },
  })
  return <div className="template-workspace">
    {archive.isError && <TemplateError error={archive.error} />}
    <div className="template-layout">
      <section className="panel template-list">
        <div className="section-heading"><h2>模板库</h2><button className="button primary" type="button" onClick={() => { setCreating(true); setNotice('') }}>
          <Plus aria-hidden="true" />新增模板</button></div>
        <Field label="邮件用途"><Select value={purpose} onChange={event => { setPurpose(event.target.value as Purpose | ''); resetList() }}>
          <option value="">全部用途</option>{Object.entries(mailPurposes).map(([key, name]) => <option key={key} value={key}>{name}</option>)}
        </Select></Field>
        <div className="account-settings-tabs" role="group" aria-label="模板状态">
          <button className="button" aria-pressed={!archived} onClick={() => { setArchived(false); resetList() }}>在用</button>
          <button className="button" aria-pressed={archived} onClick={() => { setArchived(true); resetList() }}>已归档</button>
        </div>
        {query.isPending && <Empty>正在读取模板…</Empty>}
        {query.isError && <TemplateError error={query.error} />}
        {query.data && !rows.length && <Empty>当前没有符合条件的模板。</Empty>}
        {rows.map(row => <button type="button" className="template-list-row" aria-pressed={!creating && row.id === current?.id} key={row.id}
          onClick={() => { setSelection({ id: row.id, fallback: row }); setCreating(false); setNotice(''); archive.reset() }}>
          <span className="account-symbol"><FileText aria-hidden="true" /></span><span><strong>{row.name}</strong><small>{mailPurposes[row.purpose]}</small></span>
        </button>)}
        {query.data && <Pagination offset={offset} more={query.data.has_more} change={next => { setOffset(next); setSelection(undefined); setCreating(false) }} />}
        {notice && <p className="review-notice" role="status">{notice}</p>}
      </section>
      {creating || current ? <TemplateEditor key={creating ? 'new' : current!.id} template={creating ? undefined : current}
        defaultPurpose={purpose || 'first_outreach'} busy={archive.isPending}
        pin={row => setSelection({ id: row.id, fallback: row })}
        cancel={() => setCreating(false)} archive={() => current && archive.mutate(current)} saved={async id => {
          setCreating(false)
          setPurpose('')
          setArchived(false)
          setOffset(0)
          setSelection({ id })
          await cache.invalidateQueries({ queryKey: ['mail-templates'] })
          await cache.invalidateQueries({ queryKey: ['mail-template-preview'] })
          setNotice('模板已保存。')
        }} /> : <><section className="panel"><Empty>从模板库选择一条记录，或新建模板。</Empty></section><TemplateGuide /></>}
    </div>
  </div>
}

function TemplateEditor({ template, defaultPurpose, busy, cancel, archive, saved, pin }: {
  template?: Template; defaultPurpose: Purpose; busy: boolean; cancel: () => void; archive: () => void;
  saved: (id: string) => Promise<void>; pin: (row: Template) => void
}) {
  const [edits, setEdits] = useState<{ input: TemplateInput; revision: number | null }>()
  const input = edits?.input ?? (template ? {
    name: template.name, purpose: template.purpose, subject: template.subject, body: template.body,
  } : { name: '', purpose: defaultPurpose, subject: '', body: '' })
  const stale = Boolean(template && edits && edits.revision !== template.revision)
  const preview = useMutation({ mutationFn: (contact_id: string) => mailCall({ action: 'preview_template', input, contact_id }, 'rendered_template') })
  const save = useMutation({ mutationFn: () => mailCall({ action: 'save_template', id: template?.id ?? null,
    expected_revision: edits?.revision ?? template?.revision ?? null, input }, 'saved'), onSuccess: async result => {
      await saved(result.id)
      setEdits(undefined)
    } })
  const isArchived = template?.archived_at != null
  const locked = busy || save.isPending || preview.isPending
  const complete = Boolean(input.name.trim() && input.subject.trim() && input.body.trim())
  function update<K extends keyof TemplateInput>(key: K, value: TemplateInput[K]) {
    if (template && !edits) pin(template)
    setEdits({ input: { ...input, [key]: value }, revision: edits?.revision ?? template?.revision ?? null })
    preview.reset()
  }
  return <><section className="panel template-editor">
    <div className="section-heading"><h2><FilePenLine aria-hidden="true" />{template ? '模板编辑' : '新增模板'}</h2>
      {template && (isArchived ? <button className="button" disabled={locked || Boolean(edits)} onClick={archive}><RotateCcw aria-hidden="true" />恢复模板</button>
        : <ConfirmButton label="归档此模板？现有草稿会保留其当前内容。" disabled={locked || Boolean(edits)} onConfirm={archive}><Archive aria-hidden="true" />归档模板</ConfirmButton>)}
    </div>
    {stale && <p className="alert warning" role="status">模板已有新版本。当前输入已保留，可复制所需内容后载入最新版本。</p>}
    <form onSubmit={event => { event.preventDefault(); if (complete && !locked && !stale && !isArchived) save.mutate() }}>
      <fieldset className="template-fields" disabled={locked}>
        <Field label="模板名称"><input name="name" value={input.name} maxLength={140} readOnly={isArchived} onChange={event => update('name', event.target.value)} required /></Field>
        <Field label="邮件用途"><Select value={input.purpose} disabled={isArchived} onChange={event => update('purpose', event.target.value as Purpose)}>
          {Object.entries(mailPurposes).map(([key, name]) => <option key={key} value={key}>{name}</option>)}
        </Select></Field>
        <Field label="邮件主题"><input name="subject" value={input.subject} maxLength={360} readOnly={isArchived} onChange={event => update('subject', event.target.value)} required /></Field>
        <Field label="纯文本正文"><textarea name="body" rows={9} value={input.body} maxLength={12000} readOnly={isArchived} onChange={event => update('body', event.target.value)} required /></Field>
      </fieldset>
      <div className="row-actions template-actions">
        {!isArchived && <><button type="submit" className="button primary" disabled={!complete || locked || stale}><Save aria-hidden="true" />保存模板</button>
        <FollowupCustomerPicker label="选择客户预览" title="选择预览客户" icon={Eye} disabled={!complete || locked} select={id => preview.mutate(id)} />
        {!template && <button type="button" className="button" onClick={cancel} disabled={locked}>取消</button>}</>}
        {template && edits && <button type="button" className="button" disabled={locked} onClick={() => {
          setEdits(undefined); save.reset(); preview.reset()
        }}><RotateCcw aria-hidden="true" />{stale ? '载入最新版本' : '放弃修改'}</button>}
      </div>
    </form>
    {preview.isError && <TemplateError error={preview.error} />}
    {save.isError && <TemplateError error={save.error} />}
  </section><TemplateGuide preview={preview.data} /></>
}

function TemplateGuide({ preview }: { preview?: TemplatePreview }) {
  return <aside className="panel template-guide">
    {preview && <Preview data={preview} />}
    <h2><ShieldCheck aria-hidden="true" />变量和使用规则</h2>
    <dl>{Object.entries(variables).map(([key, label]) => <div key={key}><dt><code>{`{{${key}}}`}</code></dt><dd>{label}</dd></div>)}</dl>
    <p className="review-preview">预览会标出缺失的客户资料。草稿中的占位符须补齐后才能提交审核或发送。</p>
  </aside>
}
function Preview({ data }: { data: TemplatePreview }) {
  return <div className="template-render" aria-label="邮件模板预览"><h3><Eye aria-hidden="true" />预览</h3>
    <strong>{data.subject}</strong><p className="mail-body">{data.body}</p>
    {data.missing.length > 0 && <p className="alert warning">缺少客户资料 · {data.missing.map(key => variables[key] ?? key).join('、')}</p>}
  </div>
}

export function DraftTemplatePicker({ contactId, onApply, onClose, hasText }: {
  contactId: string; onApply: (data: TemplatePreview) => void; onClose: () => void; hasText: boolean
}) {
  const [offset, setOffset] = useState(0)
  const [purpose, setPurpose] = useState<Purpose | ''>('')
  const [id, setId] = useState('')
  const rows = useTemplates(purpose, offset)
  const preview = useQuery({ queryKey: ['mail-template-preview', id, contactId], enabled: Boolean(id),
    queryFn: () => mailCall({ action: 'render_template', id, contact_id: contactId }, 'rendered_template') })
  const rendered = preview.data
  return <section className="template-draft-picker" aria-label="选用邮件模板">
    <div className="section-heading"><h3>选用邮件模板</h3><button type="button" className="button" onClick={onClose}><X aria-hidden="true" />关闭</button></div>
    <div className="form-grid">
      <Field label="模板用途"><Select value={purpose} onChange={event => { setPurpose(event.target.value as Purpose | ''); setOffset(0); setId('') }}>
        <option value="">全部用途</option>{Object.entries(mailPurposes).map(([key, label]) => <option value={key} key={key}>{label}</option>)}
      </Select></Field>
      <Field label="在用模板"><Select value={id} onChange={event => setId(event.target.value)}>
        <option value="">{rows.isPending ? '正在读取模板…' : '选择模板'}</option>{rows.data?.rows.map(row => <option value={row.id} key={row.id}>{row.name}</option>)}
      </Select></Field>
    </div>
    {rows.data && <Pagination offset={offset} more={rows.data.has_more} change={next => { setOffset(next); setId('') }} />}
    {(rows.isError || preview.isError) && <TemplateError error={rows.error ?? preview.error} />}
    {rows.data?.rows.length === 0 && <Empty>当前没有在用模板。</Empty>}
    {id && preview.isPending && <small>正在生成预览…</small>}
    {rendered && <><Preview data={rendered} />{rendered.missing.length === 0 && (hasText
      ? <ConfirmButton primary label="用模板预览替换当前草稿主题和正文？" onConfirm={() => onApply(rendered)}>替换主题和正文</ConfirmButton>
      : <button type="button" className="button primary" onClick={() => onApply(rendered)}>填入草稿</button>)}</>}
  </section>
}
