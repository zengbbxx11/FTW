import { Select } from '../ui/Select'
import { lazy, Suspense, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { request } from './api'
import { ConfirmButton, Empty, Field, Problem } from './components'
import { builtinViewIds, viewSummary } from './viewSummary'
import type { StateOption } from './api'
import { columnsWithDefaults, type AvailableColumn, type ColumnSetting, type SavedView, type ViewConfig, type ViewInput } from './viewTypes'

import { CustomerIcon } from './CustomerIcon'
import { HelpTip, ToolPopover } from '../ui/WorkbenchUI'
const ViewEditor = lazy(() => import('./ViewEditor').then(module => ({ default: module.ViewEditor })))

export function useSavedViews() {
  return useQuery({ queryKey: ['crm-views'], queryFn: () => request({ action: 'list_views', include_archived: true }, 'views') })
}

export function SavedViewBar({ views, active, config, baseRevision, counts, select, saved, available, catalog, savingLayout = false }: {
  views: SavedView[]; active?: SavedView; config: ViewConfig; baseRevision: number | null
  counts?: { active: number; priority: number; sample: number }; select: (id: string) => void; saved: (id: string) => void
  available: AvailableColumn[]; catalog: StateOption[]; manageFields?: () => void
  savingLayout?: boolean
}) {
  const [manager, setManager] = useState(false)
  const [editor, setEditor] = useState<ViewInput | null>(null)
  const cache = useQueryClient()
  const save = useMutation({
    mutationFn: (view: ViewInput) => request({ action: 'save_view', view },'saved'),
    onSuccess: async result => {
      await cache.invalidateQueries({ queryKey: ['crm-views'] })
      setEditor(null)
      saved(result.id)
    },
  })
  const manage = useMutation({
    mutationFn: (requestData: { id: string; expected_revision: number; direction: -1 | 1 } |
      { id: string; expected_revision: number; archived: boolean }) =>
      'direction' in requestData ? request({ action: 'move_view', ...requestData }, 'saved')
        : request({ action: 'archive_view', ...requestData },'saved'),
    onSuccess: () => cache.invalidateQueries({ queryKey: ['crm-views'] }),
  })
  const remove = useMutation({
    mutationFn: (view: SavedView) => request({ action: 'delete_view', id: view.id, expected_revision: view.revision }, 'saved'),
    onSuccess: async (_, view) => {
      await cache.invalidateQueries({ queryKey: ['crm-views'] })
      if (editor?.id === view.id) setEditor(null)
      if (active?.id === view.id) select(views.find(item => item.is_default && item.id !== view.id)?.id ?? 'all')
    },
  })
  const dirty = baseRevision !== null
  const live = views.filter(view => !view.archived)
  function edit(view: SavedView, configuration = view.config): ViewInput {
    return { id: view.id, expected_revision: view.id === active?.id && configuration === config ? baseRevision ?? view.revision : view.revision, name: view.name, is_default: view.is_default, hidden: view.hidden, config: configuration }
  }
  function create(name = '') {
    save.reset()
    setEditor({ id: null, expected_revision: null, name, hidden: false, is_default: false, config })
  }
  return <>
    <div className="customer-views">
      <div className="customer-view-picker">
      <nav className="customer-view-tabs" aria-label="保存的客户视图">
      {live.filter(view=>!view.hidden || view.id === active?.id).map(view =>
        <button key={view.id} className="button" aria-pressed={active?.id === view.id} onClick={() => select(view.id)}>
          <CustomerIcon name={view.id === 'priority' ? 'star' : view.id === 'sample' ? 'box'
            : view.id.startsWith('mail_') ? 'mail' : view.config.layout === 'board' ? 'board' : 'grid'} />
          {view.name}{dirty && active?.id === view.id ? ' *' : ''}
          {counts && ['all','priority','sample'].includes(view.id) && <span className="view-count">{counts[view.id === 'all' ? 'active' : view.id as 'priority' | 'sample']}</span>}
        </button>)}
      </nav>
      <HelpTip label="新建视图"><button className="button add-view" aria-label="新建视图" onClick={() => create()}>
        <CustomerIcon name="plus" /><span>新建视图</span>
      </button></HelpTip>
      </div>
      <div className="customer-view-actions">
        {dirty && active && <button className="button" disabled={save.isPending || savingLayout} onClick={() => save.mutate(edit(active,config))}>
          <CustomerIcon name="save" />保存当前视图
        </button>}
        <button className="button" aria-expanded={manager} onClick={() => setManager(value => !value)}>
          <CustomerIcon name="settings" />管理视图
        </button>
      </div>
    </div>
    {editor && <Suspense fallback={<Empty>正在打开视图编辑器…</Empty>}><ViewEditor key={editor.id ?? 'new'} value={editor} change={setEditor} current={config} available={available} catalog={catalog}
      save={() => save.mutate(editor)} cancel={() => setEditor(null)} pending={save.isPending || savingLayout} defaultLocked={views.some(view => view.id === editor.id && view.is_default)} /></Suspense>}
    {save.isError && <Problem error={save.error} />}
    {manager && <section className="panel saved-view-manager"><h2>管理客户视图</h2>
      {manage.isError && <Problem error={manage.error} />}
      {remove.isError && <Problem error={remove.error} />}
      {views.length === 0 ? <Empty>没有已保存的视图。</Empty> : <ul className="record-list">
        {views.map(view => {
          const target = { id:view.id, expected_revision:view.revision }
          const index = live.findIndex(current => current.id === view.id)
          return <li key={view.id}><div><strong>{view.name} {view.is_default ? '· 默认' : view.archived ? '· 已归档' : view.hidden ? '· 已隐藏' : ''}</strong>
            <small className="view-manager-summary">{viewSummary(view.config)}</small></div>
            <div className="row-actions">
              {!view.archived && <>
                <button className="button" onClick={() => { select(view.id); setManager(false) }}><CustomerIcon name="arrow" />打开</button>
                <button className="button" onClick={() => setEditor(edit(view,view.id===active?.id ? config : view.config))}><CustomerIcon name="settings" />设置</button>
                <button className="button" onClick={() => {
                  setEditor({ ...edit(view,view.id===active?.id ? config : view.config), id:null, expected_revision:null,
                    name:view.name + ' 副本', is_default:false, hidden:false })
                }}><CustomerIcon name="copy" />复制</button>
                <button className="button" aria-label={'上移 ' + view.name} disabled={manage.isPending || index===0}
                  onClick={() => manage.mutate({ ...target,direction:-1 })}><CustomerIcon name="up" />上移</button>
                <button className="button" aria-label={'下移 ' + view.name} disabled={manage.isPending || index===live.length-1}
                  onClick={() => manage.mutate({ ...target,direction:1 })}><CustomerIcon name="down" />下移</button>
              </>}
              <button className="button" disabled={view.is_default || manage.isPending}
                onClick={() => manage.mutate({ ...target,archived:!view.archived })}>
                <CustomerIcon name={view.archived ? 'restore' : 'archive'} />{view.archived?'恢复':'归档'}
              </button>
              {!builtinViewIds.has(view.id) && <ConfirmButton label={`删除视图“${view.name}”？只删除视图配置，不删除任何客户、邮件或沟通记录。`}
                disabled={view.is_default || remove.isPending} onConfirm={() => remove.mutate(view)}><CustomerIcon name="delete" />删除</ConfirmButton>}
              {view.is_default && <small>更换默认视图后可归档或删除</small>}
            </div></li>
        })}
      </ul>}
    </section>}
  </>
}

export function ColumnSettings({ available, saved, change, manageFields, density, setDensity }: {
  available: AvailableColumn[]; saved: ColumnSetting[] | undefined; change: (columns: ColumnSetting[]) => void; manageFields?: () => void
  density: 'standard' | 'compact'; setDensity: (density: 'standard' | 'compact') => void
}) {
  const columns = columnsWithDefaults(saved,available)
  const [dragged, setDragged] = useState<string | null>(null)
  const customVisible = columns.filter(column => !column.hidden && available.some(field => field.id === column.field && field.custom)).length
  function patch(field: string, update: Partial<ColumnSetting>) {
    change(columns.map(column => column.field === field ? { ...column, ...update } : column))
  }
  function move(index: number, neighbor: number) {
    const next = [...columns]
    if (index < 0 || neighbor < 0 || neighbor >= next.length) return
    if (next[index].field === 'name' || next[neighbor].field === 'name') return
    const [value] = next.splice(index,1)
    next.splice(neighbor,0,value)
    change(next)
  }
  return <ToolPopover label="列设置" className="customer-column-popover"
    trigger={<button type="button" className="button"><CustomerIcon name="settings" />属性与列设置</button>}>
    <div className="section-heading"><div><h2 className="ftw-popup-heading">显示字段</h2><small>自定义列 {customVisible} / 12</small></div>
      <div className="row-actions">
        <label className="customer-page-size">行密度 <Select value={density} onChange={event => setDensity(event.target.value as typeof density)}>
          <option value="standard">标准</option><option value="compact">紧凑</option>
        </Select></label>
        {manageFields && <button className="button" onClick={manageFields}><CustomerIcon name="settings" />字段与属性</button>}
      </div>
    </div>
    <div className="column-config-header" aria-hidden="true"><span></span><span>字段</span><span>显示名称</span><span>列宽</span><span>顺序</span></div>
    <ul className="record-list column-config-rows">{columns.map((column,index)=>{
      const definition = available.find(item=>item.id===column.field)
      if (!definition) return <li key={column.field}><strong>{column.label || '已归档字段'}</strong>
        <button className="button" onClick={()=>change(columns.filter(item=>item.field!==column.field))}>移除失效列</button></li>
      return <li key={column.field} onDragOver={event => { if (dragged) event.preventDefault() }} onDrop={event => {
        if (dragged && event.dataTransfer.getData('application/x-ftw-column') === dragged) {
          event.preventDefault()
          move(columns.findIndex(item => item.field === dragged), index)
        }
        setDragged(null)
      }}>
        <span className="column-drag" draggable aria-hidden="true" onDragStart={event => {
          event.dataTransfer.setData('application/x-ftw-column', column.field)
          setDragged(column.field)
        }} onDragEnd={() => setDragged(null)}><CustomerIcon name="grip" /></span>
        <label className="column-toggle"><input type="checkbox" role="switch" checked={!column.hidden} disabled={column.field==='name' || Boolean(definition.custom && column.hidden && customVisible >= 12)}
          onChange={event=>patch(column.field,{hidden:!event.target.checked})} /> {definition.title}</label>
        <Field label="显示名称"><input aria-label={definition.title + '显示名称'} value={column.label??''} placeholder={definition.title} maxLength={96}
          onChange={event=>patch(column.field,{label:event.target.value||null})} /></Field>
        <Field label="宽度"><input aria-label={definition.title + '列宽'} type="number" min="72" max="640" key={column.width} defaultValue={column.width}
          onBlur={event=>{const n=event.target.valueAsNumber;if(Number.isInteger(n))patch(column.field,{width:Math.min(640,Math.max(72,n))})}} /></Field>
        <div className="row-actions">
          <button className="button" disabled={index===0} onClick={()=>move(index,index-1)}><CustomerIcon name="up" />上移</button>
          <button className="button" disabled={index===columns.length-1} onClick={()=>move(index,index+1)}><CustomerIcon name="down" />下移</button>
        </div>
      </li>
    })}</ul>
  </ToolPopover>
}
