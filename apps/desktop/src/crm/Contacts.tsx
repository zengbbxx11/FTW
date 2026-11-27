import { lazy, Suspense, useDeferredValue, useEffect, useId, useRef, useState, type CSSProperties } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateInput, dateLabel, fromDateInput, methodKinds, outreachLabels, request, sourceLabel, stages,
  type Contact, type ContactInput, type ContactQuery, type Duplicate, type Interest, type Method, type MethodKind, type Outreach, type Stage, type StateOption, type ContactRow } from './api'
import { ConfirmButton, Duplicates, Empty, Field, Pagination, Problem } from './components'
import { Timeline } from './Timeline'
import { mailCall, mailError, states as mailStates } from '../mail/api'
import { CustomerFieldPanel, FieldManager, NewCustomerFields, useCustomerFields } from './Fields'
import { fieldEntries, type FieldValue } from './fieldTypes'
import CustomerTable, { CustomerCellEditor, targetFor, type BulkSelection } from './CustomerTable'
import { ColumnSettings, SavedViewBar, useSavedViews } from './SavedViews'
import { availableColumns, columnsWithDefaults, initialColumnsForView, systemColumns, type AvailableColumn, type CellTarget, type ViewConfig } from './viewTypes'
import { ActionMenu, IconButton, MenuAction, ToolPopover } from '../ui/WorkbenchUI'

import { CustomerIcon } from './CustomerIcon'
import { LeadEvidencePanel } from './LeadEvidence'
import { ContactHistory } from './ContactHistory'

const wideDrawer = '(min-width: 1440px)'
const AdvancedFilters = lazy(() => import('./AdvancedFilters'))

const defaultQuery: ContactQuery = {
  keyword: '', quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '', archive: 'active', sort: 'updated', offset: 0,
}
const contactSorts = { updated: '最近更新', name: '客户名称', last_contact: '最近联系' } as const

