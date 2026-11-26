import { useEffect, useRef, useState, type MouseEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { columnResizingFeature, columnSizingFeature, functionalUpdate, rowSelectionFeature,
  tableFeatures, useTable, type ColumnDef } from '@tanstack/react-table'
import { dateLabel, outreachLabels, request, shortDate, sourceLabel, stages, type ContactQuery, type ContactRow, type StateOption } from './api'
import { ConfirmButton, Field, Problem } from './components'
import { displayFieldValue, FieldInput } from './Fields'
import type { FieldValue } from './fieldTypes'
import type { AvailableColumn, CellTarget, ColumnSetting } from './viewTypes'
import { ActionMenu, IconButton, MenuAction } from '../ui/WorkbenchUI'

import { CustomerIcon } from './CustomerIcon'

const features = tableFeatures({ columnSizingFeature, columnResizingFeature, rowSelectionFeature })
const canEdit = new Set(['name', 'company', 'country', 'stage', 'level', 'sample'])

export function targetFor(row: Pick<ContactRow, 'id' | 'record_revision' | 'data_epoch'>): CellTarget {
  return { id: row.id, revision: row.record_revision, epoch: row.data_epoch }
}

function optionsFor(field: string, options: StateOption[]): Array<Omit<StateOption, 'field'>> {
  if (field === 'stage') return Object.entries(stages).map(([code,label]) => ({ code, label, aliases: [] }))
  const code = field === 'level' ? 'customer_level' : 'sample_status'
  return options.filter(option => option.field === code)
}

function statusLabel(field: string, value: string, options: StateOption[]) {
  const key = value.toLowerCase().trim()
  return optionsFor(field,options).find(option => option.code.toLowerCase() === key || option.aliases.includes(key))?.label ?? value
}

function systemValue(row: ContactRow, field: string) {
  switch (field) {
    case 'name': return row.display_name
    case 'company': return row.company_name
    case 'country': return row.country_region
    case 'stage': return row.stage
    case 'level': return row.customer_level
    case 'sample': return row.sample_status
    case 'product': return row.primary_interest
    case 'last_contact': return shortDate(row.last_contact_at)
    case 'email': return row.email ?? ''
    case 'mail_status': return outreachLabels[row.mail_outreach.status]
    case 'mail_last_sent': return shortDate(row.mail_outreach.last_sent_at)
    case 'mail_last_reply': return shortDate(row.mail_outreach.last_reply_at)
    case 'mail_count': return String(row.mail_outreach.outreach_count)
    case 'mail_subject': return row.mail_outreach.recent_subject ?? ''
    case 'source': return sourceLabel(row.source)
    default: return ''
  }
}

function valueText(row: ContactRow, column: AvailableColumn, catalog: StateOption[]) {
  if (column.custom) return displayFieldValue(column.custom,row.custom_values.find(entry => entry.field_id === column.id)?.value)
  const value = systemValue(row,column.id)
  return value ? column.kind === 'single_select' ? statusLabel(column.id,value,catalog) : value : '—'
}

function SystemEditor({ field, label, value, change, catalog }: {
  field: string; label: string; value: string; change: (value: string) => void; catalog: StateOption[]
}) {
  if (['stage','level','sample'].includes(field)) {
    const options = optionsFor(field,catalog)
    const matching = options.find(option => option.code === value || option.aliases.includes(value.trim().toLowerCase()))
    return <Field label={label}><select autoFocus value={matching?.code ?? value} onChange={event => change(event.target.value)}>
      {field !== 'stage' && <option value="">未设置</option>}
      {value && !matching && <option value={value}>{value}</option>}
      {options.map(option => <option value={option.code} key={option.code}>{option.label}</option>)}
    </select></Field>
  }
  return <Field label={label}><input autoFocus value={value} onChange={event => change(event.target.value)} /></Field>
}

export function CustomerCellEditor({ record, initialValue, column, catalog, anchor, finish }: {
  record: Pick<ContactRow, 'id' | 'record_revision' | 'data_epoch'>; initialValue: string; column: AvailableColumn
  catalog: StateOption[]; anchor: HTMLElement; finish: (next?: -1 | 1, focus?: boolean) => void
}) {
  const [system, setSystem] = useState(initialValue)
  const [customValue, setCustomValue] = useState<FieldValue | null | undefined>(undefined)
  const [target] = useState(() => targetFor(record))
  const cache = useQueryClient()
  const custom = column.custom
  const values = useQuery({
    queryKey: ['contact-fields',target.id],
    queryFn: () => request({ action: 'get_field_values', contact_id: target.id },'field_values'),
    enabled: Boolean(custom),
  })
  const fieldRecord = values.data?.record
  const existing = fieldRecord?.entries.find(entry => entry.field_id === column.id)?.value
  const [initialSystem] = useState(system)
  const changed = custom ? customValue !== undefined : system !== initialSystem
  const save = useMutation<unknown, Error, -1 | 1 | undefined>({
    mutationFn: async () => {
      if (custom && fieldRecord) return request({ action: 'patch_field_values', patch: {
        contact_id: target.id, expected_revision: target.revision, expected_epoch: target.epoch,
        entries: [{ field_id: column.id, value: customValue ?? null }],
      } }, 'saved')
      const result = await request({ action: 'update_cells', targets: [target], field: column.id, value: system },'cells_updated','duplicates')
      if (result.kind === 'duplicates') throw new Error('发现可能重复的客户，请打开详情核对后编辑。')
      return result
    },
    onSuccess: async (_,direction) => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['contacts'] }),cache.invalidateQueries({ queryKey: ['contact',target.id] }),
        cache.invalidateQueries({ queryKey: ['contact-fields',target.id] })])
      finish(direction)
    },
  })
  const saveable = changed && !save.isPending && (!custom || Boolean(fieldRecord))
  const popover = useRef<HTMLFormElement>(null)
  useEffect(() => {
    const form = popover.current!
    function place() {
      const rect = anchor.getBoundingClientRect()
      const left = Math.max(10, Math.min(rect.left, window.innerWidth - form.offsetWidth - 10))
      const top = rect.bottom + 8 + form.offsetHeight <= window.innerHeight
        ? rect.bottom + 6 : Math.max(10, rect.top - form.offsetHeight - 6)
      Object.assign(form.style, { left: left + 'px', top: top + 'px' })
    }
    form.showPopover()
    place()
    form.querySelector<HTMLElement>('input,select,textarea')?.focus()
    const bounds = new ResizeObserver(place)
    bounds.observe(form)
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      bounds.disconnect()
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
    }
  }, [anchor])
  return <form ref={popover} popover="auto" className="cell-editor" aria-label={'编辑 ' + column.title}
    onToggle={event => { if (event.newState === 'closed') finish(undefined, false) }}
    onSubmit={event => { event.preventDefault(); if (saveable) save.mutate(undefined) }}
    onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish() }
      if (event.key === 'Tab' && saveable) { event.preventDefault(); save.mutate(event.shiftKey ? -1 : 1) }
    }}>
    {custom ? (values.isError ? <Problem error={values.error} /> : fieldRecord ?
      <FieldInput label={column.title} kind={custom.kind} options={custom.options} value={customValue === undefined ? existing : customValue} change={setCustomValue} />
      : <small>正在读取…</small>)
      : <SystemEditor field={column.id} label={column.title} value={system} change={setSystem} catalog={catalog} />}
    {save.isError && <Problem error={save.error} />}
    <div className="row-actions"><button className="button primary" disabled={!saveable}><CustomerIcon name="save" />保存</button>
      <button type="button" className="button" onClick={() => finish()}>取消</button></div>
  </form>
}

