import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { request } from './api'
import { Empty, Field, Problem } from './components'
import { fieldColors, fieldEntries, fieldKinds, type CustomField, type FieldChoice, type FieldColor,
  type FieldDefinition, type FieldKind, type FieldPatch, type FieldValue } from './fieldTypes'

type DraftValues = Record<string, FieldValue | null>
type FieldInputProps = {
  label: string; kind: FieldKind; options: FieldChoice[]
  value: FieldValue | null | undefined; change: (value: FieldValue | null) => void
}

export function useCustomerFields(includeArchived = false) {
  return useQuery({
    queryKey: ['crm-fields', includeArchived],
    queryFn: () => request({ action: 'list_fields', include_archived: includeArchived }, 'fields'),
  })
}

export function FieldInput({ label, kind, options, value, change }: FieldInputProps) {
  if (kind === 'multi_select') {
    const selected = value?.type === 'multi_select' ? value.value : []
    const available = options.filter(option => !option.archived || selected.includes(option.id))
    return <fieldset className="custom-multi"><legend>{label}</legend>
      {available.length === 0 && <small className="muted">请先配置选项。</small>}
      {available.map(option => <label key={option.id}>
        <input type="checkbox" checked={selected.includes(option.id)} onChange={event => {
          const next = event.target.checked ? [...selected, option.id] : selected.filter(id => id !== option.id)
          change(next.length ? { type: kind, value: next } : null)
        }} /> {option.label}{option.archived && '（已归档）'}</label>)}
    </fieldset>
  }
  if (kind === 'single_select') {
    const selected = value?.type === kind ? value.value : ''
    return <Field label={label}><select value={selected} onChange={event => change(event.target.value ? { type: kind, value: event.target.value } : null)}>
      <option value="">未设置</option>
      {options.filter(option => !option.archived || selected === option.id).map(option =>
        <option key={option.id} value={option.id} disabled={option.archived}>{option.label}{option.archived && '（已归档）'}</option>)}
    </select></Field>
  }
  if (kind === 'checkbox') {
    const selected = value?.type === kind ? String(value.value) : ''
    return <Field label={label}><select value={selected} onChange={event =>
      change(event.target.value ? { type: kind, value: event.target.value === 'true' } : null)
    }><option value="">未设置</option><option value="true">是</option><option value="false">否</option></select></Field>
  }
  if (kind === 'number') {
    return <Field label={label}><input type="number" step="any" value={value?.type === kind ? value.value : ''}
      onChange={event => change(event.target.value === '' ? null : { type: kind, value: event.target.valueAsNumber })} /></Field>
  }
  return <Field label={label}><input type={kind} maxLength={kind === 'url' ? 1000 : 2000} value={value?.type === kind ? value.value : ''}
    onChange={event => change(event.target.value ? { type: kind, value: event.target.value } : null)} /></Field>
}

export function CustomValueInputs({ fields, values, defaults = false, change }: {
  fields: CustomField[]; values: DraftValues; defaults?: boolean
  change: (id: string, value: FieldValue | null) => void
}) {
  return <div className="form-grid">{fields.map(field =>
    <FieldInput key={field.id} label={field.name} kind={field.kind} options={field.options}
      value={field.id in values ? values[field.id] : defaults ? field.default_value : undefined}
      change={value => change(field.id, value)} />)}</div>
}

export function displayFieldValue(field: CustomField, value: FieldValue | null | undefined) {
  if (value == null) return '—'
  if (value.type === 'checkbox') return value.value ? '是' : '否'
  if (value.type === 'single_select' || value.type === 'multi_select') {
    const ids = value.type === 'single_select' ? [value.value] : value.value
    return ids.map(id => {
      const option = field.options.find(option => option.id === id)
      return option ? option.label + (option.archived ? '（已归档）' : '') : '历史选项'
    }).join('、')
  }
  return String(value.value)
}

