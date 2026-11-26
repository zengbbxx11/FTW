import { useState, type CSSProperties } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight, Clock, Database, Download, Mail, Send, TriangleAlert } from 'lucide-react'
import { dateLabel } from '../crm/api'
import { Empty } from '../crm/components'
import { DataError } from './DataTools'
import { OpsStat } from './OpsUI'
import { send, type MailAnalytics as Report } from './api'

export type MailReportLink = 'mail' | 'bounced' | 'failed' | 'unknown' | 'waiting' | 'sync'
const periods = [7, 30, 90]
const ratio = (replied: number, accepted: number) => accepted ? (100 * replied / accepted).toFixed(1) + '%' : '—'

export function MailAnalytics({ open }: { open: (kind: MailReportLink) => void }) {
  const [days, setDays] = useState(30)
  const [includeLegacy, setIncludeLegacy] = useState(false)
  const query = useQuery({ queryKey: ['mail-analytics', days, includeLegacy],
    queryFn: () => send({ action: 'mail_analytics', days, include_legacy: includeLegacy }, 'mail_analytics') })
  const csv = useMutation({ mutationFn: () => send({ action: 'export_mail_analytics', days, include_legacy: includeLegacy }, 'file') })
  const report = query.data?.data
  return <div className="ops-workspace mail-analytics">
    <header className="page-heading"><div><h1>邮件开发效果分析</h1><p>按开发邮件统计 SMTP 提交、已关联回复、退信和同步情况</p></div>
      <div className="row-actions"><button type="button" className="button" onClick={() => open('mail')}><ArrowLeft aria-hidden="true" />邮件中心</button>
        <button type="button" className="button primary" disabled={!report || csv.isPending} onClick={() => csv.mutate()}>
          <Download aria-hidden="true" />{csv.isPending ? '正在导出…' : '导出统计'}</button></div></header>
    <div className="ops-controls analytics-options"><div className="row-actions">
      <label>统计范围 <select aria-label="统计范围" disabled={csv.isPending} value={days} onChange={event => { setDays(Number(event.target.value)); csv.reset() }}>
        {periods.map(period => <option value={period} key={period}>最近 {period} 天</option>)}
      </select></label>
      <label><input type="checkbox" disabled={csv.isPending} checked={includeLegacy} onChange={event => { setIncludeLegacy(event.target.checked); csv.reset() }} />包含历史未分类邮件</label>
    </div></div>
    {csv.data && <p role="status" className="review-notice long-text">统计已导出到 {csv.data.path}</p>}
    {csv.isError && <DataError error={csv.error} />}
    {query.isError && <DataError error={query.error} />}
    {query.isPending && <Empty>正在读取邮件统计…</Empty>}
    {report && <ReportBody report={report} open={open} />}
  </div>
}

function ReportBody({ report, open }: { report: Report; open: (kind: MailReportLink) => void }) {
  const totals = report.totals
  return <>
    <div className="ops-metrics">
      <OpsStat icon={Send} tone="teal" name="SMTP 已接受" value={totals.accepted} note={`最近 ${report.days} 天`} />
      <OpsStat icon={Mail} tone="green" name="确认有回复" value={totals.replied} note={totals.accepted ? `占 ${ratio(totals.replied, totals.accepted)}` : undefined} />
      <OpsStat icon={TriangleAlert} tone="orange" name="明确退信" value={totals.bounced} />
      <OpsStat icon={Clock} tone="purple" name="已发未见回复" value={totals.waiting} />
    </div>
    <div className="ops-split analytics-grid">
      <div className="ops-column">
        <Trend report={report} />
        <section className="panel ops-table"><div className="section-heading"><h2>按客户来源分组</h2></div>
          {report.sources.length ? <div className="table-wrap"><table aria-label="客户来源统计"><thead>
            <tr><th>来源</th><th>已接受</th><th>有回复</th><th>退信</th><th>回复比例</th></tr></thead>
            <tbody>{report.sources.map(group => <tr key={group.source === null ? 'other:' : `source:${group.source}`}>
              <td>{group.source === null ? '其他来源（合计）' : group.source || '未填写来源'}</td><td>{group.accepted}</td>
              <td>{group.replied}</td><td>{group.bounced}</td><td>{ratio(group.replied, group.accepted)}</td>
            </tr>)}</tbody>
          </table></div> : <Empty>所选时间范围内没有已接受的开发邮件。</Empty>}
        </section>
      </div>
      <div className="ops-column">
        <Distribution totals={totals} />
        <section className="panel analytics-reminders"><div className="section-heading"><h2>业务提醒</h2></div>
          <ul className="ops-events">
            <li><span className="dashboard-symbol" data-tone="teal"><Clock aria-hidden="true" /></span><div>
              <strong>{totals.waiting} 封已发未见回复</strong><small>对应邮箱在发信后完成过同步。</small>
              <button type="button" className="text-button" onClick={() => open('waiting')}><ArrowRight aria-hidden="true" />查看 CRM 已发未见回复</button>
            </div></li>
            {(totals.failed > 0 || totals.unknown > 0 || totals.bounced > 0) && <li><span className="dashboard-symbol" data-tone="orange"><TriangleAlert aria-hidden="true" /></span><div>
              <strong>{totals.failed} 封提交失败 · {totals.unknown} 封结果未知 · {totals.bounced} 封明确退信</strong>
              <div className="row-actions">
                {totals.failed > 0 && <button type="button" className="text-button" onClick={() => open('failed')}>发送失败记录</button>}
                {totals.unknown > 0 && <button type="button" className="text-button" onClick={() => open('unknown')}>结果未知记录</button>}
                {totals.bounced > 0 && <button type="button" className="text-button" onClick={() => open('bounced')}>退信记录</button>}
              </div>
            </div></li>}
            <li><span className="dashboard-symbol" data-tone="purple"><Database aria-hidden="true" /></span><div>
              <strong>{totals.unsynced} 封同步待确认</strong><small>{totals.latest_sync ? `关联邮箱最新成功同步 ${dateLabel(totals.latest_sync)}` : '所选发信记录暂无可核对的成功同步。'}</small>
              <button type="button" className="text-button" onClick={() => open('sync')}><ArrowRight aria-hidden="true" />查看收件设置</button>
            </div></li>
          </ul>
        </section>
      </div>
    </div>
  </>
}

