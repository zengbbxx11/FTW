import { useEffect, useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Download, FileSpreadsheet, Upload } from 'lucide-react'
import { ConfirmButton, Field } from '../crm/components'
import { DataError } from './DataError'
import { OpsTag } from './OpsUI'
import { send, type CsvScope } from './api'

export type TransferFocus = 'import' | 'export'
const fields = {
  display_name: '客户名称', company_name: '公司或店铺', country_region: '国家或地区', stage: '客户阶段', customer_level: '客户等级',
  sample_status: '样品进度', customer_source: '客户来源', email: '邮箱', phone: '电话', whatsapp: 'WhatsApp', website_url: '网站',
  main_need: '主要需求', notes: '备注',
} as const
const allFields = Object.keys(fields) as (keyof typeof fields)[]

export function CustomerTransfer({ focus }: { focus: TransferFocus }) {
  const picker = useRef<HTMLInputElement>(null)
  const scopeNode = useRef<HTMLSelectElement>(null)
  const [file, setFile] = useState<{ name: string; text: string } | null>(null)
  const [reading, setReading] = useState(false)
  const [stage, setStage] = useState('prospect')
  const [fileError, setFileError] = useState('')
  const [scope, setScope] = useState<CsvScope>('active')
  const [selected, setSelected] = useState(allFields)
  const cache = useQueryClient()
  useEffect(() => { (focus === 'export' ? scopeNode : picker).current?.focus({ preventScroll: true }) }, [focus])
  const preview = useMutation({ mutationFn: (apply: boolean) => send({ action: 'import_contacts', csv: file!.text, apply, default_stage: stage }, 'import_report'),
    onSuccess: async reply => { if (reply.applied) await Promise.all(['contacts','audit','data-overview'].map(key => cache.invalidateQueries({ queryKey: [key] }))) } })
  const exported = useMutation({ mutationFn: () => send({ action: 'export_selected_contacts', scope, fields: selected }, 'file') })
  const fileLocked = reading || preview.isPending
  const report = preview.data

  async function select(next: File | undefined) {
    preview.reset()
    setFile(null)
    setFileError('')
    if (!next) return
    if (!next.size || next.size > 512 * 1024) { setFileError('csv_size_limit'); return }
    setReading(true)
    try { setFile({ name: next.name, text: new TextDecoder('utf-8', { fatal: true }).decode(await next.arrayBuffer()) }) }
    catch { setFileError('无法读取此文件。请先将 CSV 保存为 UTF-8 格式。') }
    finally { setReading(false) }
  }

  return <div className="ops-split data-csv-grid">
    <section className="panel data-csv-import"><div className="section-heading"><h2><Upload aria-hidden="true" />导入客户 CSV</h2></div>
      <p className="muted">支持 UTF-8 CSV，每批最多 500 条且不超过 512 KiB。</p>
      <div className="data-csv-inputs">
        <Field label="客户 CSV"><input ref={picker} type="file" accept=".csv,text/csv" disabled={fileLocked} onChange={event => void select(event.target.files?.[0])} /></Field>
        <Field label="阶段留空时"><select value={stage} disabled={fileLocked} onChange={event => { setStage(event.target.value); preview.reset() }}>
          <option value="prospect">待开发</option><option value="won">已成交</option>
        </select></Field>
      </div>
      {fileError && <DataError error={fileError} />}
      {preview.isError && <DataError error={preview.error} />}
      <div className="row-actions data-csv-actions"><button type="button" className="button" disabled={!file || fileLocked} onClick={() => preview.mutate(false)}>
        <FileSpreadsheet aria-hidden="true" />{reading ? '正在读取文件…' : '预览导入'}</button>
        {report && !report.applied && report.accepted > 0 && <ConfirmButton primary disabled={fileLocked}
          label={`将「${file?.name}」中通过校验的客户写入 FTW？提交时会再次检查重复和数据格式。`}
          onConfirm={() => preview.mutate(true)}><Upload aria-hidden="true" />确认导入</ConfirmButton>}</div>
      {report && <div className="data-import-report">
        <div className="row-actions" role="status"><OpsTag tone="green">{report.applied ? '已导入' : '可导入'} {report.accepted} 条</OpsTag>
          <OpsTag tone={report.rejected ? 'orange' : 'gray'}>跳过 {report.rejected} 条</OpsTag></div>
        {report.mappings.length > 0 && <div className="data-import-mapping"><h3>识别到的字段映射</h3><dl>{report.mappings.map(item => <div key={item.field}>
          <dt>{item.source}</dt><dd>{item.field}</dd></div>)}</dl>
          {report.unused_columns > 0 && <p className="muted">{report.unused_columns} 列未匹配到客户字段。</p>}</div>}
        {report.issues.length > 0 && <details className="data-import-issues" open><summary>跳过原因</summary><ul>{report.issues.map(issue =>
          <li key={issue.row}>CSV 记录 {issue.row} · {issue.reason}</li>)}</ul>{report.rejected > report.issues.length && <p>已显示前 {report.issues.length} 条原因。</p>}</details>}
      </div>}
    </section>
    <section className="panel data-csv-export"><div className="section-heading"><h2><Download aria-hidden="true" />导出客户 CSV</h2></div>
      <p className="muted">选择导出范围和字段。CSV 写入本地工作空间的 exports 目录。</p>
      <Field label="客户范围"><select ref={scopeNode} aria-label="导出客户范围" value={scope} disabled={exported.isPending}
        onChange={event => { setScope(event.target.value as CsvScope); exported.reset() }}>
        <option value="active">未归档客户</option><option value="archived">已归档客户</option><option value="all">全部客户（含归档）</option>
      </select></Field>
      <fieldset className="data-export-fields" disabled={exported.isPending}><legend>导出字段</legend>
        <div>{allFields.map(id => <label key={id}><input type="checkbox" checked={selected.includes(id)} onChange={event => {
          const checked = event.target.checked
          setSelected(previous => checked ? [...previous,id] : previous.filter(field => field !== id)); exported.reset()
        }} />{fields[id]}</label>)}</div>
      </fieldset>
      <div className="row-actions data-csv-actions"><button type="button" className="button" disabled={exported.isPending || selected.length === allFields.length}
        onClick={() => { setSelected(allFields); exported.reset() }}>全选字段</button>
        <button type="button" className="button primary" disabled={!selected.length || exported.isPending} onClick={() => exported.mutate()}>
          <Download aria-hidden="true" />{exported.isPending ? '正在导出…' : '生成 CSV'}</button></div>
      {exported.isError && <DataError error={exported.error} />}
      {exported.data && <p className="review-notice long-text" role="status">CSV 已生成 {exported.data.path}</p>}
    </section>
  </div>
}