export function NewCustomerFields({ values, change }: {
  values: DraftValues; change: (id: string, value: FieldValue | null) => void
}) {
  const fields = useCustomerFields()
  if (fields.isError) return <Problem error={fields.error} />
  if (!fields.data?.fields.length) return null
  return <details className="panel" open><summary>自定义属性</summary>
    <CustomValueInputs fields={fields.data.fields} values={values} change={change} defaults />
  </details>
}

export function CustomerFieldPanel({ contactId, archived }: { contactId: string; archived: boolean }) {
  const fields = useCustomerFields()
  const values = useQuery({
    queryKey: ['contact-fields', contactId],
    queryFn: () => request({ action: 'get_field_values', contact_id: contactId }, 'field_values'),
    enabled: Boolean(fields.data?.fields.length),
  })
  const [hideEmpty, setHideEmpty] = useState(false)
  const [editBase, setEditBase] = useState<FieldPatch | null>(null)
  const [draft, setDraft] = useState<DraftValues>({})
  const cache = useQueryClient()
  const current = values.data?.record
  const visible = fields.data?.fields ?? []
  const persisted: DraftValues = Object.fromEntries(current?.entries.map(entry => [entry.field_id, entry.value]) ?? [])
  const save = useMutation({
    mutationFn: (patch: FieldPatch) => request({ action: 'patch_field_values', patch }, 'saved'),
    onSuccess: async () => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['contact-fields', contactId] }),
        cache.invalidateQueries({ queryKey: ['contact', contactId] }), cache.invalidateQueries({ queryKey: ['contacts'] })])
      setEditBase(null)
      setDraft({})
    },
  })
  if (fields.isError) return <Problem error={fields.error} />
  if (visible.length === 0) return null
  const view = visible.filter(field => field.show_in_details && (!hideEmpty || persisted[field.id] != null))
  const stale = current && editBase && (current.revision !== editBase.expected_revision || current.epoch !== editBase.expected_epoch)
  return <section className="panel">
    <div className="section-heading"><h2>自定义属性</h2>
      {!editBase && <div className="row-actions">
        <label><input type="checkbox" checked={hideEmpty} onChange={event => setHideEmpty(event.target.checked)} /> 隐藏空属性</label>
        {!archived && current && <button className="button" onClick={() => {
          save.reset()
          setEditBase({ contact_id: contactId, expected_revision: current.revision, expected_epoch: current.epoch, entries: [] })
        }}>编辑属性</button>}
      </div>}
    </div>
    {values.isError && <Problem error={values.error} />}
    {values.isPending && <Empty>正在读取自定义属性…</Empty>}
    {editBase ? <form onSubmit={event => { event.preventDefault(); save.mutate({ ...editBase, entries: fieldEntries(draft) }) }}>
      {stale && <p className="alert warning" role="status">客户已被其他操作更新，当前输入已保留。取消编辑后可读取最新资料。</p>}
      <fieldset disabled={save.isPending}>
        <CustomValueInputs fields={visible} values={{ ...persisted, ...draft }} change={(id, value) => setDraft(old => ({ ...old, [id]: value }))} />
        {save.isError && <Problem error={save.error} />}
        <div className="form-footer">
          <button className="button" type="button" onClick={() => { setEditBase(null); setDraft({}) }}>取消</button>
          <button className="button primary" disabled={!Object.keys(draft).length || Boolean(stale)}>保存属性</button>
        </div>
      </fieldset>
    </form> : current && (view.length ? <dl className="profile-grid">
      {view.map(field => <div key={field.id}><dt>{field.name}</dt><dd><span className="custom-field-value" data-field-color={field.color}>{displayFieldValue(field, persisted[field.id])}</span></dd></div>)}
    </dl> : <Empty>没有要显示的属性。</Empty>)}
  </section>
}

const accessLabels = { hidden: '仅工作台可见', read: 'Agent 可读', write: 'Agent 可读写' } as const

