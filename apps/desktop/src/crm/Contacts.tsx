import { lazy, Suspense, useDeferredValue, useEffect, useId, useRef, useState, type CSSProperties } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateInput, dateLabel, fromDateInput, methodKinds, outreachLabels, request, sourceLabel, stages,
  type Contact, type ContactInput, type ContactQuery, type Duplicate, type Interest, type Method, type MethodKind, type Outreach, type Stage, type StateOption, type ContactRow } from './api'
import { ConfirmButton, Duplicates, Empty, Field, Pagination, Problem } from './components'
import { Timeline } from './Timeline'
import { mailCall, mailError, states as mailStates } from '../mail/api'
import { CustomerFieldPanel, FieldManager, NewCustomerFields, useCustomerFields } from './Fields'
import { fieldEntries, type FieldValue } from './fieldTypes'
import CustomerTable, { targetFor } from './CustomerTable'
import { ColumnSettings, SavedViewBar, useSavedViews } from './SavedViews'
import { availableColumns, columnsWithDefaults, initialColumnsForView, type AvailableColumn, type ViewConfig } from './viewTypes'

import { CustomerIcon } from './CustomerIcon'

const wideDrawer = '(min-width: 1280px)'
const AdvancedFilters = lazy(() => import('./AdvancedFilters'))

const defaultQuery: ContactQuery = {
  keyword: '', quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '', archive: 'active', sort: 'updated', offset: 0,
}

