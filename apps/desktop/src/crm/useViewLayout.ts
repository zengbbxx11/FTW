import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { request } from './api'
import type { SavedView, ViewConfig } from './viewTypes'

type Layout = Pick<ViewConfig, 'columns' | 'density'>
type Pending = { view: SavedView; patch: Layout; sequence: number }

// Only presentation is merged into the saved view; unsaved business queries never enter this queue.
export function useViewLayout(active: SavedView | undefined, saved: (id: string, from: number, to: number, patch: Layout) => void) {
  const cache = useQueryClient()
  const [drafts, setDrafts] = useState<Record<string, Layout>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const pending = useRef(new Map<string, Pending>())
  const latest = useRef(new Map<string, Pending>())
  const written = useRef(new Map<string, { base: number; revision: number }>())
  const sequence = useRef(0)
  const working = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const onSaved = useRef(saved)
  useEffect(() => { onSaved.current = saved }, [saved])
  async function flush() {
    clearTimeout(timer.current)
    if (working.current) return
    working.current = true
    setBusy(true)
    while (pending.current.size) {
      const [id, task] = pending.current.entries().next().value!
      pending.current.delete(id)
      const own = written.current.get(id)
      const revision = own?.base === task.view.revision ? own.revision : task.view.revision
      const view = { id, expected_revision: revision, name: task.view.name, hidden: task.view.hidden, is_default: task.view.is_default,
        config: { ...task.view.config, ...task.patch } }
      try {
        await request({ action: 'save_view', view }, 'saved')
        const next = revision + 1
        written.current.set(id, { base: task.view.revision, revision: next })
        cache.setQueryData<{ kind: 'views'; views: SavedView[] }>(['crm-views'], current => current && ({ ...current,
          views: current.views.map(item => item.id === id && item.revision === revision ? { ...item, revision: next, config: view.config } : item) }))
        onSaved.current(id, revision, next, task.patch)
        if (latest.current.get(id)?.sequence === task.sequence) {
          latest.current.delete(id)
          setDrafts(current => { const result = { ...current }; delete result[id]; return result })
        }
      } catch (issue) {
        pending.current.delete(id)
        setError(issue)
        await cache.invalidateQueries({ queryKey: ['crm-views'] })
      }
    }
    working.current = false
    setBusy(false)
  }
  const flushRef = useRef(flush)
  useEffect(() => { flushRef.current = flush })
  useEffect(() => () => { clearTimeout(timer.current); void flushRef.current() }, [])
  function change(patch: Layout) {
    if (!active || error) return
    const previous = latest.current.get(active.id)
    const task = { view: previous?.view ?? active, patch: { ...previous?.patch, ...patch }, sequence: ++sequence.current }
    latest.current.set(active.id, task)
    pending.current.set(active.id, task)
    setDrafts(current => ({ ...current, [active.id]: task.patch }))
    setBusy(true)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => { void flushRef.current() }, 450)
  }
  function discard() {
    pending.current.clear(); latest.current.clear(); written.current.clear()
    setDrafts({}); setError(null); setBusy(false)
  }
  return { draft: active ? drafts[active.id] : undefined, change, busy, error, discard }
}
