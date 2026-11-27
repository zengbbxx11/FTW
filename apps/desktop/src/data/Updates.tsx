import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { ArrowRight, Download, RefreshCw } from 'lucide-react'

type UpdateInfo = { version: string; notes: string; releaseUrl: string }

function useUpdate() {
  return useQuery({
    queryKey: ['ftw-update'],
    queryFn: () => invoke<UpdateInfo | null>('check_for_update'),
    staleTime: 30 * 60 * 1000,
    retry: false,
  })
}

const messages: Record<string, string> = {
  update_download_failed: '无法访问 GitHub 或下载更新，请检查网络后重试。',
  update_release_unavailable: 'GitHub 上暂时没有可用的正式发行包或更新文件。',
  update_asset_missing: '这个发行版尚未提供完整的便携更新文件。',
  update_signature_invalid: '更新签名验证未通过，已停止更新。',
  update_checksum_invalid: '下载文件校验未通过，请重新下载。',
  update_manifest_invalid: '更新信息无效，已停止更新。',
  update_archive_invalid: '更新包内容不完整或格式无效。',
  update_key_unavailable: '当前版本尚未配置更新签名公钥。',
  update_version_changed: 'GitHub 上的最新版本已变化，请重新检查更新。',
  update_not_available: '暂时没有可用的新版本，请重新检查。',
  update_mcp_busy: '请先完成当前任务并关闭 WorkBuddy 和其他 FTW 连接器，然后点击“继续更新”。',
  update_mail_busy: '正在发送邮件或同步收件，请等待完成后点击“继续更新”。',
  update_requires_portable: '一键更新适用于正式便携版，请在便携工作台中使用。',
  update_core_incompatible: '后台 Core 版本不匹配，请关闭 FTW 和连接器后重新打开。',
  update_start_failed: '无法启动更新程序，请确认工作台所在目录可写。',
}

function errorMessage(error: unknown) {
  if (typeof error === 'string' && /not allowed|denied|forbidden/i.test(error)) return '当前程序的软件更新权限异常，请安装修复版工作台。'
  const code = typeof error === 'object' && error !== null && 'code' in error ? String(error.code) : 'update_failed'
  return messages[code] ?? '检查或更新未完成，请稍后重试。'
}

export function UpdateNotice({ open }: { open: () => void }) {
  const update = useUpdate()
  if (!update.data) return null
  return <div className="update-notice" role="status">
    <span>FTW {update.data.version} 已发布</span>
    <button type="button" className="text-button" onClick={open}>查看更新 <ArrowRight aria-hidden="true" /></button>
  </div>
}

export function UpdatePanel() {
  const update = useUpdate()
  const [downloaded, setDownloaded] = useState('')
  const [error, setError] = useState('')
  const download = useMutation({ mutationFn: (version: string) => invoke('download_update', { version }) })
  const install = useMutation({ mutationFn: () => invoke('install_update') })
  const busy = download.isPending || install.isPending

  async function start() {
    if (!update.data) return
    setError('')
    try {
      if (downloaded !== update.data.version) {
        await download.mutateAsync(update.data.version)
        setDownloaded(update.data.version)
      }
      await install.mutateAsync()
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }

  async function check() {
    setError('')
    setDownloaded('')
    await update.refetch()
  }

  return <section className="panel system-tools"><div className="section-heading"><h2>软件更新</h2></div>
    {update.isFetching && <p className="muted" role="status">正在检查 GitHub 上的正式版本…</p>}
    {!update.isFetching && update.isSuccess && !update.data && <p className="muted">当前已是最新正式版本。</p>}
    {!update.isFetching && update.isError && <p role="alert" className="alert error">{errorMessage(update.error)}</p>}
    {update.data && <>
      <p><strong>发现新版本 {update.data.version}</strong></p>
      {update.data.notes && <details><summary>查看更新说明</summary><p className="update-notes">{update.data.notes}</p></details>}
      <p className="muted">更新保留当前数据和邮箱配置。更新前请完成任务并关闭 WorkBuddy。</p>
    </>}
    {error && <p role="alert" className="alert error">{error}</p>}
    <div className="row-actions">
      <button type="button" className="button" disabled={busy || update.isFetching} onClick={() => void check()}><RefreshCw aria-hidden="true" />检查更新</button>
      {update.data && <button type="button" className="button primary" disabled={busy} onClick={() => void start()}><Download aria-hidden="true" />
        {download.isPending ? '正在下载…' : install.isPending ? '正在准备更新…' : downloaded === update.data.version ? '继续更新' : '立即更新'}
      </button>}
    </div>
  </section>
}