export function FieldManager({ back }: { back: () => void }) {
  const fields = useCustomerFields(true)
  const cache = useQueryClient()
  const [edit, setEdit] = useState<CustomField | 'new' | null>(null)
  const [showArchived, setShowArchived] = useState(false)
  const operation = useMutation({
    mutationFn: (data: { id: string; expected_revision: number; direction: -1 | 1 } | { id: string; expected_revision: number; archived: boolean }) =>
      'direction' in data ? request({ action: 'move_field', ...data }, 'saved')
        : request({ action: 'archive_field', ...data }, 'saved'),
    onSuccess: () => Promise.all([cache.invalidateQueries({ queryKey: ['crm-fields'] }),
      cache.invalidateQueries({ queryKey: ['contact-fields'] })]),
  })
  const list = fields.data?.fields.filter(field => showArchived ? field.archived : !field.archived) ?? []
  return <>
    <div className="page-heading"><h1>客户字段</h1><div className="row-actions">
      <button className="button" onClick={back}>返回客户</button>
      {!edit && <button className="button primary" onClick={() => setEdit('new')}>新增字段</button>}
    </div></div>
    {edit && <DefinitionEditor key={edit === 'new' ? 'new' : edit.id}
      field={edit === 'new' ? undefined : edit} close={() => setEdit(null)} />}
    <section className="panel">
      <div className="section-heading"><h2>自定义字段</h2>
        <label><input type="checkbox" checked={showArchived} onChange={event => setShowArchived(event.target.checked)} /> 显示已归档字段</label>
      </div>
      {fields.isError && <Problem error={fields.error} />}
      {operation.isError && <Problem error={operation.error} />}
      {fields.isPending && <Empty>正在读取字段…</Empty>}
      {fields.data && (list.length ? <ul className="record-list">{list.map((field, index) => {
        const target = { id: field.id, expected_revision: field.revision }
        return <li key={field.id}><div><strong className="custom-field-value" data-field-color={field.color}>{field.name}</strong>
          <small> {fieldKinds[field.kind]} · {accessLabels[field.agent_access]}</small></div>
          <div className="row-actions">
            {!field.archived && <>
              <button className="button" disabled={Boolean(edit)} onClick={() => setEdit(field)}>编辑</button>
              <button className="button" aria-label={'上移 ' + field.name} disabled={operation.isPending || index === 0} onClick={() => operation.mutate({ ...target, direction: -1 })}>上移</button>
              <button className="button" aria-label={'下移 ' + field.name} disabled={operation.isPending || index === list.length - 1} onClick={() => operation.mutate({ ...target, direction: 1 })}>下移</button>
            </>}
            <button className="button" disabled={operation.isPending || Boolean(edit)} onClick={() => operation.mutate({ ...target, archived: !field.archived })}>
              {field.archived ? '恢复' : '归档'}
            </button>
          </div></li>
      })}</ul> : <Empty>{showArchived ? '没有已归档字段。' : '尚无自定义字段。'}</Empty>)}
    </section>
  </>
}

function ColorSelect({ value, change }: { value: FieldColor; change: (color: FieldColor) => void }) {
  return <select value={value} onChange={event => change(event.target.value as FieldColor)}>
    {Object.entries(fieldColors).map(([id,label]) => <option value={id} key={id}>{label}</option>)}
  </select>
}

