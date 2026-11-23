import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { columnResizingFeature, columnSizingFeature, functionalUpdate, rowSelectionFeature,
  tableFeatures, useTable, type ColumnDef } from '@tanstack/react-table'
import { dateLabel, request, sourceLabel, stages, type ContactRow, type StateOption } from './api'
import { ConfirmButton, Field, Problem } from './components'
import { displayFieldValue, FieldInput } from './Fields'
import type { FieldValue } from './fieldTypes'
import type { AvailableColumn, CellTarget, ColumnSetting } from './viewTypes'

const features = tableFeatures({ columnSizingFeature, columnResizingFeature, rowSelectionFeature })
const canEdit = new Set(['name', 'company', 'country', 'stage', 'level', 'sample'])

export function targetFor(row: ContactRow): CellTarget {
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
    case 'last_contact': return dateLabel(row.last_contact_at)
    case 'email': return row.email ?? ''
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

function InlineCell({ row, column, catalog, finish }: {
  row: ContactRow; column: AvailableColumn; catalog: StateOption[]; finish: (next?: -1 | 1) => void
}) {
  const [system, setSystem] = useState(() => systemValue(row,column.id))
  const [customValue, setCustomValue] = useState<FieldValue | null | undefined>(undefined)
  const [target] = useState(() => targetFor(row))
  const cache = useQueryClient()
  const custom = column.custom
  const values = useQuery({
    queryKey: ['contact-fields',row.id],
    queryFn: () => request({ action: 'get_field_values', contact_id: row.id },'field_values'),
    enabled: Boolean(custom),
  })
  const record = values.data?.record
  const existing = record?.entries.find(entry => entry.field_id === column.id)?.value
  const [initialSystem] = useState(system)
  const changed = custom ? customValue !== undefined : system !== initialSystem
  const save = useMutation<unknown, Error, -1 | 1 | undefined>({
    mutationFn: async () => {
      if (custom && record) return request({ action: 'patch_field_values', patch: {
        contact_id: row.id, expected_revision: target.revision, expected_epoch: target.epoch,
        entries: [{ field_id: column.id, value: customValue ?? null }],
      } }, 'saved')
      const result = await request({ action: 'update_cells', targets: [target], field: column.id, value: system },'cells_updated','duplicates')
      if (result.kind === 'duplicates') throw new Error('发现可能重复的客户，请打开详情核对后编辑。')
      return result
    },
    onSuccess: async (_,direction) => {
      await Promise.all([cache.invalidateQueries({ queryKey: ['contacts'] }),cache.invalidateQueries({ queryKey: ['contact',row.id] }),
        cache.invalidateQueries({ queryKey: ['contact-fields',row.id] })])
      finish(direction)
    },
  })
  const saveable = changed && !save.isPending && (!custom || Boolean(record))
  return <form className="cell-editor" onSubmit={event => { event.preventDefault(); if (saveable) save.mutate(undefined) }}
    onKeyDown={event => {
      if (event.key === 'Escape') { event.stopPropagation(); finish() }
      if (event.key === 'Tab' && saveable) { event.preventDefault(); save.mutate(event.shiftKey ? -1 : 1) }
    }}>
    {custom ? (values.isError ? <Problem error={values.error} /> : record ?
      <FieldInput label={column.title} kind={custom.kind} options={custom.options} value={customValue === undefined ? existing : customValue} change={setCustomValue} />
      : <small>正在读取…</small>)
      : <SystemEditor field={column.id} label={column.title} value={system} change={setSystem} catalog={catalog} />}
    {save.isError && <Problem error={save.error} />}
    <div className="row-actions"><button className="button primary" disabled={!saveable}>保存</button>
      <button type="button" className="button" onClick={() => finish()}>取消</button></div>
  </form>
}

export default function CustomerTable({ data, available, columns, density, catalog, open, resize, sort }: {
  data: ContactRow[]; available: AvailableColumn[]; columns: ColumnSetting[]; density: string
  catalog: StateOption[]; open: (id: string) => void; resize: (widths: Record<string,number>) => void
  sort: (field: string) => void
}) {
  const [editing, setEditing] = useState<string | null>(null)
  const [clipboardError, setClipboardError] = useState(false)
  const [bulkField, setBulkField] = useState('stage')
  const [bulkValue, setBulkValue] = useState('')
  const cache = useQueryClient()
  const columnMeta = new Map(available.map(column => [column.id,column]))
  const visible = columns.filter(column => !column.hidden && columnMeta.has(column.field))
  const widths = Object.fromEntries(visible.map(column => [column.field,column.width]))
  const defs: ColumnDef<typeof features, ContactRow>[] = visible.map(setting => ({
    id: setting.field, header: setting.label ?? columnMeta.get(setting.field)?.title ?? setting.field, size: setting.width,
    accessorFn: row => valueText(row,columnMeta.get(setting.field)!,catalog),
  }))
  const table = useTable({
    features, columns: defs, data, getRowId: row => row.id, enableRowSelection: row => row.original.archived_at === null,
    defaultColumn: { minSize: 72, maxSize: 640 },
    columnResizeMode: 'onEnd',
    state: { columnSizing: widths },
    onColumnSizingChange: updater => resize(functionalUpdate(updater,widths)),
  })
  const selected = table.getSelectedRowModel().rows
  const bulk = useMutation({
    mutationFn: () => request({ action: 'update_cells', targets: selected.map(row=>targetFor(row.original)), field: bulkField, value: bulkValue },'cells_updated'),
    onSuccess: async () => {
      table.resetRowSelection()
      await Promise.all([cache.invalidateQueries({ queryKey: ['contacts'] }), cache.invalidateQueries({ queryKey: ['contact'] })])
    },
  })
  function finish(key: string, next?: -1 | 1) {
    setEditing(null)
    window.requestAnimationFrame(() => {
      const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-edit-cell]'))
      const index = buttons.findIndex(button => button.dataset.editCell === key)
      if (index >= 0) (buttons[index + (next ?? 0)] ?? buttons[index]).focus()
    })
  }
  return <>
    {clipboardError && <p className="alert warning" role="status">无法写入剪贴板。<button className="text-button" onClick={() => setClipboardError(false)}>关闭</button></p>}
    {selected.length > 0 && <div className="row-actions panel">
      <strong>已选 {selected.length} 位客户</strong>
      <Field label="批量修改字段"><select value={bulkField} onChange={event => { setBulkField(event.target.value); setBulkValue('') }}>
        <option value="stage">客户阶段</option><option value="level">客户等级</option><option value="sample">样品进度</option>
      </select></Field>
      <SystemEditor field={bulkField} label="目标值" value={bulkValue} change={setBulkValue} catalog={catalog} />
      <ConfirmButton label={'将 ' + selected.length + ' 位客户的选定字段更新为同一值？'}
        disabled={bulk.isPending || !bulkValue || selected.length > 50} onConfirm={() => bulk.mutate()}>确认批量修改</ConfirmButton>
      {bulk.isError && <Problem error={bulk.error} />}
    </div>}
    <div className={'table-wrap customer-grid is-' + density}><table>
      <thead>{table.getHeaderGroups().map(group => <tr key={group.id}>
        <th className="grid-row-select"><input type="checkbox" aria-label="选择本页客户" checked={table.getIsAllPageRowsSelected()} onChange={table.getToggleAllPageRowsSelectedHandler()} /></th>
        {group.headers.map(header => <th key={header.id} style={{ width: header.getSize(), minWidth: header.getSize() }}>
          <button className="text-button" onClick={() => sort(header.column.id)} disabled={columnMeta.get(header.column.id)?.kind === 'multi_select'}>
            <table.FlexRender header={header} />
          </button>
          <span role="separator" aria-label="调整列宽" tabIndex={0} className="column-resize"
            onMouseDown={header.getResizeHandler()} onTouchStart={header.getResizeHandler()}
            onKeyDown={event => {
              if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                event.preventDefault()
                const width = Math.max(72, Math.min(640, header.getSize() + (event.key === 'ArrowLeft' ? -12 : 12)))
                table.setColumnSizing(previous => ({ ...previous, [header.column.id]: width }))
              }
            }} />
        </th>)}</tr>)}</thead>
      <tbody>{table.getRowModel().rows.map(entry => <tr key={entry.id} data-selected={entry.getIsSelected()}>
        <td className="grid-row-select"><input type="checkbox" aria-label={'选择 ' + entry.original.display_name} checked={entry.getIsSelected()}
          onChange={entry.getToggleSelectedHandler()} disabled={!entry.getCanSelect()} /></td>
        {entry.getAllCells().map(cell => {
          const column = columnMeta.get(cell.column.id)!
          const value = valueText(entry.original,column,catalog)
          const editable = (canEdit.has(column.id) || Boolean(column.custom)) && entry.original.archived_at === null
          const status = column.id === 'stage' || column.id === 'level' || column.id === 'sample'
          const display = status && value !== '—'
            ? <span className="customer-status" data-status={column.id} data-stage={column.id === 'stage' ? entry.original.stage : undefined}>{value}</span>
            : value
          if (editing === cell.id) return <td key={cell.id}><InlineCell row={entry.original} column={column} catalog={catalog} finish={next => finish(cell.id,next)} /></td>
          return <td key={cell.id}>
            {column.id === 'name' ? <button className="text-button customer-name" onClick={() => open(entry.id)}>{value}</button>
              : editable ? <button className="grid-cell-button" data-edit-cell={cell.id} onDoubleClick={() => setEditing(cell.id)}
                onKeyDown={event => {
                  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'c') {
                    event.preventDefault()
                    void navigator.clipboard.writeText(value).catch(() => setClipboardError(true))
                  } else if (event.key === 'Enter' || event.key === 'F2') {
                    event.preventDefault()
                    setEditing(cell.id)
                  }
                }}
                title="双击或按 Enter 编辑">{display}</button> : <span>{display}</span>}
            {column.id === 'name' && editable && <button type="button" data-edit-cell={cell.id} className="grid-edit-name text-button"
              aria-label={'编辑姓名 ' + value} onClick={() => setEditing(cell.id)}>编辑</button>}
          </td>
        })}
      </tr>)}</tbody>
    </table></div>
  </>
}
