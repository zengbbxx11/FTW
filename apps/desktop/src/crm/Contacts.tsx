import { lazy, Suspense, useDeferredValue, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { dateInput, dateLabel, fromDateInput, methodKinds, request, sourceLabel, stages,
  type Contact, type ContactInput, type ContactQuery, type Duplicate, type Interest, type Method, type MethodKind, type Stage, type StateOption, type ContactRow } from './api'
import { ConfirmButton, Duplicates, Empty, Field, Pagination, Problem } from './components'
import { Timeline } from './Timeline'
import { CustomerFieldPanel, FieldManager, NewCustomerFields, useCustomerFields } from './Fields'
import { fieldEntries, type FieldValue } from './fieldTypes'
import CustomerTable, { targetFor } from './CustomerTable'
import { ColumnSettings, SavedViewBar, useSavedViews } from './SavedViews'
import { availableColumns, columnsWithDefaults, type AvailableColumn, type ViewConfig } from './viewTypes'

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
  const columns = columnsWithDefaults(config.columns,available)
  const projection = columns.filter(column => !column.hidden && available.some(field => field.id === column.field && field.custom)).map(column => column.field)
  const query: ContactQuery = { ...defaultQuery, ...config.query, offset, custom_columns: projection }
  const keyword = useDeferredValue(query.keyword)
  const requestQuery = { ...query, keyword }
  const [selected, setSelected] = useState<string | null>(null)
  const drawer = useRef<HTMLDialogElement>(null)
  const open = (id: string) => { setSelected(id); drawer.current?.showModal() }
  const close = () => drawer.current?.close()
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
  if (manageFields) return <FieldManager back={() => setManageFields(false)} />
  return <>
    <div className="page-heading"><h1>客户管理</h1><div className="row-actions">
      <button className="button primary" onClick={create}>新增客户</button>
    </div></div>
    {views.isError && <Problem error={views.error} />}
    {fields.isError && <Problem error={fields.error} />}
    <SavedViewBar views={records} active={active} config={config} baseRevision={local?.id === active?.id ? local?.revision ?? null : null} select={select} saved={select} />
    <div className="filters customer-toolbar">
      <Field label="搜索客户"><input placeholder="姓名、公司、邮箱或产品" value={query.keyword} onChange={event => update({keyword:event.target.value})} /></Field>
      <Field label="客户阶段"><select value={query.stage} onChange={event => update({stage:event.target.value})}>
        <option value="">全部阶段</option>{Object.entries(stages).map(([key,label]) => <option key={key} value={key}>{label}</option>)}
      </select></Field>
      <Field label="排序"><select value={query.order?.length ? 'column' : query.sort} onChange={event => update({sort:event.target.value as ContactQuery['sort'],order:[]})}>
        {query.order?.length ? <option value="column">按表格列排序</option> : null}
        <option value="updated">最近更新</option><option value="name">客户名称</option><option value="last_contact">最近联系</option>
      </select></Field>
      <Field label="显示方式"><select value={board?'board':'table'} onChange={event => configure({layout:event.target.value as 'table'|'board'})}>
        <option value="table">表格</option><option value="board">看板</option>
      </select></Field>
      {board && <Field label="分组字段"><select value={config.group_field ?? 'stage'} onChange={event => configure({group_field:event.target.value})}>
        <option value="stage">客户阶段</option>
        {(fields.data?.fields ?? []).filter(field => field.kind === 'single_select').map(field => <option key={field.id} value={field.id}>{field.name}</option>)}
      </select></Field>}
      <button className="button" onClick={() => setShowAdvanced(value=>!value)} aria-expanded={showAdvanced}>
        组合筛选{query.filter ? ' · 已应用' : ''}
      </button>
      <button className="button" onClick={() => setManageFields(true)}>字段管理</button>
      {!board && <ColumnSettings available={available} saved={columns} change={items=>configure({columns:items},false)} />}
      <details className="more-filters"><summary>更多筛选</summary>
        <div className="form-grid">
          <Field label="国家或地区"><input value={query.country} onChange={event=>update({country:event.target.value})} /></Field>
          <Field label="客户等级"><StateFilter field="customer_level" catalog={catalog} value={query.level} change={level=>update({level})} /></Field>
          <Field label="样品进度"><StateFilter field="sample_status" catalog={catalog} value={query.sample_status ?? ''} change={sample_status=>update({sample_status})} /></Field>
          <Field label="客户类型"><input value={query.customer_type} onChange={event=>update({customer_type:event.target.value})} /></Field>
          <Field label="客户来源"><input value={query.source} onChange={event=>update({source:event.target.value})} /></Field>
          <Field label="归档状态"><select value={query.archive} onChange={event=>update({archive:event.target.value as ContactQuery['archive']})}>
            <option value="active">未归档</option><option value="archived">已归档</option><option value="all">全部</option></select></Field>
          <Field label="每页数量"><select value={query.page_size??20} onChange={event=>update({page_size:Number(event.target.value) as 20|50|100})}>
            <option value="20">20</option><option value="50">50</option><option value="100">100</option></select></Field>
          <Field label="行间距"><select value={config.density??'standard'} onChange={event=>configure({density:event.target.value as 'compact'|'standard'},false)}>
            <option value="standard">标准</option><option value="compact">紧凑</option></select></Field>
        </div>
      </details>
    </div>
    {showAdvanced && <Suspense fallback={<Empty>正在加载组合筛选…</Empty>}><AdvancedFilters key={active?.id}
      columns={available} catalog={catalog} filter={query.filter} close={()=>setShowAdvanced(false)}
      apply={filter=>{update({filter});setShowAdvanced(false)}} /></Suspense>}
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
          columns={columns} catalog={catalog} density={config.density ?? 'standard'} open={open}
          resize={widths=>configure({columns:columns.map(column=>({...column,width:widths[column.field]??column.width}))},false)}
          sort={field=>update({order:[{field,descending:query.order?.[0]?.field===field && !query.order[0].descending}]})} />}
        <Pagination offset={offset} pageSize={query.page_size ?? 20} more={result.data.has_more} change={setOffset} />
      </>}
    </>}
    <dialog className="contact-drawer" ref={drawer} onClose={() => setSelected(null)} aria-label="客户详情">
      <header className="contact-drawer-header"><span className="font-semibold">客户详情</span>
        <button className="button" type="button" onClick={close}>关闭详情</button></header>
      <div className="contact-drawer-body" role="region" aria-label="客户资料和跟进" tabIndex={0}>
        {selected && <ContactDetail key={selected} id={selected} back={close} compose={compose} history={history} drawer />}
      </div>
    </dialog>
  </>
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

