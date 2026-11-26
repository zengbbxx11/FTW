import type { LucideIcon } from 'lucide-react'

type Metric = { id: string; label: string; count: number; icon: LucideIcon; tone?: 'teal' | 'blue' | 'orange' | 'violet' }

export function WorkspaceMetrics<T extends Metric>({ items, active, select, label }: {
  items: readonly T[]; active: string; select: (id: T['id']) => void; label: string
}) {
  return <div className="workspace-metrics" role="group" aria-label={label}>
    {items.map(item => <button type="button" key={item.id} aria-pressed={active === item.id} data-tone={item.tone}
      onClick={() => select(item.id)}><span><item.icon aria-hidden="true" /></span>
      <span><strong>{item.count.toLocaleString('zh-CN')}</strong><small>{item.label}</small></span></button>)}
  </div>
}
