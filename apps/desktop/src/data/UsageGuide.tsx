import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useQuery } from '@tanstack/react-query'
import { ArrowRight, BookOpen, Bot, Copy, DatabaseBackup, Download, LifeBuoy, Mail, Send, UsersRound } from 'lucide-react'

type GuideTarget = 'accounts' | 'imap' | 'agent' | 'customers' | 'backup' | 'system'
type ConnectionConfig = { command: string; args: string[] }

function connectionError(error: unknown) {
  if (typeof error === 'string' && /not allowed|denied|forbidden/i.test(error)) return '当前程序的连接配置权限异常，请更新工作台程序。'
  const code = typeof error === 'object' && error !== null && 'code' in error ? String(error.code) : ''
  if (code === 'mcp_missing') return '找不到 ftw-mcp.exe，请确认它与工作台位于同一目录。'
  return '无法生成连接配置，请在系统状态查看工作台状态。'
}

export function UsageGuide({ navigate }: { navigate: (target: GuideTarget) => void }) {
  const [notice, setNotice] = useState('')
  const connection = useQuery({ queryKey: ['mcp-connection-config'], queryFn: () => invoke<ConnectionConfig>('mcp_connection_config') })
  const server = connection.data && { type: 'stdio', command: connection.data.command, args: connection.data.args }
  const full = server && JSON.stringify({ mcpServers: { ftw: server } }, null, 2)
  const fragment = server && JSON.stringify({ ftw: server }, null, 2).slice(1, -1).trim()
  async function copy(value: string) {
    try { await navigator.clipboard.writeText(value); setNotice('已复制配置。请到 WorkBuddy 的 MCP 配置中粘贴。') }
    catch { setNotice('无法访问剪贴板，请手动选择下方配置并复制。') }
  }
  return <div className="guide-workspace">
    <header className="page-heading"><div><h1>FTW 快速使用指南</h1><p>从配置邮箱到与 Agent 协作，按需要查看以下步骤。</p></div></header>
    <div className="guide-grid">
      <section className="panel guide-card"><h2><BookOpen aria-hidden="true" />认识工作台</h2>
        <p>客户管理保存联系人和业务资料；邮件中心处理收发；工作台和 Agent 共享同一份本地数据。</p>
        <button className="button" onClick={() => navigate('customers')}><UsersRound aria-hidden="true" />打开客户管理</button>
      </section>
      <section className="panel guide-card"><h2><Mail aria-hidden="true" />配置邮箱</h2>
        <p>在邮箱账户中填写服务商提供的 SMTP 信息和授权码。需要收取回复时，再配置 IMAP 并同步收件箱。</p>
        <div className="row-actions"><button className="button" onClick={() => navigate('accounts')}>发件账户 <ArrowRight aria-hidden="true" /></button>
          <button className="button" onClick={() => navigate('imap')}>收件设置 <ArrowRight aria-hidden="true" /></button></div>
      </section>
      <section className="panel guide-card guide-connection"><h2><Bot aria-hidden="true" />连接 WorkBuddy</h2>
        <ol><li>在 WorkBuddy 打开「连接器 → 自定义连接器 → MCP 服务管理」。</li>
          <li>若配置为空，复制完整配置并粘贴；已有其他连接器时，将下方 ftw 片段加入 <code>mcpServers</code>，保留其他内容。</li>
          <li>保存后按 WorkBuddy 提示信任或刷新连接器。移动 FTW 文件夹后重新复制配置。</li></ol>
        {connection.isPending && <p className="muted">正在生成本机配置…</p>}
        {connection.isError && <p className="alert error" role="alert">{connectionError(connection.error)}</p>}
        {full && fragment && <><div className="row-actions"><button className="button primary" onClick={() => void copy(full)}><Copy aria-hidden="true" />复制完整配置</button>
          <button className="button" onClick={() => void copy(fragment)}><Copy aria-hidden="true" />复制 ftw 片段</button></div>
          <pre className="guide-code"><code>{full}</code></pre>
          <details><summary>已有其他连接器？查看 ftw 片段</summary><p>将片段放在现有 <code>mcpServers</code> 的花括号内，并用逗号与其他连接器分隔。</p><pre className="guide-code"><code>{fragment}</code></pre></details>
        </>}
        {notice && <p role="status" className="review-notice">{notice}</p>}
      </section>
      <section className="panel guide-card"><h2><Send aria-hidden="true" />Agent 发信</h2>
        <p>让 WorkBuddy 查询客户、起草或跟进邮件。自动发信前，在工作台选择本机 Agent、发件账户、用途和限额；也可选择逐封审批。</p>
        <button className="button" onClick={() => navigate('agent')}>打开发信设置 <ArrowRight aria-hidden="true" /></button>
      </section>
      <section className="panel guide-card"><h2><DatabaseBackup aria-hidden="true" />备份与换设备</h2>
        <p>定期创建备份。迁移便携版前关闭 FTW 和 MCP 客户端，完整复制工作台及数据目录；换设备后重新配置邮箱凭据和 MCP 路径。</p>
        <button className="button" onClick={() => navigate('backup')}>备份与恢复 <ArrowRight aria-hidden="true" /></button>
      </section>
      <section className="panel guide-card"><h2><Download aria-hidden="true" />更新 FTW</h2>
        <p>在系统状态检查 GitHub 正式版本并点击“立即更新”。原来的客户资料、邮箱配置和连接器路径会保留；更新前请完成任务并关闭 WorkBuddy。</p>
        <p>旧版本首次升级时，打开旧 FTW，再运行新版便携包中的 FTW-Update.exe。</p>
        <button className="button" onClick={() => navigate('system')}>打开软件更新 <ArrowRight aria-hidden="true" /></button>
      </section>
      <section className="panel guide-card"><h2><LifeBuoy aria-hidden="true" />遇到问题</h2>
        <p>在系统状态查看连接和同步情况，必要时导出诊断报告，并附上发生时间与操作步骤。</p>
        <button className="button" onClick={() => navigate('system')}>打开系统状态 <ArrowRight aria-hidden="true" /></button>
      </section>
    </div>
  </div>
}