export type BulkSelection = {
  targets: Record<string, CellTarget>
  match?: { count: number; fingerprint: string }
}

export default function CustomerTable({ data, available, columns, density, catalog, open, resize, sort, order, activeRow, editCustomer, showMail, emailPreview, bulkQuery, selection, setSelection, filtering }: {
  data: ContactRow[]; available: AvailableColumn[]; columns: ColumnSetting[]; density: string
  catalog: StateOption[]; open: (id: string) => void; resize: (widths: Record<string,number>) => void
  sort: (field: string) => void; order?: { field: string; descending: boolean }
  activeRow: string | null; editCustomer: (id: string) => void; showMail: (id: string) => void; emailPreview: boolean
  bulkQuery: ContactQuery; selection: BulkSelection; setSelection: (value: BulkSelection) => void; filtering: boolean
}) {
  const [editing, setEditing] = useState<{ key: string; anchor: HTMLElement } | null>(null)
  const [clipboardError, setClipboardError] = useState(false)
  const [bulkField, setBulkField] = useState('stage')
  const [bulkValue, setBulkValue] = useState('')
  const [bulkNotice, setBulkNotice] = useState('')
  const cache = useQueryClient()
  const columnMeta = new Map(available.map(column => [column.id,column]))
  const visible = columns.filter(column => !column.hidden && columnMeta.has(column.field))
  const widths = Object.fromEntries(visible.map(column => [column.field,column.width]))
  const defs: ColumnDef<typeof features, ContactRow>[] = visible.map(setting => ({
    id: setting.field,
    header: setting.label ?? (setting.field === 'country' ? '国家地区' : columnMeta.get(setting.field)?.title ?? setting.field),
    size: setting.width,
    accessorFn: row => valueText(row,columnMeta.get(setting.field)!,catalog),
  }))
  const pageTargets = data.filter(row => row.archived_at === null).map(targetFor)
  const rowSelection = Object.fromEntries(pageTargets
    .filter(target => selection.match || Object.hasOwn(selection.targets, target.id)).map(target => [target.id, true as const]))
  const allPage = pageTargets.length > 0 && pageTargets.every(target => rowSelection[target.id])
  const selectedCount = selection.match?.count ?? Object.keys(selection.targets).length
  const table = useTable({
    features, columns: defs, data, getRowId: row => row.id, enableRowSelection: row => row.original.archived_at === null,
    defaultColumn: { minSize: 72, maxSize: 640 },
    columnResizeMode: 'onEnd',
    state: { columnSizing: widths, rowSelection },
    onColumnSizingChange: updater => resize(functionalUpdate(updater,widths)),
  })
  const selectMatching = useMutation({
    mutationFn: () => request({ action: 'preview_bulk_contacts', query: bulkQuery }, 'bulk_contacts'),
    onSuccess: result => {
      setBulkNotice(result.count ? '' : '当前筛选没有在用客户。')
      setSelection({ targets: {}, match: result.count ? result : undefined })
    },
  })
  const bulk = useMutation({
    mutationFn: () => request(selection.match
      ? { action: 'update_matching_contacts', query: bulkQuery, fingerprint: selection.match.fingerprint, field: bulkField, value: bulkValue }
      : { action: 'update_cells', targets: Object.values(selection.targets), field: bulkField, value: bulkValue }, 'cells_updated'),
    onSuccess: async result => {
      setSelection({ targets: {} })
      setBulkNotice('已更新 ' + result.count + ' 位客户。')
      await Promise.all([cache.invalidateQueries({ queryKey: ['contacts'] }), cache.invalidateQueries({ queryKey: ['contact'] })])
    },
  })
  function toggleTargets(targets: CellTarget[], checked: boolean) {
    const next = { ...selection.targets }
    for (const target of targets) {
      if (checked) next[target.id] ??= target
      else delete next[target.id]
    }
    setBulkNotice('')
    setSelection({ targets: next })
  }
  const selectionBusy = filtering || bulk.isPending || selectMatching.isPending
  function finish(key: string, next?: -1 | 1, focus = true) {
    setEditing(current => current?.key === key ? null : current)
    if (!focus) return
    window.requestAnimationFrame(() => {
      const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-edit-cell]'))
      const index = buttons.findIndex(button => button.dataset.editCell === key)
      if (index >= 0) (buttons[index + (next ?? 0)] ?? buttons[index]).focus()
    })
  }
  function start(key: string, anchor: HTMLElement) { setEditing({ key, anchor }) }
  return <>
    {clipboardError && <p className="alert warning" role="status">无法写入剪贴板。<button className="text-button" onClick={() => setClipboardError(false)}>关闭</button></p>}
    {bulkNotice && <p className="review-notice" role="status">{bulkNotice}</p>}
    {selectedCount > 0 && <div className="row-actions customer-bulk">
      {selectedCount > 0 && <strong>{selection.match ? '已选全部匹配的' : '跨页已选'} {selectedCount} 位客户</strong>}
      {!selection.match && pageTargets.length > 0 && <button type="button" className="button" disabled={selectionBusy}
        title="选择当前筛选中所有未归档客户，包括其他页面"
        onClick={() => selectMatching.mutate()}><CustomerIcon name="check" />选择全部匹配客户</button>}
      {selectedCount > 0 && <>
      <Field label="批量修改字段"><select value={bulkField} onChange={event => { setBulkField(event.target.value); setBulkValue('') }}>
        <option value="stage">客户阶段</option><option value="level">客户等级</option><option value="sample">样品进度</option>
      </select></Field>
      <SystemEditor field={bulkField} label="目标值" value={bulkValue} change={setBulkValue} catalog={catalog} />
      <ConfirmButton label={'将 ' + selectedCount + ' 位客户的选定字段更新为同一值？'}
        disabled={selectionBusy || !bulkValue || (!selection.match && selectedCount > 1000)} onConfirm={() => bulk.mutate()}>确认批量修改</ConfirmButton>
      <button type="button" className="text-button" disabled={selectionBusy} onClick={() => { bulk.reset(); setSelection({ targets: {} }) }}><CustomerIcon name="close" />取消选择</button>
      {!selection.match && selectedCount > 1000 && <span>逐项选择最多提交 1,000 条，可用筛选后选择全部匹配客户。</span>}
      </>}
      {bulk.isError && <Problem error={bulk.error} />}
      {selectMatching.isError && <Problem error={selectMatching.error} />}
    </div>}
    <div className={'table-wrap customer-grid is-' + density}><table aria-label="客户表格">
      <thead>{table.getHeaderGroups().map(group => <tr key={group.id}>
        <th className="grid-row-select"><input type="checkbox" aria-label="选择本页客户" checked={allPage}
          ref={input => { if (input) input.indeterminate = Boolean(Object.keys(rowSelection).length) && !allPage }}
          onChange={event => toggleTargets(pageTargets, event.target.checked)} disabled={selectionBusy || Boolean(selection.match) || !pageTargets.length} /></th>
        {group.headers.map(header => {
          const ordered = order?.field === header.column.id
          const sortable = columnMeta.get(header.column.id)?.kind !== 'multi_select'
          return <th key={header.id} scope="col" data-field={header.column.id} aria-sort={ordered ? order.descending ? 'descending' : 'ascending' : undefined}
            style={{ width: header.getSize(), minWidth: header.getSize() }}>
          <button className="text-button customer-column-sort" onClick={() => sort(header.column.id)} disabled={!sortable}>
            <span><table.FlexRender header={header} /></span>
            {sortable && <CustomerIcon name={ordered ? order.descending ? 'down' : 'up' : 'sort'} />}
          </button>
          <span role="separator" aria-label={`调整${columnMeta.get(header.column.id)?.title ?? '当前列'}列宽`}
            aria-orientation="vertical" aria-valuenow={header.getSize()} aria-valuemin={72} aria-valuemax={640}
            tabIndex={0} className="column-resize"
            onMouseDown={header.getResizeHandler()} onTouchStart={header.getResizeHandler()}
            onKeyDown={event => {
              if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                event.preventDefault()
                const width = Math.max(72, Math.min(640, header.getSize() + (event.key === 'ArrowLeft' ? -12 : 12)))
                table.setColumnSizing(previous => ({ ...previous, [header.column.id]: width }))
              }
            }} />
        </th>})}<th scope="col" className="customer-row-actions">操作</th></tr>)}</thead>
      <tbody>{table.getRowModel().rows.map(entry => <tr key={entry.id} data-selected={entry.getIsSelected()} data-active={entry.id === activeRow}>
        <td className="grid-row-select"><input type="checkbox" aria-label={'选择 ' + entry.original.display_name} checked={entry.getIsSelected()}
          onChange={event => toggleTargets([targetFor(entry.original)], event.target.checked)}
          disabled={selectionBusy || Boolean(selection.match) || !entry.getCanSelect()} /></td>
        {entry.getAllCells().map(cell => {
          const column = columnMeta.get(cell.column.id)!
          const value = valueText(entry.original,column,catalog)
          const fullDate = column.id === 'last_contact' ? entry.original.last_contact_at
            : column.id === 'mail_last_sent' ? entry.original.mail_outreach.last_sent_at
              : column.id === 'mail_last_reply' ? entry.original.mail_outreach.last_reply_at : undefined
          const secondary = emailPreview ? entry.original.email : entry.original.company_name || entry.original.email
          const editable = (canEdit.has(column.id) || Boolean(column.custom)) && entry.original.archived_at === null
          const status = column.id === 'stage' || column.id === 'level' || column.id === 'sample' || column.id === 'mail_status'
          const display = status && value !== '—'
            ? <span className="customer-status" data-status={column.id === 'mail_status' ? entry.original.mail_outreach.status : column.id} data-stage={column.id === 'stage' ? entry.original.stage : undefined}>{value}</span>
            : value
          function editOnClick(event: MouseEvent<HTMLButtonElement>) {
            if (status && column.id !== 'mail_status') start(cell.id,event.currentTarget)
          }
          const editor = editing?.key === cell.id && <CustomerCellEditor record={entry.original} column={column}
            initialValue={systemValue(entry.original, column.id)}
            anchor={editing.anchor} catalog={catalog} finish={(next, focus) => finish(cell.id,next,focus)} />
          return <td key={cell.id} data-field={column.id} data-editing={Boolean(editor)}>
            {column.id === 'name' ? <button className="text-button customer-name" aria-current={entry.id === activeRow ? 'true' : undefined}
                data-edit-cell={editable ? cell.id : undefined} onClick={() => open(entry.id)}
                onKeyDown={event => { if (event.key === 'F2' && editable) { event.preventDefault(); start(cell.id,event.currentTarget) } }}>
                <span className="customer-row-avatar" aria-hidden="true">{Array.from(value)[0]?.toUpperCase()}</span>
                <span className="customer-row-identity"><span className="customer-name-text" title={value}>{value}</span>
                  {secondary && <small title={secondary}>{secondary}</small>}</span>
              </button>
              : editable ? <button className="grid-cell-button" data-edit-cell={cell.id} onClick={editOnClick}
                onDoubleClick={event => { if (!status) start(cell.id,event.currentTarget) }}
                onKeyDown={event => {
                  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'c') {
                    event.preventDefault()
                    void navigator.clipboard.writeText(value === '—' ? '' : value).catch(() => setClipboardError(true))
                  } else if (event.key === 'F2' || event.key === 'Enter') {
                    event.preventDefault()
                    start(cell.id,event.currentTarget)
                  }
                }}
                title={status ? '点击修改' : value + ' · 双击或按 F2 编辑'}>{display}</button>
                : <span title={fullDate === undefined ? value : dateLabel(fullDate)}>{display}</span>}
              {editor}
          </td>
        })}
        <td className="customer-row-actions">
          {/* Menu follows Shadcn Admin's DataTableRowActions, using the existing FTW detail routes. */}
          <ActionMenu trigger={<IconButton label={`操作 · ${entry.original.display_name}`}><CustomerIcon name="more" /></IconButton>}>
            <MenuAction onSelect={() => requestAnimationFrame(() => open(entry.id))}><CustomerIcon name="contact" />打开详情</MenuAction>
            {entry.original.archived_at === null && <MenuAction onSelect={() => requestAnimationFrame(() => editCustomer(entry.id))}>
              <CustomerIcon name="edit" />编辑客户
            </MenuAction>}
            <MenuAction onSelect={() => requestAnimationFrame(() => showMail(entry.id))}><CustomerIcon name="mail" />邮件往来</MenuAction>
          </ActionMenu>
        </td>
      </tr>)}</tbody>
    </table></div>
  </>
}