export function ContactList({ compose, history, files, company, create, importCsv, quality, samples, initialView }: {
  compose: (contact: Contact) => void; history: (id: string) => void; files: (id: string) => void; create: () => void
  company: (id: string) => void; importCsv: () => void; quality: (duplicates?: boolean) => void; samples: () => void; initialView?: string
}) {
  const catalog = useStateCatalog()
  const fields = useCustomerFields()
  const views = useSavedViews()
  const records = views.data?.views ?? []
  const [activeId,setActiveId] = useState(initialView ?? '')
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
  const bulkQuery: ContactQuery = { ...requestQuery, offset: 0, order: [], custom_columns: [], page_size: undefined, sort: 'updated' }
  const bulkKey = (active?.id ?? '') + JSON.stringify(bulkQuery)
  const liveBulkKey = useRef(bulkKey)
  useEffect(() => { liveBulkKey.current = bulkKey }, [bulkKey])
  const [bulkSelection, setBulkSelection] = useState<{ key: string; data: BulkSelection }>({ key: '', data: { targets: {} } })
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
    if (Object.keys(change).some(key => !['order', 'sort', 'page_size'].includes(key))) {
      setBulkSelection({ key: '', data: { targets: {} } })
    }
    const next = { ...config.query, ...change }
    configure({ query:next })
  }
  function select(id: string) {
    setBulkSelection({ key: '', data: { targets: {} } })
    setActiveId(id)
    setLocal(null)
    setOffset(0)
    setShowAdvanced(false)
  }
  function manageColumnFields() {
    close()
    setSelection(null)
    setManageFields(true)
  }
  const result = useQuery({
    queryKey: ['contacts', requestQuery], enabled: !board && fields.isFetched && (!initialView || !views.isPending),
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
    <div className="customer-command">
    <header className="page-heading customer-heading">
      <div><h1>客户管理</h1><p>
        {overview.data && <><strong>{overview.data.active.toLocaleString('zh-CN')}</strong> 位客户 · </>}
        {active?.id === 'mail_no_reply' ? '按最新 SMTP / IMAP 记录计算' : '管理客户资料、样品进度与邮件往来'}
      </p></div>
      <div className="row-actions">
        <button className="button" onClick={importCsv}><CustomerIcon name="upload" />导入 CSV</button>
        <button className="button primary" onClick={create}><CustomerIcon name="plus" />新增客户</button>
        <ActionMenu trigger={<IconButton label="更多客户操作"><CustomerIcon name="more" /></IconButton>}>
          <MenuAction onSelect={samples}><CustomerIcon name="box" />样品进度跟踪</MenuAction>
          <MenuAction onSelect={() => quality(true)}><CustomerIcon name="users" />客户查重与合并</MenuAction>
          <MenuAction onSelect={() => quality()}><CustomerIcon name="shield" />客户数据质量</MenuAction>
        </ActionMenu>
      </div>
    </header>
    {views.isError && <Problem error={views.error} />}
    {fields.isError && <Problem error={fields.error} />}
    <SavedViewBar views={records} active={active} config={config} baseRevision={local?.id === active?.id ? local?.revision ?? null : null} counts={overview.data} select={select} saved={select} />
    <div className="customer-toolbar">
      <div className="customer-search">
        <span className="search-symbol"><CustomerIcon name="search" /></span>
        <input type="search" aria-label="搜索客户" ref={search} placeholder="搜索姓名、公司、邮箱或产品" value={query.keyword}
          onChange={event => update({keyword:event.target.value})} />
        <kbd aria-hidden="true">Ctrl K</kbd>
      </div>
      <ToolPopover label="筛选客户" trigger={<button className="button" type="button" data-filtered={isFiltered({ ...query, keyword: '' }) || Boolean(query.filter)}>
        <CustomerIcon name="filter" />筛选<CustomerIcon name="expand" />
      </button>}>
        <h2 className="ftw-popup-heading">筛选客户</h2>
        <div className="form-grid">
          <Field label="客户阶段"><select value={query.stage} onChange={event => update({stage:event.target.value})}>
            <option value="">全部阶段</option>{Object.entries(stages).map(([key,label]) => <option key={key} value={key}>{label}</option>)}
          </select></Field>
          <Field label="国家或地区"><input value={query.country} onChange={event=>update({country:event.target.value})} /></Field>
          <Field label="客户等级"><StateFilter field="customer_level" catalog={catalog} value={query.level} change={level=>update({level})} /></Field>
          <Field label="样品进度"><StateFilter field="sample_status" catalog={catalog} value={query.sample_status ?? ''} change={sample_status=>update({sample_status})} /></Field>
          <Field label="客户类型"><input value={query.customer_type} onChange={event=>update({customer_type:event.target.value})} /></Field>
          <Field label="客户来源"><input value={query.source} onChange={event=>update({source:event.target.value})} /></Field>
          <Field label="归档状态"><select value={query.archive} onChange={event=>update({archive:event.target.value as ContactQuery['archive']})}>
            <option value="active">未归档</option><option value="archived">已归档</option><option value="all">全部</option>
          </select></Field>
        </div>
        <div className="ftw-popup-footer">
        <button className="button" type="button" onClick={() => update({
          quick_view: '', country: '', stage: '', level: '', customer_type: '', source: '', archive: 'active', sample_status: '', filter: undefined,
        })}>
            <CustomerIcon name="restore" />清除筛选
          </button>
        </div>
      </ToolPopover>
      <ActionMenu trigger={<button type="button" className="button" title={query.order?.length ? '按表格列排序' : contactSorts[query.sort]}>
        <CustomerIcon name="sort" />排序<CustomerIcon name="expand" />
      </button>}>
        {(Object.keys(contactSorts) as (keyof typeof contactSorts)[]).map(sort =>
          <MenuAction key={sort} onSelect={() => update({sort,order:[]})}>
            {sort === query.sort && !query.order?.length && <CustomerIcon name="check" />}{contactSorts[sort]}
          </MenuAction>)}
        {Boolean(query.order?.length) && <MenuAction disabled><CustomerIcon name="check" />按表格列排序</MenuAction>}
      </ActionMenu>
      <button className="button" onClick={() => setShowAdvanced(value=>!value)} aria-expanded={showAdvanced} data-filtered={Boolean(query.filter)}>
        <CustomerIcon name="sliders" />高级筛选
      </button>
      {!board && <ColumnSettings available={available} saved={columns} change={items=>configure({columns:items},false)}
        density={config.density ?? 'standard'} setDensity={density => configure({density},false)}
        manageFields={manageColumnFields} />}
      {board && <button className="button" onClick={manageColumnFields}><CustomerIcon name="settings" />字段管理</button>}
      <div className="layout-switch" role="group" aria-label="客户视图布局">
        <IconButton label="表格视图" aria-pressed={!board} onClick={() => configure({ layout: 'table' })}><CustomerIcon name="grid" /></IconButton>
        <IconButton label="看板视图" aria-pressed={board} onClick={() => configure({ layout: 'board' })}><CustomerIcon name="board" /></IconButton>
      </div>
      {board && <Field label="分组字段"><select value={config.group_field ?? 'stage'} onChange={event => configure({group_field:event.target.value})}>
        <option value="stage">客户阶段</option>
        {(fields.data?.fields ?? []).filter(field => field.kind === 'single_select').map(field => <option key={field.id} value={field.id}>{field.name}</option>)}
      </select></Field>}
    </div>
    {active?.id === 'mail_no_reply' && <p className="customer-mail-context">
      <CustomerIcon name="clock" />仅显示最新发信后已完成 IMAP 同步、暂未查到已关联客户回复的记录。
    </p>}
    {showAdvanced && <Suspense fallback={<Empty>正在加载组合筛选…</Empty>}><AdvancedFilters key={active?.id}
      columns={available} catalog={catalog} filter={query.filter} close={()=>setShowAdvanced(false)}
      query={requestQuery} apply={filter=>{update({filter});setShowAdvanced(false)}} /></Suspense>}
    {overview.data && <div className="customer-overview" aria-label="全部有效客户统计">
      {([
        ['active', '全部客户', 'users'], ['priority', '重点客户', 'star'], ['sample', '样品客户', 'box'],
        ['in_progress', '跟进中', 'growth'],
      ] as const).map(([key, label, symbol]) => <div className="customer-metric" key={key} data-metric={key}>
        <span className="customer-metric-icon"><CustomerIcon name={symbol} /></span>
        <div><strong>{overview.data![key].toLocaleString('zh-CN')}</strong><span>{label}</span></div>
      </div>)}
    </div>}
    </div>
    <section className="customer-data" aria-label={board ? '客户看板' : '客户表格'}>
    {board ? <ContactBoard query={requestQuery} group={config.group_field ?? 'stage'} available={available} open={open} /> : <>
      {result.isPending && <div className="customer-loading" role="status">
        <span>正在读取客户…</span>
        <div aria-hidden="true">{[0, 1, 2, 3].map(line => <div key={line} />)}</div>
      </div>}
      {result.isError && <Problem error={result.error} />}
      {result.data && <>
        {result.data.rows.length === 0 ? <Empty>{isFiltered(query) || query.filter ? <>
          <p>没有符合筛选条件的客户。</p><button className="button" onClick={()=>configure({query:{...defaultQuery}})}>
            <CustomerIcon name="restore" />清除筛选
          </button>
        </> : <><p>当前没有客户记录。</p><div className="row-actions">
          <button className="button primary" onClick={create}><CustomerIcon name="plus" />新增客户</button>
          <button className="button" onClick={importCsv}><CustomerIcon name="upload" />导入客户 CSV</button>
        </div></>}</Empty> : <CustomerTable key={(active?.id ?? '') + JSON.stringify(requestQuery)} data={result.data.rows} available={available}
          bulkQuery={bulkQuery} selection={bulkSelection.key === bulkKey ? bulkSelection.data : { targets: {} }}
          setSelection={data => { if (liveBulkKey.current === bulkKey) setBulkSelection({ key: bulkKey, data }) }} filtering={requestQuery.keyword !== query.keyword}
          columns={columns} catalog={catalog} density={config.density ?? 'standard'} open={open} activeRow={selected} order={query.order?.[0]}
          editCustomer={id => open(id, 'edit')} showMail={id => open(id, 'mail')} emailPreview={Boolean(active?.id.startsWith('mail_'))}
          resize={widths=>configure({columns:columns.map(column=>({...column,width:widths[column.field]??column.width}))},false)}
          sort={field=>update({order:[{field,descending:query.order?.[0]?.field===field && !query.order[0].descending}]})} />}
        <div className="customer-list-footer">
          <span>{result.data.rows.length > 0 ? `第 ${offset + 1}–${offset + result.data.rows.length} 条` : '本页无记录'}</span>
          <label className="customer-page-size">每页 <select aria-label="每页客户数" value={query.page_size ?? 20}
            onChange={event => update({page_size:Number(event.target.value) as 20|50|100})}>
            <option value="20">20</option><option value="50">50</option><option value="100">100</option>
          </select> 条</label>
          <div className="customer-pagination" role="group" aria-label="客户分页">
            <IconButton label="上一页" disabled={!offset} onClick={() => setOffset(Math.max(0, offset - (query.page_size ?? 20)))}>
              <CustomerIcon name="back" />
            </IconButton>
            <span aria-current="page">第 {Math.floor(offset / (query.page_size ?? 20)) + 1} 页</span>
            <IconButton label="下一页" disabled={!result.data.has_more} onClick={() => setOffset(offset + (query.page_size ?? 20))}>
              <CustomerIcon name="arrow" />
            </IconButton>
          </div>
        </div>
      </>}
    </>}
    </section>
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
          {!board && <><IconButton label="上一位客户" disabled={selectedIndex < 1}
            onClick={() => open(pageRows[selectedIndex - 1].id)}><CustomerIcon name="back" /></IconButton>
            <IconButton label="下一位客户" disabled={selectedIndex < 0 || selectedIndex >= pageRows.length - 1}
              onClick={() => open(pageRows[selectedIndex + 1].id)}><CustomerIcon name="arrow" /></IconButton></>}
          <IconButton autoFocus label="关闭详情" onClick={close}><CustomerIcon name="close" /></IconButton>
        </div>
      </header>
      <div className="contact-drawer-body" role="region" aria-label="客户资料和跟进" tabIndex={0}>
        {selection && <ContactDetail key={selection.id + selection.sequence} id={selection.id} entry={selection.entry} back={close} openRelated={open} compose={compose} history={history} files={files} company={company} drawer />}
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

function revealEditorSection(id: string) {
  const section = document.getElementById(id)
  if (!section) return
  if (section instanceof HTMLDetailsElement) section.open = true
  section.scrollIntoView({ block: 'start' })
  section.focus({ preventScroll: true })
}

export type CompanyIdentity = Pick<ContactInput, 'company_name' | 'country_region'>

export function ContactEditor({ record, saved, cancel, initialGroup = 'basic', initialCompany, companyProfile }: {
  record?: Contact; saved: (id: string) => void; cancel: () => void; initialGroup?: EditorGroup; initialCompany?: CompanyIdentity
  companyProfile?: CellTarget
}) {
  const [input, setInput] = useState(record?.input ?? { ...blank, ...initialCompany })
  const [original, setOriginal] = useState(input)
  const [group, setGroup] = useState<EditorGroup>(initialGroup)
  const [managingFields, setManagingFields] = useState(false)
  const header = useRef<HTMLDivElement>(null)
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
      : { action: 'create_contact_with_fields', input, allow_duplicate: allow, entries: fieldEntries(custom), company: companyProfile }, 'saved', 'duplicates'),
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
    revealEditorSection(sectionPrefix + item)
  }
  function addMethod(kind: MethodKind) {
    update('methods', [...input.methods, { kind, value: '', label: '',
      is_primary: !input.methods.some(method => method.kind === kind), is_verified: false }])
  }
  useEffect(() => {
    const bar = header.current
    const editor = bar?.parentElement
    if (!bar || !editor) return
    const measure = () => editor.style.setProperty('--editor-header-height', bar.offsetHeight + 'px')
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(bar)
    return () => observer.disconnect()
  }, [])
  useEffect(() => {
    if (initialGroup === 'basic') return
    const frame = requestAnimationFrame(() => revealEditorSection(sectionPrefix + initialGroup))
    return () => cancelAnimationFrame(frame)
  }, [initialGroup, sectionPrefix])
  function fieldsFor(keys: Array<typeof textFields[number][0]>) {
    return keys.map(key => {
      const label = textFields.find(([field]) => field === key)![1]
      return <Field key={key} label={label}>
        {initialCompany && !companyProfile && (key === 'company_name' || key === 'country_region') ? <input readOnly value={input[key]} /> : catalog.some(option => option.field === key)
          ? <StateSelect field={key} catalog={catalog} value={input[key]} change={value => update(key, value)} />
          : <input type={key === 'website_url' ? 'url' : 'text'} value={input[key]} onChange={event => update(key, event.target.value)} />}
      </Field>
    })
  }

  return <>
  <form className="contact-editor" onSubmit={event => { event.preventDefault(); mutation.mutate(false) }}
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
    <div ref={header} className="contact-editor-status"><header className="page-heading contact-editor-heading">
      <div><h1>{record ? '编辑客户' : '新增客户'}</h1>
        <p>{record ? record.input.display_name : '先填写基本信息，联系方式和其他资料可随时补充。'}</p></div>
      <div className="row-actions">
        <button className="button" type="button" disabled={mutation.isPending} onClick={cancelEdit}><CustomerIcon name="close" />取消</button>
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
        <button type="button" key={key} aria-controls={sectionPrefix + key}
          aria-current={group === key ? 'location' : undefined} onClick={() => goTo(key)}>
          <CustomerIcon name={key === 'basic' ? 'contact' : key === 'channels' ? 'mail' : key === 'interests' ? 'box' : key === 'notes' ? 'note' : 'settings'} />{label}
        </button>)}</nav>
    <fieldset className="editor-fields" disabled={mutation.isPending}
      onFocusCapture={event => {
        const section = (event.target as HTMLElement).closest<HTMLElement>('[data-editor-group]')
        if (section) setGroup(section.dataset.editorGroup as EditorGroup)
      }}>
      <section className="panel" id={sectionPrefix+'basic'} data-editor-group="basic" data-active={group === 'basic'} tabIndex={-1}>
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
      <section className="panel" id={sectionPrefix+'channels'} data-editor-group="channels" data-active={group === 'channels'} tabIndex={-1}>
        <div className="section-heading"><h2><CustomerIcon name="mail" />联系方式</h2></div>
        <div className="form-grid">{fieldsFor(['website_url','primary_language'])}</div>
        <div className="row-actions editor-channel-tools">
          <button className="button" type="button" disabled={input.methods.length >= 12} onClick={() => addMethod('email')}><CustomerIcon name="mail" />添加邮箱</button>
          <button className="button" type="button" disabled={input.methods.length >= 12} onClick={() => addMethod('phone')}><CustomerIcon name="plus" />添加电话 / 其他渠道</button>
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
      <details className="panel editor-section" id={sectionPrefix+'business'} data-editor-group="business" data-active={group === 'business'} tabIndex={-1}>
        <summary><CustomerIcon name="settings" />业务状态<span>客户类型 · 样品 · 批量订单</span></summary>
        <div className="form-grid">
          {fieldsFor(['customer_type','first_contact_channel','sample_status','bulk_order_status','alibaba_buyer_id'])}
          <Field label="人工记录的首次联系时间"><input type="datetime-local" value={dateInput(input.first_contact_at)}
            onChange={e => update('first_contact_at', fromDateInput(e.target.value))} /></Field>
          <Field label="人工记录的最近联系时间"><input type="datetime-local" value={dateInput(input.last_contact_at)}
            onChange={e => update('last_contact_at', fromDateInput(e.target.value))} /></Field>
        </div>
      </details>
      <details className="panel editor-section" id={sectionPrefix+'interests'} data-editor-group="interests" data-active={group === 'interests'} tabIndex={-1}><summary><CustomerIcon name="box" />产品兴趣{input.interests.length > 0 && ' · ' + input.interests.length}</summary>
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
      {!record && <details className="panel editor-section" id={sectionPrefix+'custom'} data-editor-group="custom"
        data-active={group === 'custom'} tabIndex={-1}>
        <summary><CustomerIcon name="settings" />自定义属性</summary>
        <NewCustomerFields values={custom} manage={() => setManagingFields(true)}
          change={(id, value) => setCustom(old => ({ ...old, [id]: value }))} />
      </details>}
      <details className="panel editor-section" id={sectionPrefix+'notes'} data-editor-group="notes" data-active={group === 'notes'} tabIndex={-1}>
        <summary><CustomerIcon name="note" />备注与需求</summary><div className="form-grid">
          <Field label="主要需求"><textarea rows={3} value={input.main_need} onChange={e => update('main_need', e.target.value)} /></Field>
          <Field label="备注"><textarea rows={3} value={input.notes} onChange={e => update('notes', e.target.value)} /></Field>
        </div>
      </details>
    </fieldset>
    <aside className="editor-guide" aria-label="填写提示">
      <h2>填写进度</h2>
      <ul className="editor-progress">
        <li data-complete={Boolean(input.display_name.trim())}><CustomerIcon name={input.display_name.trim() ? 'checkCircle' : 'contact'} />{input.display_name.trim() ? '已有客户名称' : '待填写客户名称'}</li>
        <li data-complete={input.methods.some(method => method.value.trim())}><CustomerIcon name="mail" />{input.methods.filter(method => method.value.trim()).length} 个联系渠道</li>
        <li data-complete={input.interests.some(interest => interest.product_text.trim())}><CustomerIcon name="box" />{input.interests.filter(interest => interest.product_text.trim()).length} 项产品兴趣</li>
      </ul>
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
  {managingFields && <NewCustomerFieldManager close={() => { setManagingFields(false); goTo('custom') }} />}
  </>
}

