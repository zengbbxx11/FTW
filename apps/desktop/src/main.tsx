import { useCallback, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { invoke } from '@tauri-apps/api/core'
import './styles.css'

type Health = {
  coreVersion: string
  schemaVersion: number
  storageMode: 'portable' | 'local_app_data' | 'development'
  dataRoot: string
  processId: number
}

type Issue = { code: string; detail: string }

type Check =
  | { status: 'loading' }
  | { status: 'ready'; health: Health; checkedAt: Date }
  | { status: 'error'; issue: Issue }

const modeLabels: Record<Health['storageMode'], string> = {
  portable: '便携模式',
  local_app_data: 'Windows 用户目录',
  development: '开发目录',
}

const errorLabels: Record<string, string> = {
  profile_unavailable: '无法打开数据目录',
  pipe_unavailable: '无法连接 Core',
  core_missing: '找不到 Core 程序',
  core_start_failed: 'Core 启动失败',
  core_timeout: 'Core 响应超时',
  core_unhealthy: 'Core 无法读取数据库',
}

function normalizeIssue(value: unknown): Issue {
  if (typeof value === 'object' && value !== null && 'code' in value && 'detail' in value) {
    return { code: String(value.code), detail: String(value.detail) }
  }
  return { code: 'unknown', detail: String(value) }
}

function App() {
  const [check, setCheck] = useState<Check>({ status: 'loading' })

  const refresh = useCallback(async () => {
    setCheck({ status: 'loading' })
    try {
      const health = await invoke<Health>('core_health')
      setCheck({ status: 'ready', health, checkedAt: new Date() })
    } catch (error) {
      setCheck({ status: 'error', issue: normalizeIssue(error) })
    }
  }, [])

  useEffect(() => { void refresh() }, [refresh])

  return (
    <>
      <header className="app-header">
        <div className="brand-mark" aria-hidden="true">F</div>
        <div className="brand-name">FTW <span>外贸智能工作台</span></div>
      </header>
      <main className="workspace">
        <div className="page-heading">
          <h1>系统状态</h1>
          <button className="refresh" type="button" onClick={() => void refresh()} disabled={check.status === 'loading'}>
            {check.status === 'loading' ? '正在检查…' : '重新检查'}
          </button>
        </div>

        <section className="status-card" aria-label="Core 连接" aria-live="polite">
          <div className="status-header">
            <div className={'status-icon ' + check.status} aria-hidden="true">
              {check.status === 'ready' ? '✓' : check.status === 'error' ? '!' : '·'}
            </div>
            <div>
              <h2>
                {check.status === 'loading' && '正在连接 Core'}
                {check.status === 'ready' && 'Core 已就绪'}
                {check.status === 'error' && (errorLabels[check.issue.code] ?? '健康检查失败')}
              </h2>
              {check.status === 'ready' && (
                <p className="muted">上次检查 {check.checkedAt.toLocaleTimeString('zh-CN', { hour12: false })}</p>
              )}
            </div>
          </div>

          {check.status === 'ready' && (
            <dl className="details">
              <div className="detail-cell">
                <dt>Core 版本</dt>
                <dd>{check.health.coreVersion}</dd>
              </div>
              <div className="detail-cell">
                <dt>数据库版本</dt>
                <dd>{check.health.schemaVersion}</dd>
              </div>
              <div className="detail-cell">
                <dt>存储模式</dt>
                <dd>{modeLabels[check.health.storageMode]}</dd>
              </div>
              <div className="detail-cell">
                <dt>进程 ID</dt>
                <dd>{check.health.processId}</dd>
              </div>
              <div className="detail-cell data-path">
                <dt>数据目录</dt>
                <dd>{check.health.dataRoot}</dd>
              </div>
            </dl>
          )}

          {check.status === 'error' && (
            <p className="error-detail">{check.issue.detail}</p>
          )}
        </section>
      </main>
    </>
  )
}

createRoot(document.getElementById('root')!).render(<App />)
