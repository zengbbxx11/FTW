import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { request } from './api'
import { Empty, Field, Problem } from './components'
import { columnsWithDefaults, type AvailableColumn, type ColumnSetting, type SavedView, type ViewConfig, type ViewInput } from './viewTypes'

import { CustomerIcon } from './CustomerIcon'

export function useSavedViews() {
  return useQuery({ queryKey: ['crm-views'], queryFn: () => request({ action: 'list_views', include_archived: true }, 'views') })
}

export function SavedViewBar({ views, active, config, baseRevision, counts, select, saved }: {
  views: SavedView[]; active?: SavedView; config: ViewConfig; baseRevision: number | null
  counts?: { active: number; priority: number; sample: number }; select: (id: string) => void; saved: (id: string) => void
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
      <nav className="customer-view-tabs" aria-label="保存的客户视图">
      {live.filter(view=>!view.hidden || view.id === active?.id).map(view =>
        <button key={view.id} className="button" aria-pressed={active?.id === view.id} onClick={() => select(view.id)}>
          <CustomerIcon name={view.id === 'priority' ? 'star' : view.id === 'sample' ? 'box' : view.config.layout === 'board' ? 'board' : 'grid'} />
          {view.name}{dirty && active?.id === view.id ? ' *' : ''}
          {counts && ['all','priority','sample'].includes(view.id) && <span className="view-count">{counts[view.id === 'all' ? 'active' : view.id as 'priority' | 'sample']}</span>}
        </button>)}
      <button className="button add-view" onClick={() => create()}>＋ 新建视图</button>
      </nav>
      <div className="customer-view-actions">
        {dirty && active && <button className="button" disabled={save.isPending} onClick={() => save.mutate(edit(active,config))}>保存当前视图</button>}
        <button className="button" aria-expanded={manager} onClick={() => setManager(value => !value)}>管理视图</button>
      </div>
    </div>
    {editor && <form className="panel" onSubmit={event => { event.preventDefault(); save.mutate(editor) }}>
      <div className="form-grid">
        <Field label="视图名称"><input autoFocus value={editor.name} maxLength={100} required onChange={event => setEditor({ ...editor, name: event.target.value })} /></Field>
        <label><input type="checkbox" checked={editor.is_default} disabled={active?.id === editor.id && active?.is_default}
          onChange={event => setEditor({ ...editor, is_default: event.target.checked, hidden: event.target.checked ? false : editor.hidden })} /> 设为默认视图</label>
        <label><input type="checkbox" checked={editor.hidden} disabled={editor.is_default}
          onChange={event => setEditor({ ...editor, hidden: event.target.checked })} /> 隐藏标签</label>
      </div>
      <div className="form-footer"><button className="button" type="button" onClick={() => setEditor(null)}>取消</button>
        <button className="button primary" disabled={save.isPending}>保存</button></div>
    </form>}
    {save.isError && <Problem error={save.error} />}
    {manager && <section className="panel"><h2>管理客户视图</h2>
      {manage.isError && <Problem error={manage.error} />}
      {views.length === 0 ? <Empty>没有已保存的视图。</Empty> : <ul className="record-list">
        {views.map(view => {
          const target = { id:view.id, expected_revision:view.revision }
          const index = live.findIndex(current => current.id === view.id)
          return <li key={view.id}><strong>{view.name} {view.is_default ? '· 默认' : view.archived ? '· 已归档' : view.hidden ? '· 已隐藏' : ''}</strong>
            <div className="row-actions">
              {!view.archived && <>
                <button className="button" onClick={() => select(view.id)}>打开</button>
                <button className="button" onClick={() => setEditor(edit(view,view.id===active?.id ? config : view.config))}>设置</button>
                <button className="button" onClick={() => {
                  setEditor({ ...edit(view,view.id===active?.id ? config : view.config), id:null, expected_revision:null,
                    name:view.name + ' 副本', is_default:false, hidden:false })
                }}>复制</button>
                <button className="button" aria-label={'上移 ' + view.name} disabled={manage.isPending || index===0}
                  onClick={() => manage.mutate({ ...target,direction:-1 })}>上移</button>
                <button className="button" aria-label={'下移 ' + view.name} disabled={manage.isPending || index===live.length-1}
                  onClick={() => manage.mutate({ ...target,direction:1 })}>下移</button>
              </>}
              <button className="button" disabled={view.is_default || manage.isPending}
                onClick={() => manage.mutate({ ...target,archived:!view.archived })}>{view.archived?'恢复':'归档'}</button>
            </div></li>
        })}
      </ul>}
    </section>}
  </>
}

export function ColumnSettings({ available, saved, change, manageFields }: {
  available: AvailableColumn[]; saved: ColumnSetting[] | undefined; change: (columns: ColumnSetting[]) => void; manageFields: () => void
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
    const [value] = next.splice(index,1)
    next.splice(neighbor,0,value)
    change(next)
  }
  return <details className="column-settings"><summary><CustomerIcon name="settings" />列设置</summary><div className="panel column-config">
    <div className="section-heading"><div><h2>显示字段</h2><small>自定义列 {customVisible} / 12</small></div>
      <button className="button" onClick={manageFields}>字段与属性</button></div>
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
        }} onDragEnd={() => setDragged(null)}>⋮⋮</span>
        <label className="column-toggle"><input type="checkbox" role="switch" checked={!column.hidden} disabled={column.field==='name' || Boolean(definition.custom && column.hidden && customVisible >= 12)}
          onChange={event=>patch(column.field,{hidden:!event.target.checked})} /> {definition.title}</label>
        <Field label="显示名称"><input value={column.label??''} placeholder={definition.title} maxLength={96}
          onChange={event=>patch(column.field,{label:event.target.value||null})} /></Field>
        <Field label="宽度"><input type="number" min="72" max="640" key={column.width} defaultValue={column.width}
          onBlur={event=>{const n=event.target.valueAsNumber;if(Number.isInteger(n))patch(column.field,{width:Math.min(640,Math.max(72,n))})}} /></Field>
        <div className="row-actions">
          <button className="button" disabled={index===0} onClick={()=>move(index,index-1)}>上移</button>
          <button className="button" disabled={index===columns.length-1} onClick={()=>move(index,index+1)}>下移</button>
        </div>
      </li>
    })}</ul>
  </div></details>
}