function NewCustomerFieldManager({ close }: { close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => { dialog.current?.showModal() }, [])
  return <dialog ref={dialog} className="customer-editor-dialog" aria-label="客户字段" onClose={close}>
    <FieldManager back={() => dialog.current?.close()} backLabel="返回新增客户" />
  </dialog>
}

function CustomerEditorDialog({ record, group, close }: { record: Contact; group: EditorGroup; close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => { dialog.current?.showModal() }, [])
  return <dialog className="customer-editor-dialog" ref={dialog} aria-label="编辑客户" onCancel={event => event.preventDefault()}>
    <ContactEditor record={record} initialGroup={group} cancel={close} saved={close} />
  </dialog>
}

const detailSections = { overview: '基本资料', channels: '联系渠道', interests: '采购需求', activity: '跟进记录', mail: '邮件往来', evidence: '调查证据', changes: '变更历史' } as const
const detailIcons = { overview: 'contact', channels: 'mail', interests: 'box', activity: 'activity', mail: 'send', evidence: 'note', changes: 'clock' } as const
const quickFields = [
  ['company', 'company_name'], ['country', 'country_region'], ['stage', 'stage'],
  ['level', 'customer_level'], ['sample', 'sample_status'],
] as const

function outreachSyncLabel(outreach: Outreach) {
  if (outreach.last_sent_at === null) return '暂无关联发信'
  if (outreach.sync_at === null) return '尚未同步'
  const time = dateLabel(outreach.sync_at)
  if (outreach.sync_outcome !== 'ok') return `同步未完成 · ${time}`
  return outreach.sync_at > outreach.last_sent_at ? `已同步 · ${time}` : `同步早于最近发信 · ${time}`
}

