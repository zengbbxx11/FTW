import { Archive, CircleAlert, CircleCheck, CircleHelp, Clock, FilePenLine, LoaderCircle, Search, Send } from 'lucide-react'
import { states, type DraftRow } from './api'

const stateIcons = {
  draft: FilePenLine, pending_approval: Clock, approved: CircleCheck, queued: Clock, sending: LoaderCircle,
  smtp_accepted: Send, failed: CircleAlert, delivery_unknown: CircleHelp,
}

export function MailStatus({ row, bounced = false }: { row: DraftRow; bounced?: boolean }) {
  const Icon = bounced ? Archive : row.approval_expired ? Clock : stateIcons[row.state]
  return <span className="mail-status" data-status={bounced ? 'bounced' : row.approval_expired ? 'expired' : row.state}>
    <Icon aria-hidden="true" />{bounced ? '已退信' : row.approval_expired ? '批准已过期' : states[row.state]}
  </span>
}

export function MailSearch({ label, value, change }: { label: string; value: string; change: (value: string) => void }) {
  return <label className="mail-search">
    <Search aria-hidden="true" />
    <input type="search" maxLength={100} aria-label={label} placeholder={label} value={value} onChange={event => change(event.target.value)} />
  </label>
}