function DefinitionEditor({ field, close }: { field?: CustomField; close: () => void }) {
  const [draft, setDraft] = useState<FieldDefinition>(() => ({
    id: field?.id ?? null, expected_revision: field?.revision ?? null, name: field?.name ?? '', kind: field?.kind ?? 'text',
    color: field?.color ?? 'gray', agent_access: field?.agent_access ?? 'read', show_in_details: field?.show_in_details ?? true,
    default_value: field?.default_value ?? null, options: field?.options ?? [],
  }))
  const cache = useQueryClient()
  const save = useMutation({
    mutationFn: () => request({ action: 'save_field', definition: draft }, 'saved'),
    onSuccess: async () => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['crm-fields'] }), cache.invalidateQueries({ queryKey: ['contact-fields'] })])
      close()
    },
  })
  function update<K extends keyof FieldDefinition>(key: K, value: FieldDefinition[K]) {
    setDraft(current => ({ ...current, [key]: value }))
  }
  function updateChoice(id: string, change: Partial<FieldChoice>) {
    setDraft(current => {
      const options = current.options.map(option => option.id === id ? { ...option, ...change } : option)
      let value = current.default_value
      if (change.archived && value?.type === 'single_select' && value.value === id) value = null
      if (change.archived && value?.type === 'multi_select') {
        const choices = value.value.filter(option => option !== id)
        value = choices.length ? { type: 'multi_select', value: choices } : null
      }
      return { ...current, options, default_value: value }
    })
  }
  function moveChoice(index: number, offset: number) {
    const options = [...draft.options]
    const neighbor = index + offset
    if (neighbor < 0 || neighbor >= options.length) return
    const moved = options[index]
    options[index] = options[neighbor]
    options[neighbor] = moved
    update('options', options)
  }
  function submit(event: FormEvent) { event.preventDefault(); save.mutate() }
  return <form className="panel" onSubmit={submit}>
    <h2>{field ? '编辑字段' : '新增字段'}</h2>
    <fieldset disabled={save.isPending}>
      <div className="form-grid">
        <Field label="字段名称"><input value={draft.name} maxLength={96} onChange={event => update('name', event.target.value)} required /></Field>
        <Field label="字段类型"><select disabled={Boolean(field)} value={draft.kind} onChange={event =>
          setDraft(current => ({ ...current, kind: event.target.value as FieldKind, options: [], default_value: null }))}>
          {Object.entries(fieldKinds).map(([id,label]) => <option key={id} value={id}>{label}</option>)}
        </select></Field>
        <Field label="标签颜色"><ColorSelect value={draft.color} change={value => update('color', value)} /></Field>
        <Field label="Agent 权限"><select value={draft.agent_access} onChange={event => update('agent_access', event.target.value as keyof typeof accessLabels)}>
          {Object.entries(accessLabels).map(([id,label]) => <option key={id} value={id}>{label}</option>)}
        </select></Field>
        <label><input type="checkbox" checked={draft.show_in_details} onChange={event => update('show_in_details', event.target.checked)} /> 在客户详情显示</label>
        <FieldInput label="新客户的默认值" kind={draft.kind} options={draft.options} value={draft.default_value} change={value => update('default_value', value)} />
      </div>
      {(draft.kind === 'single_select' || draft.kind === 'multi_select') && <section className="custom-options">
        <h3>选项</h3>
        {draft.options.map((option,index) => <div className="form-grid" key={option.id}>
          <Field label="选项名称"><input value={option.label} maxLength={80} onChange={event => updateChoice(option.id, { label: event.target.value })} required /></Field>
          <Field label="选项颜色"><ColorSelect value={option.color} change={color => updateChoice(option.id, { color })} /></Field>
          <div className="row-actions">
            <button type="button" className="button" disabled={index === 0} onClick={() => moveChoice(index,-1)}>上移</button>
            <button type="button" className="button" disabled={index === draft.options.length - 1} onClick={() => moveChoice(index,1)}>下移</button>
            <button type="button" className="button" onClick={() => updateChoice(option.id, { archived: !option.archived })}>{option.archived ? '恢复选项' : '归档选项'}</button>
          </div>
        </div>)}
        <button type="button" className="button" disabled={draft.options.length >= 36} onClick={() => update('options',
          [...draft.options, { id: crypto.randomUUID(), label: '', color: 'gray', archived: false }])}>添加选项</button>
      </section>}
      {save.isError && <Problem error={save.error} />}
      <div className="form-footer"><button className="button" type="button" onClick={close}>取消</button>
        <button className="button primary">保存字段</button></div>
    </fieldset>
  </form>
}
