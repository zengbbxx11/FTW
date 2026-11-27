import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Inbox, TriangleAlert } from 'lucide-react'
import type { Versions } from '../data/changeKeys'
import { inboxCall } from './Inbox'
import { useSyncFrequency } from './syncSettings'

const pause = (ms: number) => new Promise(resolve => window.setTimeout(resolve, ms))

function useReplyBaseline() {
  const versions = useQuery({ queryKey: ['core-changes'], queryFn: () => invoke<Versions>('core_changes') })
  const epoch = versions.data?.epoch
  const key = epoch ? `ftw.reply-cursor.${epoch}` : ''
  const baseline = useQuery({
    queryKey: ['reply-cursor', epoch], enabled: Boolean(epoch),
    queryFn: async () => {
      const stored = localStorage.getItem(key)
      if (stored !== null && Number.isSafeInteger(Number(stored))) return Number(stored)
      const initial = await inboxCall({ action: 'reply_updates', after: null }, 'reply_updates')
      localStorage.setItem(key, String(initial.cursor))
      return initial.cursor
    },
  })
  return { baseline, epoch, key }
}

export function AutoSync() {
  const [frequency] = useSyncFrequency()
  const [problem, setProblem] = useState(false)
  const cache = useQueryClient()
  const { baseline } = useReplyBaseline()
  useEffect(() => {
    if (!frequency) { setProblem(false); return }
    if (!baseline.isSuccess) return
    let stopped = false
    let timer = 0
    async function run() {
      let pending = false
      let failed = false
      try {
        const { rows } = await inboxCall({ action: 'accounts' }, 'accounts')
        for (const account of rows.filter(row => row.host)) {
          if (stopped) return
          if (account.outcome === 'running') { pending = true; continue }
          if (account.outcome !== 'more_available' && account.last_sync && Date.now() / 1000 - account.last_sync < frequency * 60) {
            if (account.outcome && account.outcome !== 'ok') failed = true
            continue
          }
          try {
            await inboxCall({ action: 'sync', account_id: account.account_id }, 'sync_started')
            let status = 'running'
            for (let attempt = 0; attempt < 28 && status === 'running' && !stopped; attempt++) {
              await pause(2000)
              const accounts = await inboxCall({ action: 'accounts' }, 'accounts')
              status = accounts.rows.find(row => row.account_id === account.account_id)?.outcome ?? ''
            }
            if (status === 'running' || status === 'more_available') pending = true
            else if (status !== 'ok') failed = true
          } catch (error) {
            const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : String(error)
            if (code.includes('imap_busy') || code.includes('imap_recent_sync')) pending = true
            else failed = true
          }
          await Promise.all(['imap-accounts', 'inbox', 'contacts', 'contact', 'dashboard'].map(key => cache.invalidateQueries({ queryKey: [key] })))
        }
      } catch { failed = true }
      if (!stopped) {
        setProblem(failed)
        timer = window.setTimeout(() => void run(), pending ? 5000 : frequency * 60_000)
      }
    }
    timer = window.setTimeout(() => void run(), 1000)
    return () => { stopped = true; window.clearTimeout(timer) }
  }, [baseline.isSuccess, cache, frequency])
  return problem ? <p className="alert warning" role="status"><TriangleAlert aria-hidden="true" />自动收件未完成，请到邮件中心检查收件状态。</p> : null
}

export function ReplyNotice({ open }: { open: () => void }) {
  const cache = useQueryClient()
  const { baseline, epoch, key } = useReplyBaseline()
  const updates = useQuery({
    queryKey: ['reply-updates', epoch, baseline.data], enabled: baseline.data !== undefined,
    queryFn: () => inboxCall({ action: 'reply_updates', after: baseline.data ?? null }, 'reply_updates'),
  })
  useEffect(() => {
    if (updates.data && baseline.data !== undefined && updates.data.cursor < baseline.data) {
      localStorage.setItem(key, String(updates.data.cursor))
      cache.setQueryData(['reply-cursor', epoch], updates.data.cursor)
    }
  }, [baseline.data, cache, epoch, key, updates.data])
  if (!updates.data?.count) return null
  return <div className="reply-notice" role="status"><Inbox aria-hidden="true" />
    <span>新同步 {updates.data.count} 条客户回复</span>
    <button type="button" className="text-button" onClick={() => {
      if (!updates.data) return
      localStorage.setItem(key, String(updates.data.cursor))
      cache.setQueryData(['reply-cursor', epoch], updates.data.cursor)
      open()
    }}>查看收件箱</button>
  </div>
}