export function ContactList({ compose, history, create, importCsv }: { compose: (contact: Contact) => void; history: (id: string) => void; create: () => void; importCsv: () => void }) {
  const catalog = useStateCatalog()
  const fields = useCustomerFields()
  const views = useSavedViews()
  const records = views.data?.views ?? []
  const [activeId,setActiveId] = useState('')
  const active = records.find(view => view.id === activeId && !view.archived) ?? records.find(view => view.is_default)
  const [local,setLocal] = useState<{ id: string; revision: number | null; config: ViewConfig } | null>(null)
  const [offset,setOffset] = useState(0)
  const [showAdvanced,setShowAdvanced] = useState(false)
  const [manageFields,setManageFields] = useState(false)
  const config = local?.id === (active?.id ?? '') ? local.config : active?.config ?? {}
  const board = config.layout === 'board'
  const available = availableColumns(fields.data?.fields ?? [])
  const columns = columnsWithDefaults(config.columns?.length ? config.columns : initialColumnsForView(active?.id), available)
  const projection = columns.filter(column => !column.hidden && available.some(field => field.id === column.field && field.custom)).map(column => column.field)
  const query: ContactQuery = { ...defaultQuery, ...config.query, offset, custom_columns: projection }
  const keyword = useDeferredValue(query.keyword)
  const requestQuery = { ...query, keyword }
  const [selection, setSelection] = useState<{ id: string; entry?: 'edit' | 'mail'; sequence: number } | null>(null)
  const selected = selection?.id ?? null
  const [drawerWidth, setDrawerWidth] = useState<number | undefined>(undefined)
  const drawer = useRef<HTMLDialogElement>(null)
  const drawerTrigger = useRef<HTMLElement | null>(null)
  const search = useRef<HTMLInputElement>(null)
  function open(id: string, entry?: 'edit' | 'mail') {
    const dialog = drawer.current
    if (!dialog) return
    if (!dialog.open) {
      drawerTrigger.current = document.activeElement as HTMLElement
      if (window.matchMedia(wideDrawer).matches) dialog.show()
      else dialog.showModal()
    }
    setSelection(previous => ({ id, entry, sequence: (previous?.sequence ?? 0) + 1 }))
  }
  const close = () => drawer.current?.close()
  useEffect(() => {
    const media = window.matchMedia(wideDrawer)
    function resize() {
      const dialog = drawer.current
      if (!dialog?.open) return
      dialog.close()
      if (media.matches) dialog.show()
      else dialog.showModal()
    }
    function keyboard(event: KeyboardEvent) {
      if (event.key.toLowerCase() === 'k' && (event.ctrlKey || event.metaKey) && !document.querySelector('dialog:modal')) {
        event.preventDefault()
        search.current?.focus()
      }
    }
    media.addEventListener('change', resize)
    window.addEventListener('keydown', keyboard)
    return () => { media.removeEventListener('change', resize); window.removeEventListener('keydown', keyboard) }
  }, [])
  function configure(change: Partial<ViewConfig>, reset = true) {
    setLocal({ id: active?.id ?? '', revision: local?.id === active?.id ? local?.revision ?? null : active?.revision ?? null,
      config: { ...config, ...change } })
    if (reset) setOffset(0)
  }
  function update(change: Partial<ContactQuery>) {
    const next = { ...config.query, ...change }
    configure({ query:next })
  }
  function select(id: string) {
    setActiveId(id)
    setLocal(null)
    setOffset(0)
    setShowAdvanced(false)
  }
  const result = useQuery({
    queryKey: ['contacts', requestQuery], enabled: !board && fields.isFetched,
    queryFn: () => request({ action: 'list_contacts', query: requestQuery }, 'contacts'),
  })
  const overview = useQuery({
    queryKey: ['contacts', 'customer-stats'],
    queryFn: () => request({ action: 'customer_stats' }, 'customer_stats'),
  })
  const pageRows = result.data?.rows ?? []
  const selectedIndex = pageRows.findIndex(row => row.id === selected)
  const sideStyle = drawerWidth === undefined ? undefined : { '--customer-drawer-width': drawerWidth + 'px' } as CSSProperties
  const setWidth = (width: number) => setDrawerWidth(Math.max(340, Math.min(620, width)))
  if (manageFields) return <FieldManager back={() => setManageFields(false)} />
  return <section className="contact-workbench" aria-label="客户管理" style={sideStyle}>
    <div className="contact-work">
    <header className="page-heading customer-heading">
      <div><h1><CustomerIcon name="users" />客户管理</h1><p>跟进客户沟通、样品进度与邮件开发</p></div>
      <div className="row-actions">
        <button className="button" onClick={importCsv}>导入 CSV</button>
        <button className="button primary" onClick={create}><CustomerIcon name="plus" />新增客户</button>
      </div>
    </header>
    {views.isError && <Problem error={views.error} />}
    {fields.isError && <Problem error={fields.error} />}
    <SavedViewBar views={records} active={active} config={config} baseRevision={local?.id === active?.id ? local?.revision ?? null : null} counts={overview.data} select={select} saved={select} />
    <div className="customer-toolbar">
      <div className="customer-search">
        <span className="search-symbol"><CustomerIcon name="search" /></span>
        <input aria-label="搜索客户" ref={search} placeholder="搜索姓名、公司、邮箱或产品" value={query.keyword}
          onChange={event => update({keyword:event.target.value})} />
        <kbd aria-hidden="true">Ctrl K</kbd>
      </div>
      <label className="customer-toolbar-control"><span className="sr-only">客户阶段</span><select value={query.stage} onChange={event => update({stage:event.target.value})}>
        <option value="">全部阶段</option>{Object.entries(stages).map(([key,label]) => <option key={key} value={key}>{label}</option>)}
      </select></label>
      <label className="customer-toolbar-control"><span className="sr-only">排序</span><select value={query.order?.length ? 'column' : query.sort} onChange={event => update({sort:event.target.value as ContactQuery['sort'],order:[]})}>
        {query.order?.length ? <option value="column">按表格列排序</option> : null}
        <option value="updated">最近更新</option><option value="name">客户名称</option><option value="last_contact">最近联系</option>
      </select></label>
      <div className="layout-switch" role="group" aria-label="客户视图布局">
        <button type="button" aria-pressed={!board} onClick={() => configure({ layout: 'table' })}>表格</button>
        <button type="button" aria-pressed={board} onClick={() => configure({ layout: 'board' })}>看板</button>
      </div>
      {board && <Field label="分组字段"><select value={config.group_field ?? 'stage'} onChange={event => configure({group_field:event.target.value})}>
        <option value="stage">客户阶段</option>
        {(fields.data?.fields ?? []).filter(field => field.kind === 'single_select').map(field => <option key={field.id} value={field.id}>{field.name}</option>)}
      </select></Field>}
      <button className="button" onClick={() => setShowAdvanced(value=>!value)} aria-expanded={showAdvanced}>
        <CustomerIcon name="filter" />组合筛选{query.filter ? ' · 已应用' : ''}
      </button>
      {!board && <ColumnSettings available={available} saved={columns} change={items=>configure({columns:items},false)}
        manageFields={() => setManageFields(true)} />}
      {board && <button className="button" onClick={() => setManageFields(true)}>字段管理</button>}
      <details className="more-filters"><summary>更多筛选</summary>
        <div className="form-grid">
          <Field label="国家或地区"><input value={query.country} onChange={event=>update({country:event.target.value})} /></Field>
          <Field label="客户等级"><StateFilter field="customer_level" catalog={catalog} value={query.level} change={level=>update({level})} /></Field>
          <Field label="样品进度"><StateFilter field="sample_status" catalog={catalog} value={query.sample_status ?? ''} change={sample_status=>update({sample_status})} /></Field>
          <Field label="客户类型"><input value={query.customer_type} onChange={event=>update({customer_type:event.target.value})} /></Field>
          <Field label="客户来源"><input value={query.source} onChange={event=>update({source:event.target.value})} /></Field>
          <Field label="归档状态"><select value={query.archive} onChange={event=>update({archive:event.target.value as ContactQuery['archive']})}>
            <option value="active">未归档</option><option value="archived">已归档</option><option value="all">全部</option></select></Field>
          <Field label="行间距"><select value={config.density??'standard'} onChange={event=>configure({density:event.target.value as 'compact'|'standard'},false)}>
            <option value="standard">标准</option><option value="compact">紧凑</option></select></Field>
        </div>
      </details>
    </div>
    {showAdvanced && <Suspense fallback={<Empty>正在加载组合筛选…</Empty>}><AdvancedFilters key={active?.id}
      columns={available} catalog={catalog} filter={query.filter} close={()=>setShowAdvanced(false)}
      query={requestQuery} apply={filter=>{update({filter});setShowAdvanced(false)}} /></Suspense>}
    {overview.data && <div className="customer-overview" aria-label="全部有效客户统计">
      {([
        ['active', '全部客户', 'users'], ['priority', '重点客户', 'star'], ['sample', '样品客户', 'box'],
        ['in_progress', '跟进中', 'growth'], ['won', '已成交', 'checkCircle'],
      ] as const).map(([key, label, symbol]) => <div className="customer-metric" key={key} data-metric={key}>
        <span className="customer-metric-icon"><CustomerIcon name={symbol} /></span>
        <div><strong>{overview.data![key].toLocaleString('zh-CN')}</strong><span>{label}</span></div>
      </div>)}
    </div>}
    <section className="customer-data" aria-label={board ? '客户看板' : '客户表格'}>
    {board ? <ContactBoard query={requestQuery} group={config.group_field ?? 'stage'} available={available} open={open} /> : <>
      {result.isPending && <Empty>正在读取客户…</Empty>}
      {result.isError && <Problem error={result.error} />}
      {result.data && <>
        {result.data.rows.length === 0 ? <Empty>{isFiltered(query) || query.filter ? <>
          <p>没有符合筛选条件的客户。</p><button className="button" onClick={()=>configure({query:{...defaultQuery}})}>清除筛选</button>
        </> : <><p>当前没有客户记录。</p><div className="row-actions">
          <button className="button primary" onClick={create}>新增客户</button>
          <button className="button" onClick={importCsv}>导入客户 CSV</button>
        </div></>}</Empty> : <CustomerTable key={(active?.id ?? '') + JSON.stringify(requestQuery)} data={result.data.rows} available={available}
          columns={columns} catalog={catalog} density={config.density ?? 'standard'} open={open} activeRow={selected}
          editCustomer={id => open(id, 'edit')} showMail={id => open(id, 'mail')} emailPreview={Boolean(active?.id.startsWith('mail_'))}
          resize={widths=>configure({columns:columns.map(column=>({...column,width:widths[column.field]??column.width}))},false)}
          sort={field=>update({order:[{field,descending:query.order?.[0]?.field===field && !query.order[0].descending}]})} />}
        <div className="customer-list-footer"><span>本页 {result.data.rows.length} 位客户</span>
          <label className="customer-page-size">每页 <select aria-label="每页客户数" value={query.page_size ?? 20} onChange={event => update({page_size:Number(event.target.value) as 20|50|100})}>
            <option value="20">20</option><option value="50">50</option><option value="100">100</option></select> 条</label>
          <Pagination offset={offset} pageSize={query.page_size ?? 20} more={result.data.has_more} change={setOffset} /></div>
      </>}
    </>}
    </section>
    </div>
    <dialog className="contact-drawer" ref={drawer} aria-label="客户详情"
      onKeyDown={event => {
        if (event.key === 'Escape' && !event.defaultPrevented && (event.target as HTMLElement).closest('dialog') === event.currentTarget) close()
      }}
      onClose={event => {
        if (event.currentTarget.open) return
        setSelection(null)
        drawerTrigger.current?.focus()
      }}>
      <div className="customer-drawer-grip" role="separator" aria-label="调整客户详情宽度" aria-orientation="vertical"
        aria-valuemin={340} aria-valuemax={620} aria-valuenow={drawerWidth ?? 420} tabIndex={0}
        onPointerDown={event => event.currentTarget.setPointerCapture(event.pointerId)}
        onPointerMove={event => {
          if (event.currentTarget.hasPointerCapture(event.pointerId) && drawer.current)
            setWidth(drawer.current.getBoundingClientRect().right - event.clientX)
        }}
        onKeyDown={event => {
          if (!drawer.current || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return
          event.preventDefault()
          setWidth(drawer.current.offsetWidth + (event.key === 'ArrowLeft' ? 16 : -16))
        }} />
      <header className="contact-drawer-header"><span className="font-semibold">客户详情</span>
        <div className="row-actions">
          {!board && <><button className="button" type="button" aria-label="上一位客户" disabled={selectedIndex < 1}
            onClick={() => open(pageRows[selectedIndex - 1].id)}>‹</button>
            <button className="button" type="button" aria-label="下一位客户" disabled={selectedIndex < 0 || selectedIndex >= pageRows.length - 1}
              onClick={() => open(pageRows[selectedIndex + 1].id)}>›</button></>}
          <button className="button" type="button" autoFocus aria-label="关闭详情" onClick={close}>×</button>
        </div>
      </header>
      <div className="contact-drawer-body" role="region" aria-label="客户资料和跟进" tabIndex={0}>
        {selection && <ContactDetail key={selection.id + selection.sequence} id={selection.id} entry={selection.entry} back={close} compose={compose} history={history} drawer />}
      </div>
    </dialog>
  </section>
}

function ContactBoard({ query, group, available, open }: {
  query: ContactQuery; group:string; available:AvailableColumn[]; open:(id:string)=>void
}) {
  const [drag,setDrag] = useState<ContactRow|null>(null)
  const cache = useQueryClient()
  const custom = available.find(column=>column.id===group)?.custom
  const groups: Array<{id:string|null; label:string}> = group==='stage'
    ? Object.entries(stages).filter(([stage])=>!query.stage||stage===query.stage).map(([id,label])=>({id,label}))
    : [...(custom?.options.map(option=>({id:option.id,label:option.label+(option.archived?'（已归档）':'')})) ?? []),{id:null,label:'未设置'}]
  const move = useMutation({
    mutationFn: async ({row,value}:{row:ContactRow;value:string|null}) => {
      if (group==='stage') return request({action:'update_cells',targets:[targetFor(row)],field:'stage',value:value??''},'cells_updated')
      return request({action:'patch_field_values',patch:{contact_id:row.id,expected_revision:row.record_revision,expected_epoch:row.data_epoch,
        entries:[{field_id:group,value:value?{type:'single_select',value}:null}]}},'saved')
    },
    onSuccess:()=>Promise.all([cache.invalidateQueries({queryKey:['contacts']}),cache.invalidateQueries({queryKey:['contact']}),
      cache.invalidateQueries({queryKey:['contact-fields']})]),
  })
  if (group!=='stage'&&!custom) return <Empty>此看板的分组字段已归档，请选择另一个字段。</Empty>
  return <>
    {move.isError && <Problem error={move.error} />}
    <div className="stage-board" aria-label="客户看板">
      {groups.map(item=><BoardColumn key={JSON.stringify({...query,offset:0,group,item:item.id})}
        query={query} group={group} value={item.id} label={item.label} open={open}
        groups={groups.filter(option=>!custom?.options.find(x=>x.id===option.id&&x.archived))}
        move={(row,value)=>move.mutate({row,value})} drag={drag} setDrag={setDrag} />)}
    </div>
  </>
}

function BoardColumn({ query, group, value, label, open, groups, move, drag, setDrag }: {
  query:ContactQuery;group:string;value:string|null;label:string;open:(id:string)=>void
  groups:Array<{id:string|null;label:string}>;move:(row:ContactRow,value:string|null)=>void
  drag:ContactRow|null;setDrag:(row:ContactRow|null)=>void
}) {
  const [offset,setOffset]=useState(0)
  const filters: ContactQuery={...query,group:[group,value],offset,page_size:20,custom_columns:[]}
  const result=useQuery({queryKey:['contacts',filters],queryFn:()=>request({action:'list_contacts',query:filters},'contacts')})
  return <section className="stage-column" onDragOver={event=>{if(drag)event.preventDefault()}} onDrop={event=>{
    if(!drag)return
    event.preventDefault()
    setDrag(null)
    if(event.dataTransfer.getData('application/x-ftw-contact')===drag.id&&groups.some(x=>x.id===value))move(drag,value)
  }}>
    <h2>{label}</h2>
    {result.isError&&<Problem error={result.error}/>}
    {result.isPending&&<small>正在读取…</small>}
    {result.data?.rows.length===0&&<small>暂无客户</small>}
    {result.data?.rows.map(row=><div className="stage-card" key={row.id} draggable={row.archived_at===null}
      onDragStart={event=>{setDrag(row);event.dataTransfer.setData('application/x-ftw-contact',row.id)}}
      onDragEnd={()=>setDrag(null)}>
      <button className="text-button" onClick={()=>open(row.id)}>{row.display_name}</button>
      <small>{[row.company_name,row.country_region,row.primary_interest].filter(Boolean).join(' · ')}</small>
      {row.archived_at===null&&<select aria-label={'移动客户 ' + row.display_name} value={value??''}
        onChange={event=>move(row,event.target.value||null)}>
        {!groups.some(item=>item.id===value)&&<option value={value??''} disabled>{label}</option>}
        {groups.map(item=><option key={item.id??'unset'} value={item.id??''}>{item.label}</option>)}
      </select>}
    </div>)}
    {result.data&&<Pagination offset={offset} more={result.data.has_more} change={setOffset}/>}
  </section>
}

const blank: ContactInput = {
  display_name: '', company_name: '', country_region: '', customer_type: '', customer_source: '', customer_level: '',
  stage: 'prospect', first_contact_channel: '', primary_language: '', main_need: '', sample_status: '',
  bulk_order_status: '', first_contact_at: null, last_contact_at: null, website_url: '', alibaba_buyer_id: '',
  notes: '', methods: [], interests: [],
}
const textFields = [
  ['company_name', '公司或店铺'], ['country_region', '国家或地区'], ['customer_level', '客户等级'],
  ['customer_type', '客户类型'], ['customer_source', '客户来源'], ['first_contact_channel', '首次沟通渠道'],
  ['primary_language', '主要沟通语言'], ['sample_status', '样品进度'], ['bulk_order_status', '批量订单进度'],
  ['website_url', '网站或店铺地址'], ['alibaba_buyer_id', '阿里买家 ID'],
] as const

const editorGroups = {
  basic: '基本信息', channels: '联系方式', business: '业务状态', interests: '产品兴趣',
  custom: '自定义属性', notes: '备注与需求',
} as const
type EditorGroup = keyof typeof editorGroups

export function ContactEditor({ record, saved, cancel, initialGroup = 'basic' }: { record?: Contact; saved: (id: string) => void; cancel: () => void; initialGroup?: EditorGroup }) {
  const [input, setInput] = useState(record?.input ?? blank)
  const [original, setOriginal] = useState(input)
  const [group, setGroup] = useState<EditorGroup>(initialGroup)
  const discard = useRef<HTMLDialogElement>(null)
  const sectionPrefix = useId()
  const [custom, setCustom] = useState<Record<string, FieldValue | null>>({})
  const [base, setBase] = useState(() => record && { revision: record.record_revision, epoch: record.data_epoch })
  const changed = record && base && (record.record_revision !== base.revision || record.data_epoch !== base.epoch)
  const catalog = useStateCatalog()
  const [duplicate, setDuplicate] = useState<Duplicate[]>([])
  const cache = useQueryClient()
  const mutation = useMutation({
    mutationFn: (allow: boolean) => request(record && base
      ? { action: 'save_contact_checked', id: record.id, input, allow_duplicate: allow, expected_revision: base.revision, expected_epoch: base.epoch }
      : { action: 'create_contact_with_fields', input, allow_duplicate: allow, entries: fieldEntries(custom) }, 'saved', 'duplicates'),
    onSuccess: async result => {
      if (result.kind === 'duplicates') { setDuplicate(result.matches); return }
      await Promise.all([
        cache.invalidateQueries({ queryKey: ['contacts'] }), cache.invalidateQueries({ queryKey: ['contact', result.id] }),
        cache.invalidateQueries({ queryKey: ['lead'] }),
      ])
      saved(result.id)
    },
  })
  const update = <K extends keyof ContactInput>(key: K, value: ContactInput[K]) => {
    setDuplicate([])
    setInput(current => ({ ...current, [key]: value }))
  }
  const updateMethod = (index: number, changes: Partial<Method>) => {
    const next = { ...input.methods[index], ...changes }
    update('methods', input.methods.map((method, position) => position === index ? next
      : next.is_primary && method.kind === next.kind ? { ...method, is_primary: false } : method))
  }
  const updateInterest = (index: number, changes: Partial<Interest>) =>
    update('interests', input.interests.map((interest, position) => position === index ? { ...interest, ...changes } : interest))

  const dirty = input !== original || Object.keys(custom).length > 0
  function cancelEdit() {
    if (!mutation.isPending) {
      if (dirty) discard.current?.showModal()
      else cancel()
    }
  }
  function goTo(item: EditorGroup) {
    setGroup(item)
    const section = document.getElementById(sectionPrefix + item)
    if (section instanceof HTMLDetailsElement) section.open = true
    section?.scrollIntoView({ block: 'start' })
    section?.focus({ preventScroll: true })
  }
  function addMethod(kind: MethodKind) {
    update('methods', [...input.methods, { kind, value: '', label: '',
      is_primary: !input.methods.some(method => method.kind === kind), is_verified: false }])
  }
  useEffect(() => {
    if (initialGroup === 'basic') return
    const frame = requestAnimationFrame(() => {
      const section = document.getElementById(sectionPrefix + initialGroup)
      section?.scrollIntoView({ block: 'start' })
      section?.focus({ preventScroll: true })
    })
    return () => cancelAnimationFrame(frame)
  }, [initialGroup, sectionPrefix])
  function fieldsFor(keys: Array<typeof textFields[number][0]>) {
    return keys.map(key => {
      const label = textFields.find(([field]) => field === key)![1]
      return <Field key={key} label={label}>
        {catalog.some(option => option.field === key)
          ? <StateSelect field={key} catalog={catalog} value={input[key]} change={value => update(key, value)} />
          : <input type={key === 'website_url' ? 'url' : 'text'} value={input[key]} onChange={event => update(key, event.target.value)} />}
      </Field>
    })
  }

  return <form className="contact-editor" onSubmit={event => { event.preventDefault(); mutation.mutate(false) }}
    onKeyDown={event => {
      if (event.key === 'Escape' && !(event.target as HTMLElement).closest('.confirmation')) {
        event.stopPropagation()
        event.preventDefault()
        cancelEdit()
      }
    }}
    onInvalidCapture={event => {
      const details = (event.target as HTMLElement).closest('details')
      if (details) details.open = true
    }}>
    <div className="contact-editor-status"><header className="page-heading contact-editor-heading">
      <div><h1>{record ? '编辑客户' : '新增客户'}</h1>
        <p>{record ? record.input.display_name : '先填写基本信息，联系方式和其他资料可随时补充。'}</p></div>
      <div className="row-actions">
        <button className="button" type="button" disabled={mutation.isPending} onClick={cancelEdit}>取消</button>
        <button className="button primary" type="submit" disabled={mutation.isPending}><CustomerIcon name="check" />{mutation.isPending ? '正在保存…' : '保存客户'}</button>
      </div>
    </header>
    {changed && <div className="alert warning" role="status">客户资料已更新，当前输入仍保留。
      <ConfirmButton label="放弃本次编辑并读取最新的客户资料？" onConfirm={() => {
        setOriginal(record.input)
        setInput(record.input)
        setBase({ revision: record.record_revision, epoch: record.data_epoch })
        setDuplicate([])
        mutation.reset()
      }}>重新载入客户资料</ConfirmButton>
    </div>}
    {mutation.isError && <Problem error={mutation.error} />}
    <Duplicates matches={duplicate}><button className="button" type="button" disabled={mutation.isPending} onClick={() => mutation.mutate(true)}>确认继续保存</button></Duplicates>
    </div>
    <div className="editor-workspace">
    <nav className="editor-nav" aria-label="客户表单分区">{(Object.entries(editorGroups) as Array<[EditorGroup, string]>)
      .filter(([key]) => key !== 'custom' || !record).map(([key,label]) =>
        <button type="button" key={key} aria-current={group === key ? 'location' : undefined} onClick={() => goTo(key)}>
          <CustomerIcon name={key === 'basic' ? 'contact' : key === 'channels' ? 'mail' : key === 'interests' ? 'box' : key === 'notes' ? 'note' : 'settings'} />{label}
        </button>)}</nav>
    <fieldset className="editor-fields" disabled={mutation.isPending}>
      <section className="panel" id={sectionPrefix+'basic'} tabIndex={-1}>
        <h2><CustomerIcon name="contact" />基本信息</h2>
        <div className="form-grid">
          <Field label="客户名称 *"><input required autoFocus={initialGroup === 'basic'} value={input.display_name} onChange={e => update('display_name', e.target.value)} /></Field>
          {fieldsFor(['company_name','country_region'])}
          <Field label="客户阶段"><select value={input.stage} onChange={e => update('stage', e.target.value as Stage)}>
            {Object.entries(stages).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
          </select></Field>
          {fieldsFor(['customer_level','customer_source'])}
        </div>
      </section>
      <section className="panel" id={sectionPrefix+'channels'} tabIndex={-1}>
        <div className="section-heading"><h2><CustomerIcon name="mail" />联系方式</h2></div>
        <div className="form-grid">{fieldsFor(['website_url','primary_language'])}</div>
        <div className="row-actions editor-channel-tools">
          <button className="button" type="button" disabled={input.methods.length >= 12} onClick={() => addMethod('email')}>添加邮箱</button>
          <button className="button" type="button" disabled={input.methods.length >= 12} onClick={() => addMethod('phone')}>添加电话 / 其他渠道</button>
        </div>
        {input.methods.map((method, index) => (
          <div className="form-grid collection-row" key={method.id ?? 'new-method-' + index}>
            <Field label="渠道类型"><select value={method.kind} onChange={e => updateMethod(index, { kind: e.target.value as MethodKind })}>
              {Object.entries(methodKinds).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
            </select></Field>
            <Field label="账号"><input required autoFocus={!method.id && !method.value} type={method.kind === 'email' ? 'email' : 'text'}
              value={method.value} onChange={e => updateMethod(index, { value: e.target.value, is_verified: false })} /></Field>
            <Field label="渠道备注"><input value={method.label} onChange={e => updateMethod(index, { label: e.target.value })} /></Field>
            <div className="row-actions"><label><input type="checkbox" checked={method.is_primary} onChange={e => updateMethod(index, { is_primary: e.target.checked })} />主要</label>
              <label><input type="checkbox" checked={method.is_verified} onChange={e => updateMethod(index, { is_verified: e.target.checked })} />已核实</label>
              <button type="button" className="text-button" onClick={() => update('methods', input.methods.filter((_, i) => i !== index))}>移除</button></div>
          </div>
        ))}
      </section>
      <details className="panel editor-section" id={sectionPrefix+'business'} tabIndex={-1} open={initialGroup === 'business' || undefined}>
        <summary><CustomerIcon name="settings" />业务状态<span>客户类型 · 样品 · 批量订单</span></summary>
        <div className="form-grid">
          {fieldsFor(['customer_type','first_contact_channel','sample_status','bulk_order_status','alibaba_buyer_id'])}
          <Field label="人工记录的首次联系时间"><input type="datetime-local" value={dateInput(input.first_contact_at)}
            onChange={e => update('first_contact_at', fromDateInput(e.target.value))} /></Field>
          <Field label="人工记录的最近联系时间"><input type="datetime-local" value={dateInput(input.last_contact_at)}
            onChange={e => update('last_contact_at', fromDateInput(e.target.value))} /></Field>
        </div>
      </details>
      <details className="panel editor-section" id={sectionPrefix+'interests'} tabIndex={-1} open={initialGroup === 'interests' || undefined}><summary><CustomerIcon name="box" />产品兴趣{input.interests.length > 0 && ' · ' + input.interests.length}</summary>
        <div className="section-heading"><button className="button" type="button" disabled={input.interests.length >= 20}
          onClick={() => update('interests', [...input.interests, { product_text: '', intent_quantity: null, quantity_is_estimate: false, requirement: '' }])}>添加产品</button></div>
        {input.interests.map((interest, index) => (
          <div className="form-grid collection-row" key={interest.id ?? 'new-interest-' + index}>
            <Field label="型号或品类"><input required value={interest.product_text} onChange={e => updateInterest(index, { product_text: e.target.value })} /></Field>
            <Field label="意向数量"><input type="number" min="0" max="9000000000000000" step="1" value={interest.intent_quantity ?? ''}
              onChange={e => updateInterest(index, { intent_quantity: e.target.value ? e.target.valueAsNumber : null })} /></Field>
            <Field label="采购要求"><input value={interest.requirement} onChange={e => updateInterest(index, { requirement: e.target.value })} /></Field>
            <div className="row-actions"><label><input type="checkbox" checked={interest.quantity_is_estimate}
              onChange={e => updateInterest(index, { quantity_is_estimate: e.target.checked })} />估算</label>
              <button className="text-button" type="button" onClick={() => update('interests', input.interests.filter((_, i) => i !== index))}>移除</button></div>
          </div>
        ))}
      </details>
      {!record && <div id={sectionPrefix+'custom'} tabIndex={-1}><NewCustomerFields values={custom}
        change={(id, value) => setCustom(old => ({ ...old, [id]: value }))} /></div>}
      <details className="panel editor-section" id={sectionPrefix+'notes'} tabIndex={-1} open={initialGroup === 'notes' || undefined}>
        <summary><CustomerIcon name="note" />备注与需求</summary><div className="form-grid">
          <Field label="主要需求"><textarea rows={3} value={input.main_need} onChange={e => update('main_need', e.target.value)} /></Field>
          <Field label="备注"><textarea rows={3} value={input.notes} onChange={e => update('notes', e.target.value)} /></Field>
        </div>
      </details>
    </fieldset>
    <aside className="editor-guide" aria-label="填写提示">
      <h2>填写建议</h2>
      <p>以联系人为主记录，公司和店铺资料可以稍后补充。</p>
      <h2>保存时检查</h2>
      <ul><li>姓名、邮箱与网址格式</li><li>现有客户与候选客户中的可能重复项</li></ul>
    </aside>
    </div>
    <dialog ref={discard} className="confirmation" aria-label="放弃客户编辑">
      <p>放弃本次修改并返回？</p><div className="form-footer">
        <button className="button" type="button" onClick={() => discard.current?.close()}>继续编辑</button>
        <button className="button" type="button" onClick={cancel}>放弃修改</button>
      </div>
    </dialog>
  </form>
}

function CustomerEditorDialog({ record, group, close }: { record: Contact; group: EditorGroup; close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => { dialog.current?.showModal() }, [])
  return <dialog className="customer-editor-dialog" ref={dialog} aria-label="编辑客户" onCancel={event => event.preventDefault()}>
    <ContactEditor record={record} initialGroup={group} cancel={close} saved={close} />
  </dialog>
}

const detailSections = { overview: '基本资料', channels: '联系渠道', interests: '采购需求', activity: '沟通时间线', mail: '邮件往来' } as const

export function ContactDetail({ id, entry, back, compose, history, drawer }: {
  id: string; back: () => void; compose: (contact: Contact) => void; history: (id: string) => void; drawer?: boolean; entry?: 'edit' | 'mail'
}) {
  const [editing, setEditing] = useState<EditorGroup | null>(entry === 'edit' ? 'basic' : null)
  const [section, setSection] = useState<keyof typeof detailSections>(entry === 'mail' ? 'mail' : 'overview')
  const [copy, setCopy] = useState('')
  const catalog = useStateCatalog()
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['contact', id], queryFn: () => request({ action: 'get_contact', id }, 'contact') })
  const recent = useQuery({
    queryKey: ['activities', id, '', 0],
    enabled: Boolean(drawer && section === 'overview' && !editing),
    queryFn: () => request({ action: 'list_activities', contact_id: id, event_type: '', offset: 0 }, 'activities'),
  })
  const archive = useMutation({
    mutationFn: (archived: boolean) => request({ action: 'set_archived', id, archived }, 'saved'),
    onSuccess: () => Promise.all([
      cache.invalidateQueries({ queryKey: ['contacts'] }), cache.invalidateQueries({ queryKey: ['contact', id] }),
    ]),
  })
  if (!query.data) return <>{!drawer && <button className="button" onClick={back}>返回客户列表</button>}
    {query.isError ? <Problem error={query.error} /> : <Empty>正在读取客户…</Empty>}</>
  const record = query.data.record
  const { input } = record
  const archived = record.archived_at !== null
  return <>
    {editing && !archived && <CustomerEditorDialog record={record} group={editing} close={() => setEditing(null)} />}
    {!drawer && <button className="text-button back-link" onClick={back}>返回客户列表</button>}
    <div className="page-heading customer-profile-heading">
      <div className="customer-identity">
        <span className="customer-avatar" aria-hidden="true">{Array.from(input.display_name)[0]?.toUpperCase()}</span>
        <div><h1>{input.display_name}</h1>
          <p>{[input.company_name, input.country_region, archived ? '已归档' : ''].filter(Boolean).join(' · ')}</p>
          <span className="stage-tag" data-stage={input.stage}>{stages[input.stage]}</span>
          {input.customer_level && <span className="customer-status" data-status="level">{stateLabel(catalog,'customer_level',input.customer_level)}</span>}
        </div>
      </div>
      <div className="row-actions">
        {!archived && input.methods.some(method => method.kind === 'email') && <button className="button primary" onClick={() => compose(record)}><CustomerIcon name="mail" />写邮件</button>}
        <button className="button" onClick={() => setSection('activity')}><CustomerIcon name="clock" />跟进记录</button>
        <ConfirmButton label={archived ? '恢复这个客户？' : '归档这个客户？已有跟进仍可查看。'}
          disabled={archive.isPending} onConfirm={() => archive.mutate(!archived)}>{archived ? '恢复客户' : '归档客户'}</ConfirmButton>
      </div>
    </div>
    {archive.isError && <Problem error={archive.error} />}
    <nav className="detail-sections" aria-label="客户详情分区">{Object.entries(detailSections).map(([key,label]) =>
      <button className="text-button" type="button" key={key} aria-pressed={section===key}
        onClick={() => setSection(key as keyof typeof detailSections)}>{label}</button>)}
    </nav>
    <div hidden={section!=='overview'}>
    <section className="panel customer-properties"><div className="section-heading"><h2><CustomerIcon name="contact" />基本资料</h2>
      {!archived && <button className="text-button customer-section-edit" onClick={() => setEditing('basic')}><CustomerIcon name="edit" />编辑</button>}</div>
      <dl className="profile-grid">
        <div><dt>客户名称</dt><dd>{input.display_name}</dd></div>
        {textFields.map(([key, label]) => input[key] && <div key={key}><dt>{label}</dt><dd>{stateLabel(catalog, key, input[key])}</dd></div>)}
        <div><dt>首次联系</dt><dd>{dateLabel(record.effective_first_contact_at)}</dd></div>
        <div><dt>最近联系</dt><dd>{dateLabel(record.effective_last_contact_at)}</dd></div>
        <div><dt>最近会话来信</dt><dd>{dateLabel(record.latest_thread_reply_at)}</dd></div>
        <div><dt>资料更新</dt><dd>{dateLabel(record.updated_at)}</dd></div>
        <div><dt>记录来源</dt><dd>{sourceLabel(record.source)}</dd></div>
        {input.notes && <div><dt>备注</dt><dd>{input.notes}</dd></div>}
      </dl>
    </section>
      {drawer && input.main_need && <section className="panel customer-detail-summary">
        <h2><CustomerIcon name="note" />主要需求</h2><p className="long-text customer-needs">{input.main_need}</p>
      </section>}
      {drawer && input.interests.length > 0 && <section className="panel customer-detail-summary">
        <div className="section-heading"><h2><CustomerIcon name="box" />产品兴趣</h2><button className="text-button" onClick={() => setSection('interests')}>全部产品</button></div>
        <ul className="customer-interest-cards">{input.interests.slice(0, 3).map(interest => <li key={interest.id}><CustomerIcon name="box" /><strong>{interest.product_text}</strong>
          {interest.intent_quantity !== null && <span>{interest.quantity_is_estimate ? '估计 ' : ''}{interest.intent_quantity.toLocaleString('zh-CN')} 件</span>}
        </li>)}</ul>
      </section>}
      {drawer && recent.data?.rows.length ? <section className="panel customer-detail-summary">
        <div className="section-heading"><h2><CustomerIcon name="clock" />最近跟进</h2><button className="text-button" onClick={() => setSection('activity')}>查看全部</button></div>
        <ul className="customer-recent-activity">{recent.data.rows.slice(0, 3).map(activity => <li key={activity.id}>
          <small>{dateLabel(activity.occurred_at)} · {sourceLabel(activity.source)}</small><strong>{activity.summary}</strong>
        </li>)}</ul>
      </section> : null}
      <CustomerFieldPanel contactId={record.id} archived={archived} />
    </div>
    {section==='channels' && <section className="panel"><div className="section-heading"><h2><CustomerIcon name="mail" />联系渠道</h2>
      {!archived && <button className="text-button customer-section-edit" onClick={() => setEditing('channels')}><CustomerIcon name="edit" />编辑</button>}</div>
      {copy && <p className="muted" role="status">{copy}</p>}
      {input.methods.length ? <ul className="record-list contact-channels">{input.methods.map(method =>
        <li key={method.id}><div><strong>{methodKinds[method.kind]}</strong><p>{method.value}</p>
          {(method.label || method.is_primary || method.is_verified) && <small>{[method.label, method.is_primary ? '主要' : '', method.is_verified ? '已核实' : ''].filter(Boolean).join(' · ')}</small>}</div>
          <button className="button" onClick={() => {
            void navigator.clipboard.writeText(method.value).then(() => setCopy('已复制联系方式'), () => setCopy('无法写入剪贴板'))
          }}>复制</button></li>)}</ul> : <p className="muted">尚无联系渠道</p>}
    </section>}
    {section==='interests' && <>
      {input.main_need && <section className="panel"><h2>采购需求</h2><p className="long-text">{input.main_need}</p></section>}
      <section className="panel"><div className="section-heading"><h2><CustomerIcon name="box" />产品兴趣</h2>
        {!archived && <button className="text-button customer-section-edit" onClick={() => setEditing('interests')}><CustomerIcon name="edit" />编辑</button>}</div>{input.interests.length ? <ul className="record-list interest-list">{input.interests.map(interest =>
        <li key={interest.id}><strong>{interest.product_text}</strong>{interest.intent_quantity !== null &&
          <span>{interest.quantity_is_estimate ? '估计数量 ' : '意向数量 '}{interest.intent_quantity.toLocaleString()}</span>}
          {interest.requirement && <p className="long-text">{interest.requirement}</p>}</li>)}</ul>
        : <p className="muted">尚无产品兴趣</p>}</section>
    </>}
    <div hidden={section!=='activity'}>{section==='activity' && <Timeline contact={record} />}</div>
    {section==='mail' && <CustomerMail contactId={id} outreach={record.mail_outreach} open={() => history(id)} />}
  </>
}

function CustomerMail({ contactId, outreach, open }: { contactId: string; outreach: Outreach; open: () => void }) {
  const { data, isPending, error } = useQuery({
    queryKey: ['mail-drafts', contactId, 0],
    queryFn: () => mailCall({ action: 'list_drafts', contact_id: contactId, offset: 0 }, 'drafts'),
  })
  return <section className="panel"><div className="section-heading">
    <h2>发件记录</h2><button className="button" onClick={open}>查看来信与会话</button>
  </div>
    <dl className="profile-grid">
      <div><dt>开发状态</dt><dd><span className="customer-status" data-status={outreach.status}>{outreachLabels[outreach.status]}</span></dd></div>
      <div><dt>最近发信</dt><dd>{dateLabel(outreach.last_sent_at)}</dd></div>
      <div><dt>最近回复</dt><dd>{dateLabel(outreach.last_reply_at)}</dd></div>
      <div><dt>开发提交数</dt><dd>{outreach.outreach_count}</dd></div>
      {outreach.sync_at !== null && <div><dt>{outreach.sync_outcome==='ok' ? '上次同步完成' : '上次同步尝试'}</dt><dd>{dateLabel(outreach.sync_at)}</dd></div>}
    </dl>
    {isPending && <p className="muted">正在读取邮件…</p>}
    {error && <p className="alert error" role="alert">{mailError(error)}</p>}
    {data && (data.rows.length ? <ul className="record-list customer-mail-list">
      {data.rows.map(row => <li key={row.id}><strong>{row.subject}</strong>
        <small>{row.recipient} · {dateLabel(row.updated_at)}{row.agent_name ? ' · ' + row.agent_name : ''}</small>
        <span className="customer-status" data-status={row.state}>{row.approval_expired ? '批准已过期' : mailStates[row.state]}</span>
      </li>)}</ul> : <p className="muted">尚无发件记录</p>)}
  </section>
}

function useStateCatalog() {
  return useQuery({ queryKey: ['crm-states'], queryFn: () => request({ action: 'state_catalog' }, 'state_catalog'), staleTime: Infinity }).data?.options ?? []
}

function findState(catalog: StateOption[], field: string, value: string) {
  const normalized = value.trim().toLowerCase()
  const code = field === 'customer_level' && normalized.startsWith('a |') ? 'a' : normalized
  return catalog.find(option => option.field === field &&
    (option.code.toLowerCase() === code || option.aliases.includes(code)))
}

function stateLabel(catalog: StateOption[], field: string, value: string) {
  return findState(catalog, field, value)?.label ?? (value || '—')
}

function StateSelect({ field, catalog, value, change }: {
  field: string; catalog: StateOption[]; value: string; change: (value: string) => void
}) {
  const option = findState(catalog, field, value)
  return <select value={option?.code ?? value} onChange={e => change(e.target.value)}>
    <option value="">未设置</option>
    {value && !option && <option value={value}>历史值 · {value}</option>}
    {catalog.filter(item => item.field === field).map(item => <option key={item.code} value={item.code}>{item.label}</option>)}
  </select>
}

function StateFilter({ field, catalog, value, change }: {
  field: string; catalog: StateOption[]; value: string; change: (value: string) => void
}) {
  return <>
    <input list={field + '-filter'} value={value} onChange={e => change(e.target.value)} />
    <datalist id={field + '-filter'}>{catalog.filter(item => item.field === field).map(item =>
      <option key={item.code} value={item.label}>{item.label}</option>)}</datalist>
  </>
}

function isFiltered(query: ContactQuery) {
  return Boolean(query.keyword || query.quick_view || query.country || query.stage || query.level ||
    query.customer_type || query.source || query.sample_status || query.archive !== 'active')
}