function Trend({ report }: { report: Report }) {
  const [table, setTable] = useState(false)
  const max = Math.max(1, ...report.trend.flatMap(day => [day.accepted, day.replied]))
  return <section className="panel analytics-trend">
    <div className="section-heading"><h2>发信与回复趋势</h2><button type="button" className="button" aria-pressed={table}
      onClick={() => setTable(!table)}>{table ? '查看图表' : '查看逐日数据'}</button></div>
    {table ? <div className="table-wrap analytics-trend-table"><table aria-label="逐日邮件开发记录">
      <thead><tr><th>日期</th><th>SMTP 已接受</th><th>首次确认有回复</th></tr></thead>
      <tbody>{report.trend.map(day => <tr key={day.day}><td>{day.day}</td><td>{day.accepted}</td><td>{day.replied}</td></tr>)}</tbody>
    </table></div> : <>
      <div className="analytics-bar-chart" data-long={report.days > 30} role="img" aria-label={`最近 ${report.days} 天，SMTP 已接受 ${report.totals.accepted} 封，确认有回复 ${report.totals.replied} 封`}>
        {report.trend.map(day => <div key={day.day} title={`${day.day} · 接受 ${day.accepted} · 回复 ${day.replied}`}>
          <span style={{ height: `${100 * day.accepted / max}%` }} /><span style={{ height: `${100 * day.replied / max}%` }} />
        </div>)}
      </div>
      <div className="dashboard-axis"><span>{report.trend[0]?.day.slice(5)}</span><span>{report.trend.at(-1)?.day.slice(5)}</span></div>
    </>}
    <div className="analytics-chart-legend"><span>按提交日统计 SMTP 已接受</span><span>按首次回复收件日统计</span></div>
  </section>
}

function Distribution({ totals }: { totals: Report['totals'] }) {
  const replied = totals.accepted ? 100 * totals.replied / totals.accepted : 0
  const waiting = totals.accepted ? 100 * (totals.replied + totals.waiting) / totals.accepted : 0
  const bounced = totals.accepted ? 100 * (totals.replied + totals.waiting + totals.bounced) / totals.accepted : 0
  return <section className="panel analytics-distribution"><div className="section-heading"><h2>开发结果分布</h2></div>
    <div className="dashboard-progress">
      <div className="dashboard-ring analytics-ring" data-empty={!totals.accepted}
        style={{ '--replied': `${replied}%`, '--waiting': `${waiting}%`, '--bounced': `${bounced}%` } as CSSProperties}>
        <span><strong>{totals.accepted}</strong><small>SMTP 已接受</small></span>
      </div>
      <dl className="dashboard-legend"><div data-tone="teal"><dt>确认有回复</dt><dd>{totals.replied}</dd></div>
        <div data-tone="purple"><dt>已发未见回复</dt><dd>{totals.waiting}</dd></div>
        <div data-tone="orange"><dt>明确退信</dt><dd>{totals.bounced}</dd></div>
        <div data-tone="gray"><dt>同步待确认</dt><dd>{totals.unsynced}</dd></div>
      </dl>
    </div>
    <p className="muted">回复比例按已接受邮件中能匹配到人工回复的邮件计算。</p>
  </section>
}
