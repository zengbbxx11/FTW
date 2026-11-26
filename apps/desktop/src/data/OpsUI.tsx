import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

export function OpsStat({ icon: Icon, tone, name, value, note }: {
  icon: LucideIcon; tone: string; name: string; value: number | string; note?: string
}) {
  return <article className="panel ops-stat">
    <span className="dashboard-symbol" data-tone={tone}><Icon aria-hidden="true" /></span>
    <div><strong>{typeof value === 'number' ? value.toLocaleString('zh-CN') : value}</strong><small>{name}</small></div>
    {note && <span className="ops-metric-note">{note}</span>}
  </article>
}

export function OpsTag({ tone, children }: { tone: 'teal' | 'green' | 'orange' | 'purple' | 'red' | 'gray'; children: ReactNode }) {
  return <span className="ops-tag" data-tone={tone}>{children}</span>
}