export function ContactDetail({ id, entry, back, openRelated, compose, history, files, company, drawer }: {
  id: string; back: () => void; openRelated: (id: string) => void; compose: (contact: Contact) => void; history: (id: string) => void
  files: (id: string) => void; company: (id: string) => void; drawer?: boolean; entry?: 'edit' | 'edit-channels' | 'edit-business' | 'mail'
}) {
  const [editing, setEditing] = useState<EditorGroup | null>(entry === 'edit' ? 'basic' : entry === 'edit-channels' ? 'channels' : entry === 'edit-business' ? 'business' : null)
  const [section, setSection] = useState<keyof typeof detailSections>(entry === 'mail' ? 'mail' : 'overview')
  const [copy, setCopy] = useState('')
  const [showEmpty, setShowEmpty] = useState(false)
  const [startFollowup, setStartFollowup] = useState(false)
  const [propertyEditor, setPropertyEditor] = useState<{ field: string; anchor: HTMLButtonElement } | null>(null)
  const catalog = useStateCatalog()
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['contact', id], queryFn: () => request({ action: 'get_contact', id }, 'contact') })
  const overview = useQuery({ queryKey: ['development-overview', id], queryFn: () => request({ action: 'development_overview', id }, 'development_overview') })
  const recent = useQuery({
    queryKey: ['activities', id, '', 0],
    enabled: Boolean(drawer && section === 'overview' && !editing),
    queryFn: () => request({ action: 'list_activities', contact_id: id, event_type: '', offset: 0 }, 'activities'),
  })
  const archive = useMutation({
    mutationFn: ({ archived, target }: { archived: boolean; target: CellTarget }) =>
      request({ action: 'set_contact_archive', target, archived }, 'saved'),
    onSuccess: () => Promise.all([
      cache.invalidateQueries({ queryKey: ['contacts'] }), cache.invalidateQueries({ queryKey: ['contact', id] }),
    ]),
  })
  if (!query.data) return <>{!drawer && <button className="button" onClick={back}>返回客户列表</button>}
    {query.isError ? <Problem error={query.error} /> : <Empty>正在读取客户…</Empty>}</>
  const record = query.data.record
  const { input } = record
  const archived = record.archived_at !== null
  const primaryEmail = input.methods.find(method => method.kind === 'email' && method.is_primary)
    ?? input.methods.find(method => method.kind === 'email')
  function copyChannel(value: string) {
    void navigator.clipboard.writeText(value).then(() => setCopy('已复制联系方式'), () => setCopy('无法写入剪贴板'))
  }
  function openSection(next: keyof typeof detailSections) {
    setStartFollowup(false)
    setSection(next)
  }
  const additionalProperties = <>
    {textFields.filter(([key]) => !quickFields.some(([, source]) => source === key)).map(([key, label]) =>
      (input[key] || showEmpty) && <div key={key}><dt>{label}</dt><dd>{stateLabel(catalog, key, input[key])}</dd></div>)}
    {(!drawer || showEmpty || record.effective_first_contact_at) && <div><dt>首次联系</dt><dd>{dateLabel(record.effective_first_contact_at)}</dd></div>}
    {(!drawer || showEmpty || record.latest_thread_reply_at) && <div><dt>最近会话来信</dt><dd>{dateLabel(record.latest_thread_reply_at)}</dd></div>}
    <div><dt>资料更新</dt><dd>{dateLabel(record.updated_at)}</dd></div>
    <div><dt>记录来源</dt><dd>{sourceLabel(record.source)}</dd></div>
    {(input.notes || showEmpty) && <div><dt>备注</dt><dd>{input.notes || '—'}</dd></div>}
  </>
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
        {!archived && primaryEmail && <button className="button primary" onClick={() => compose(record)}><CustomerIcon name="send" />写邮件</button>}
        <button className="button" onClick={() => company(id)}><CustomerIcon name="building" />公司联系人</button>
        {!archived && section !== 'activity' && <button className="button" onClick={() => { setStartFollowup(true); setSection('activity') }}>
          <CustomerIcon name="addActivity" />新增跟进
        </button>}
        {!record.merged_into && <ConfirmButton label={archived ? '恢复这个客户？' : '归档这个客户？已有跟进仍可查看。'}
          disabled={archive.isPending} onConfirm={() => archive.mutate({ archived: !archived, target: targetFor(record) })}>
          <CustomerIcon name={archived ? 'restore' : 'archive'} />{archived ? '恢复客户' : '归档客户'}
        </ConfirmButton>}
        {record.merged_into && <button className="button" onClick={() => openRelated(record.merged_into!)}>打开合并后的客户</button>}
      </div>
    </div>
    {archive.isError && <Problem error={archive.error} />}
    {copy && <p className="muted customer-copy-feedback" role="status">{copy}</p>}
    <nav className="detail-sections" aria-label="客户详情分区">{Object.entries(detailSections).map(([key,label]) =>
      <button className="text-button" type="button" key={key} aria-pressed={section===key}
        onClick={() => openSection(key as keyof typeof detailSections)}><CustomerIcon name={detailIcons[key as keyof typeof detailIcons]} />{label}</button>)}
    </nav>
    {section === 'evidence' && <LeadEvidencePanel id={id} contact readOnly={archived} />}
    {section === 'changes' && <ContactHistory id={id} />}
    <div hidden={section!=='overview'}>
    {overview.isError && <p className="alert warning" role="status">开发概况暂不可用，请稍后重试。</p>}
    {overview.data && <section className="panel customer-development-summary">
      <div className="section-heading"><h2>开发概况</h2><button type="button" className="text-button" onClick={() => openSection('mail')}>查看邮件</button></div>
      <dl className="profile-grid">
        <div><dt>邮件状态</dt><dd><span className="customer-status" data-status={overview.data.record.mail_outreach.status}>{outreachLabels[overview.data.record.mail_outreach.status]}</span></dd></div>
        <div><dt>最近发信</dt><dd>{dateLabel(overview.data.record.mail_outreach.last_sent_at)}</dd></div>
        <div><dt>最近回复</dt><dd>{dateLabel(overview.data.record.mail_outreach.last_reply_at)}</dd></div>
        <div><dt>收件同步</dt><dd>{outreachSyncLabel(overview.data.record.mail_outreach)}</dd></div>
      </dl>
      {overview.data.record.related_contacts.length > 0 && <div className="customer-related-hint"><strong>相关公司或同域联系人</strong>
        <p className="muted">以下为资料关联提示，可分别维护和联系。</p>
        <ul>{overview.data.record.related_contacts.map(row => <li key={row.id}>
          <button type="button" className="text-button" onClick={() => openRelated(row.id)}>{row.display_name}</button>
          <span>{row.reason === 'email_domain' ? '邮箱域名相同' : row.reason === 'company_profile' ? '同公司档案' : '公司名称相同'} · {outreachLabels[row.mail_status]} · 最近联系 {dateLabel(row.last_contact_at)}</span>
        </li>)}</ul>{overview.data.record.related_has_more && <small>仅显示前 20 条关联联系人，可在客户管理中继续搜索。</small>}
      </div>}
    </section>}
    {drawer && <div className="customer-contact-summary">
      {primaryEmail && <div>
        <CustomerIcon name="mail" /><span title={primaryEmail.value}>{primaryEmail.value}</span>
        <IconButton label="复制主要邮箱" onClick={() => copyChannel(primaryEmail.value)}><CustomerIcon name="copy" /></IconButton>
      </div>}
      <button type="button" className="text-button" onClick={() => openSection('mail')}>
        <CustomerIcon name="send" />邮件开发<span className="customer-status" data-status={record.mail_outreach.status}>{outreachLabels[record.mail_outreach.status]}</span>
        <CustomerIcon name="arrow" />
      </button>
    </div>}
    <details open className="panel customer-properties">
      <summary><CustomerIcon name="contact" /><strong>基本资料</strong><CustomerIcon name="expand" /></summary>
      <div className="property-tools">
        {drawer && <label><input type="checkbox" checked={showEmpty} onChange={event => setShowEmpty(event.target.checked)} />显示空字段</label>}
        {!archived && <button className="text-button customer-section-edit" onClick={() => setEditing('basic')}><CustomerIcon name="edit" />编辑资料</button>}
      </div>
      <dl className="profile-grid">
        <div><dt>客户名称</dt><dd>{input.display_name}</dd></div>
        {quickFields.map(([id, key]) => {
          const column = systemColumns.find(candidate => candidate.id === id)!
          const value = input[key]
          if (!value && !showEmpty) return null
          const label = key === 'stage' ? stages[input.stage] : stateLabel(catalog, key, value)
          return <div key={id}><dt>{column.title}</dt><dd>
            {drawer && !archived ? <button type="button" className="customer-edit-property" aria-label={`编辑${column.title}`}
              onClick={event => setPropertyEditor({ field: id, anchor: event.currentTarget })}>
              <span>{label}</span><CustomerIcon name="edit" />
            </button> : label}
            {propertyEditor?.field === id && <CustomerCellEditor key={id} record={record} initialValue={value} column={column}
              catalog={catalog} anchor={propertyEditor.anchor} finish={(_, focus = true) => {
                const anchor = propertyEditor.anchor
                setPropertyEditor(current => current?.field === id ? null : current)
                if (focus) requestAnimationFrame(() => anchor.focus())
              }} />}
          </dd></div>
        })}
        {(!drawer || showEmpty || record.effective_last_contact_at) && <div><dt>最近联系</dt><dd>{dateLabel(record.effective_last_contact_at)}</dd></div>}
        {!drawer && additionalProperties}
      </dl>
      {drawer && <details className="customer-more-properties">
        <summary>更多属性<CustomerIcon name="expand" /></summary>
        <dl className="profile-grid">{additionalProperties}</dl>
      </details>}
    </details>
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
      {input.methods.length ? <ul className="record-list contact-channels">{input.methods.map(method =>
        <li key={method.id}><div><strong>{methodKinds[method.kind]}</strong><p>{method.value}</p>
          {(method.label || method.is_primary || method.is_verified) && <small>{[method.label, method.is_primary ? '主要' : '', method.is_verified ? '已核实' : ''].filter(Boolean).join(' · ')}</small>}</div>
          <IconButton label={`复制${methodKinds[method.kind]}`} onClick={() => copyChannel(method.value)}><CustomerIcon name="copy" /></IconButton></li>)}</ul>
        : <p className="muted">尚无联系渠道</p>}
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
    {section==='activity' && <Timeline contact={record} startNew={startFollowup} />}
    {section==='mail' && <CustomerMail contactId={id} outreach={record.mail_outreach} open={() => history(id)} openFiles={() => files(id)} />}
  </>
}