export function ContactEditor({ record, saved, cancel }: { record?: Contact; saved: (id: string) => void; cancel: () => void }) {
  const [input, setInput] = useState(record?.input ?? blank)
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

  return <form onSubmit={event => { event.preventDefault(); mutation.mutate(false) }}>
    <div className="page-heading"><h1>{record ? '编辑客户' : '新增客户'}</h1><button className="button" type="button" onClick={cancel}>取消</button></div>
    {changed && <div className="alert warning" role="status">客户资料已更新，当前输入仍保留。
      <ConfirmButton label="放弃本次编辑并读取最新的客户资料？" onConfirm={() => {
        setInput(record.input)
        setBase({ revision: record.record_revision, epoch: record.data_epoch })
        setDuplicate([])
        mutation.reset()
      }}>重新载入客户资料</ConfirmButton>
    </div>}
    <fieldset disabled={mutation.isPending}>
      <section className="panel">
        <h2>客户资料</h2>
        <div className="form-grid">
          <Field label="客户名称（必填）"><input required autoFocus value={input.display_name} onChange={e => update('display_name', e.target.value)} /></Field>
          <Field label="客户阶段"><select value={input.stage} onChange={e => update('stage', e.target.value as Stage)}>
            {Object.entries(stages).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
          </select></Field>
          {textFields.map(([key, label]) => <Field key={key} label={label}>
            {catalog.some(option => option.field === key)
              ? <StateSelect field={key} catalog={catalog} value={input[key]} change={value => update(key, value)} />
              : <input type={key === 'website_url' ? 'url' : 'text'} value={input[key]} onChange={e => update(key, e.target.value)} />}
          </Field>)}
          <Field label="人工记录的首次联系时间"><input type="datetime-local" value={dateInput(input.first_contact_at)}
            onChange={e => update('first_contact_at', fromDateInput(e.target.value))} /></Field>
          <Field label="人工记录的最近联系时间"><input type="datetime-local" value={dateInput(input.last_contact_at)}
            onChange={e => update('last_contact_at', fromDateInput(e.target.value))} /></Field>
          <Field label="主要需求"><textarea rows={3} value={input.main_need} onChange={e => update('main_need', e.target.value)} /></Field>
          <Field label="备注"><textarea rows={3} value={input.notes} onChange={e => update('notes', e.target.value)} /></Field>
        </div>
      </section>
      <section className="panel">
        <div className="section-heading"><h2>联系渠道</h2><button className="button" type="button" disabled={input.methods.length >= 12}
          onClick={() => update('methods', [...input.methods, { kind: 'email', value: '', label: '', is_primary: false, is_verified: false }])}>添加渠道</button></div>
        {input.methods.map((method, index) => (
          <div className="form-grid collection-row" key={method.id ?? 'new-method-' + index}>
            <Field label="渠道类型"><select value={method.kind} onChange={e => updateMethod(index, { kind: e.target.value as MethodKind })}>
              {Object.entries(methodKinds).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
            </select></Field>
            <Field label="账号"><input required type={method.kind === 'email' ? 'email' : 'text'}
              value={method.value} onChange={e => updateMethod(index, { value: e.target.value, is_verified: false })} /></Field>
            <Field label="渠道备注"><input value={method.label} onChange={e => updateMethod(index, { label: e.target.value })} /></Field>
            <div className="row-actions"><label><input type="checkbox" checked={method.is_primary} onChange={e => updateMethod(index, { is_primary: e.target.checked })} />主要</label>
              <label><input type="checkbox" checked={method.is_verified} onChange={e => updateMethod(index, { is_verified: e.target.checked })} />已核实</label>
              <button type="button" className="text-button" onClick={() => update('methods', input.methods.filter((_, i) => i !== index))}>移除</button></div>
          </div>
        ))}
      </section>
      <section className="panel">
        <div className="section-heading"><h2>产品兴趣</h2><button className="button" type="button" disabled={input.interests.length >= 20}
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
      </section>
      {!record && <NewCustomerFields values={custom} change={(id, value) => setCustom(old => ({ ...old, [id]: value }))} />}
      {mutation.isError && <Problem error={mutation.error} />}
      <Duplicates matches={duplicate}><button className="button" type="button" onClick={() => mutation.mutate(true)}>确认继续保存</button></Duplicates>
      <div className="form-footer contact-editor-footer"><button className="button primary" type="submit">{mutation.isPending ? '正在保存…' : '保存客户'}</button></div>
    </fieldset>
  </form>
}

export function ContactDetail({ id, back, compose, history, drawer }: {
  id: string; back: () => void; compose: (contact: Contact) => void; history: (id: string) => void; drawer?: boolean
}) {
  const [editing, setEditing] = useState(false)
  const catalog = useStateCatalog()
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['contact', id], queryFn: () => request({ action: 'get_contact', id }, 'contact') })
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
  if (editing) return <ContactEditor record={record} cancel={() => setEditing(false)} saved={() => setEditing(false)} />
  return <>
    {!drawer && <button className="text-button back-link" onClick={back}>返回客户列表</button>}
    <div className="page-heading">
      <div><h1>{input.display_name}</h1><p>{[input.company_name, input.country_region, stages[input.stage], sourceLabel(record.source), archived ? '已归档' : ''].filter(Boolean).join(' · ')}</p></div>
      <div className="row-actions">
        {!archived && <button className="button" onClick={() => setEditing(true)}>编辑客户</button>}
        {!archived && input.methods.some(method => method.kind === 'email') && <button className="button primary" onClick={() => compose(record)}>写邮件</button>}
        <button className="button" onClick={() => history(id)}>邮件往来</button>
        <ConfirmButton label={archived ? '恢复这个客户？' : '归档这个客户？已有跟进仍可查看。'}
          disabled={archive.isPending} onConfirm={() => archive.mutate(!archived)}>{archived ? '恢复客户' : '归档客户'}</ConfirmButton>
      </div>
    </div>
    {archive.isError && <Problem error={archive.error} />}
    <section className="panel"><h2>客户资料</h2>
      <dl className="profile-grid">
        {textFields.map(([key, label]) => input[key] && <div key={key}><dt>{label}</dt><dd>{stateLabel(catalog, key, input[key])}</dd></div>)}
        <div><dt>首次联系</dt><dd>{dateLabel(record.effective_first_contact_at)}</dd></div>
        <div><dt>最近联系</dt><dd>{dateLabel(record.effective_last_contact_at)}</dd></div>
        <div><dt>最近会话来信</dt><dd>{dateLabel(record.latest_thread_reply_at)}</dd></div>
        <div><dt>资料更新</dt><dd>{dateLabel(record.updated_at)}</dd></div>
        {input.main_need && <div><dt>主要需求</dt><dd>{input.main_need}</dd></div>}
        {input.notes && <div><dt>备注</dt><dd>{input.notes}</dd></div>}
      </dl>
    </section>
    <div className="panel-columns">
      <section className="panel"><h2>联系渠道</h2>{input.methods.length ? <ul className="record-list">{input.methods.map(method =>
        <li key={method.id}><strong>{methodKinds[method.kind]}</strong><span>{method.value}</span>
          {(method.label || method.is_primary || method.is_verified) && <small>{[method.label, method.is_primary ? '主要' : '', method.is_verified ? '已核实' : ''].filter(Boolean).join(' · ')}</small>}</li>)}</ul>
        : <p className="muted">尚无渠道</p>}</section>
      <section className="panel"><h2>产品兴趣</h2>{input.interests.length ? <ul className="record-list">{input.interests.map(interest =>
        <li key={interest.id}><strong>{interest.product_text}</strong>{interest.intent_quantity !== null &&
          <span>{interest.quantity_is_estimate ? '约 ' : ''}{interest.intent_quantity.toLocaleString()}</span>}
          {interest.requirement && <small>{interest.requirement}</small>}</li>)}</ul>
        : <p className="muted">尚无产品兴趣</p>}</section>
    </div>
    <CustomerFieldPanel contactId={record.id} archived={archived} />
    <Timeline contact={record} />
  </>
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