function CustomerMail({ contactId, outreach, open, openFiles }: { contactId: string; outreach: Outreach; open: () => void; openFiles: () => void }) {
  const { data, isPending, error } = useQuery({
    queryKey: ['mail-drafts', contactId, 0],
    queryFn: () => mailCall({ action: 'list_drafts', contact_id: contactId, offset: 0 }, 'drafts'),
  })
  return <section className="panel"><div className="section-heading">
    <h2>发件记录</h2><div className="row-actions"><button className="button" onClick={open}><CustomerIcon name="mail" />查看来信与会话</button>
      <button className="button" onClick={openFiles}><CustomerIcon name="note" />查看客户附件</button></div>
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

export function useStateCatalog() {
  return useQuery({ queryKey: ['crm-states'], queryFn: () => request({ action: 'state_catalog' }, 'state_catalog'), staleTime: Infinity }).data?.options ?? []
}

function findState(catalog: StateOption[], field: string, value: string) {
  const normalized = value.trim().toLowerCase()
  const code = field === 'customer_level' && normalized.startsWith('a |') ? 'a' : normalized
  return catalog.find(option => option.field === field &&
    (option.code.toLowerCase() === code || option.aliases.includes(code)))
}

export function stateLabel(catalog: StateOption[], field: string, value: string) {
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
