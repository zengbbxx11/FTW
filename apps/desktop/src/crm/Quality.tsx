import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, ClipboardCheck, CopyCheck, ExternalLink, GitMerge, Mail, MailWarning, Pencil, RefreshCw, Search, ShieldCheck, TriangleAlert, UsersRound } from 'lucide-react'
import { OpsStat, OpsTag } from '../data/OpsUI'
import { outreachLabels, request, stages, type Contact, type QualityIssue } from './api'
import { ConfirmButton, Empty, Pagination, Problem } from './components'
import { CustomerIcon } from './CustomerIcon'

const categories = [
  ['duplicate', '可能重复'], ['missing_email', '缺少邮箱'], ['invalid_email', '邮箱资料异常'], ['mail_problem', '发信异常'], ['missing_market', '缺少国家或地区'],
  ['invalid_website', '网址资料异常'], ['date_order', '联系时间异常'], ['sample_mismatch', '样品状态矛盾'], ['excluded_mail', '开发状态与发信冲突'],
] as const
const descriptions: Record<string, string> = {
  email: '相同邮箱', domain: '姓名与网站域名相同', company_country: '姓名、公司及地区相同',
  missing_email: '该客户尚未添加邮箱', invalid_email: '邮箱格式或标准化记录异常', missing_market: '国家或地区为空',
  failed: outreachLabels.failed, delivery_unknown: outreachLabels.delivery_unknown, bounced: outreachLabels.bounced,
  invalid_website: '网址格式或保存的域名异常', date_order: '人工首次联系时间晚于最近联系时间',
  sample_mismatch: '客户阶段为样品推进中，样品进度为未涉及', excluded_mail: '客户阶段为不开发，仍有未结束的发信记录',
}
const guidance: Record<string, string> = {
  duplicate: '对照姓名、邮箱、公司及沟通历史，确认两条资料属于同一人后再合并。',
  missing_email: '在客户的联系渠道中补充已核对的邮箱。',
  invalid_email: '在联系渠道中核对并更正邮箱地址。若显示地址正确，可重新保存以更新其标准化记录。',
  missing_market: '在基本信息中补充客户的国家或地区。',
  invalid_website: '在联系渠道中核对网址。保存时会重新计算域名。',
  date_order: '核对后在业务状态中调整人工记录的首次和最近联系时间。',
  sample_mismatch: '核对沟通记录，在客户资料里调整样品进度或客户阶段。',
  excluded_mail: '打开相关发信记录，核对客户状态与发信任务。后续处理使用邮件中心的既有操作。',
}
const issueKey = (issue: QualityIssue) => [issue.category, issue.contact_id, issue.other_id].join(':')
const issueLabel = (issue: QualityIssue) => categories.find(([key]) => key === issue.category)?.[1] ?? issue.category
const isMailIssue = (issue: QualityIssue) => issue.category === 'mail_problem' || issue.category === 'excluded_mail'
const issueSource = (issue: QualityIssue) => issue.category === 'excluded_mail' ? '客户资料 / 邮件记录' : isMailIssue(issue) ? '邮件记录' : '客户资料'
type RepairArea = 'basic' | 'channels' | 'business'

export function CustomerQuality({ back, open, edit, openMail, duplicatesOnly = false }: {
  back: () => void; open: (id: string) => void; edit: (id: string, area: RepairArea) => void
  openMail: (id: string, draftId?: string) => void; duplicatesOnly?: boolean
}) {
  const [category, setCategory] = useState(duplicatesOnly ? 'duplicate' : '')
  const [offset, setOffset] = useState(0)
  const [active, setActive] = useState('')
  const [compare, setCompare] = useState('')
  const [notice, setNotice] = useState('')
  const [search, setSearch] = useState('')
  const keyword = useDeferredValue(search.trim())
  const currentOffset = keyword === search.trim() ? offset : 0
  const query = useQuery({
    queryKey: ['contacts', 'quality', category, keyword, currentOffset],
    queryFn: () => request({ action: 'customer_quality', category, keyword, offset: currentOffset }, 'customer_quality'),
  })
  const counts = new Map(query.data?.counts)
  const selected = query.data?.rows.find(item => issueKey(item) === active) ?? query.data?.rows[0]
  const selectIssue = (key: string) => { setActive(key); setCompare(''); setNotice('') }
  const details = selected && (selected.other_id && (duplicatesOnly || compare === issueKey(selected)) ?
    <CompareCustomers key={issueKey(selected)} ids={[selected.contact_id, selected.other_id]}
      reason={descriptions[selected.detail]} open={open}
      merged={name => {
        selectIssue('')
        setNotice('客户已合并，保留 ' + name)
        if (query.data?.rows.length === 1 && offset) setOffset(offset - 20)
      }} />
    : <QualityDetails issue={selected} open={open} edit={edit} openMail={openMail} compare={() => setCompare(issueKey(selected))} />)
  const pagination = query.data && (offset > 0 || query.data.has_more) &&
    <Pagination offset={offset} more={query.data.has_more} change={next => { setOffset(next); selectIssue('') }} />

  return <section className={duplicatesOnly ? 'quality-workspace' : 'quality-workspace quality-inspection ops-workspace'}>
    <header className="page-heading"><div><button className="text-button back-link" onClick={back}><ArrowLeft aria-hidden="true" />返回客户管理</button>
      <h1>{duplicatesOnly ? '重复客户检测与合并' : '客户数据质量检查'}</h1><p>{duplicatesOnly ? '核对邮箱相同，或姓名及公司资料相同的联系人。' : '检查在用客户的重复资料、联系方式和发信异常。'}</p></div>
      <button className="button primary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{query.isFetching ? '正在检查…' : '重新检查'}</button>
    </header>
    {query.data && duplicatesOnly && <div className="duplicate-summary"><span><ShieldCheck aria-hidden="true" /></span>
      <strong>{counts.get('duplicate') ?? 0}</strong> 对客户待核对</div>}
    {!duplicatesOnly && <div className="ops-metrics" aria-label="全库数据质量统计">
      <OpsStat icon={UsersRound} tone="teal" name="在用客户" value={query.data?.active_customers ?? '—'} />
      <OpsStat icon={CopyCheck} tone="orange" name="可能重复" value={query.data ? counts.get('duplicate') ?? 0 : '—'} note="按客户对计" />
      <OpsStat icon={Mail} tone="purple" name="缺少邮箱" value={query.data ? counts.get('missing_email') ?? 0 : '—'} />
      <OpsStat icon={MailWarning} tone="orange" name="发信异常" value={query.data ? counts.get('mail_problem') ?? 0 : '—'} />
    </div>}
    {notice && <p className="review-notice" role="status">{notice}</p>}
    {query.isError && <Problem error={query.error} />}
    {duplicatesOnly ? <>
      {query.isPending && <Empty>正在检查客户数据…</Empty>}
      {query.data?.rows.length === 0 && <Empty>暂无待核对的重复客户。</Empty>}
      {query.data && (selected || pagination) && <div className="review-layout quality-review-layout">
        <section className="panel review-list"><h3>潜在重复客户</h3>
          {query.data.rows.map(issue => <button key={issueKey(issue)} className="review-list-item"
            aria-pressed={issue === selected} onClick={() => selectIssue(issueKey(issue))}>
            <span className="review-item-icon"><CustomerIcon name="users" /></span>
            <span><strong>{issue.contact_name} / {issue.other_name}</strong><small>{descriptions[issue.detail]}</small></span>
          </button>)}{pagination}
        </section>{details}
      </div>}
    </> : <div className="ops-split crm-insight-grid" data-has-detail={Boolean(details)}>
      <section className="panel ops-table crm-insight-table quality-results">
        <div className="section-heading"><h2><ClipboardCheck aria-hidden="true" />检查结果</h2>
          <label className="followup-search"><Search aria-hidden="true" /><input type="search" maxLength={300}
            aria-label="搜索质量问题" placeholder="搜索客户姓名或公司" value={search}
            onChange={event => { setSearch(event.target.value); setOffset(0); selectIssue('') }} /></label>
        </div>
        <div className="quality-filters" aria-label="问题类型">
          {([['', '全部问题'], ...categories] as const).map(([value, label]) =>
            <button key={value} type="button" className="button" aria-pressed={category === value}
              onClick={() => { setCategory(value); setOffset(0); selectIssue('') }}>
              {label}{value && query.data && <span>{counts.get(value) ?? 0}</span>}
            </button>)}
        </div>
        {query.isPending && <Empty>正在检查客户数据…</Empty>}
        {query.data?.rows.length === 0 && <Empty>当前筛选下没有待核对问题。</Empty>}
        {query.data && query.data.rows.length > 0 && <div className="table-wrap"><table aria-label="客户质量检查结果">
          <thead><tr><th scope="col">客户与问题</th><th scope="col">问题类型</th><th scope="col">优先级</th><th scope="col">发现来源</th><th scope="col">操作</th></tr></thead>
          <tbody>{query.data.rows.map(issue => <tr key={issueKey(issue)} data-selected={issue === selected}>
            <td><strong>{issue.contact_name}{issue.other_name && ' / ' + issue.other_name}</strong><small>{descriptions[issue.detail]}</small></td>
            <td>{issueLabel(issue)}</td>
            <td><QualityPriority issue={issue} /></td>
            <td>{issueSource(issue)}</td>
            <td><button className="text-button" aria-label={`查看 ${issue.contact_name} 的${issueLabel(issue)}`} aria-pressed={issue === selected}
              onClick={() => selectIssue(issueKey(issue))}>查看</button></td>
          </tr>)}</tbody>
        </table></div>}
        {pagination}
      </section>{details}
    </div>}
  </section>
}

function QualityPriority({ issue }: { issue: QualityIssue }) {
  const high = isMailIssue(issue) || issue.detail === 'email'
  const low = issue.category === 'missing_market'
  return <OpsTag tone={high ? 'red' : low ? 'gray' : 'orange'}>{high ? '高' : low ? '低' : '中'}</OpsTag>
}

function QualityDetails({ issue, open, edit, openMail, compare }: {
  issue: QualityIssue; open: (id: string) => void; edit: (id: string, area: RepairArea) => void
  openMail: (id: string, draftId?: string) => void; compare: () => void
}) {
  const mail = isMailIssue(issue)
  const label = mail ? '查看相关邮件' : issue.other_id ? '打开重复客户审查'
    : ({ missing_email: '补充邮箱', invalid_email: '核对邮箱', invalid_website: '核对网址',
      date_order: '核对联系时间', sample_mismatch: '核对样品进度', missing_market: '补充地区',
    } as Record<string, string>)[issue.category] ?? '编辑客户资料'
  const act = () => {
    if (mail) return openMail(issue.contact_id, issue.draft_id ?? undefined)
    if (issue.other_id) return compare()
    const area = issue.category === 'missing_market' ? 'basic'
      : issue.category === 'sample_mismatch' || issue.category === 'date_order' ? 'business' : 'channels'
    edit(issue.contact_id, area)
  }
  return <aside className="panel crm-insight-context quality-finding" aria-label="问题详情">
    <div className="section-heading"><h2><ShieldCheck aria-hidden="true" />问题详情</h2><QualityPriority issue={issue} /></div>
    <h3>{issueLabel(issue)}</h3>
    <dl><div><dt>涉及客户</dt><dd>{issue.contact_name}{issue.other_name && ' / ' + issue.other_name}</dd></div>
      <div><dt>发现依据</dt><dd>{descriptions[issue.detail]}</dd></div>
      <div><dt>发现来源</dt><dd>{issueSource(issue)}</dd></div></dl>
    <div className={'quality-guidance' + (mail || issue.other_id ? ' crm-attention' : '')}><h3>处理建议</h3><p>{mail
      ? guidance[issue.category] ?? (issue.detail === 'delivery_unknown' ? '先核实原发件记录，再决定如何处理提交结果未知的邮件。' : '打开相关邮件，核对发送结果和错误原因。')
      : guidance[issue.category]}</p></div>
    <div className="row-actions"><button className="button primary" onClick={act}>
      {mail ? <Mail aria-hidden="true" /> : issue.other_id ? <CopyCheck aria-hidden="true" /> : <Pencil aria-hidden="true" />}{label}</button>
      <button className="text-button" onClick={() => open(issue.contact_id)}><ExternalLink aria-hidden="true" />客户资料</button></div>
  </aside>
}

function CompareCustomers({ ids, reason, open, merged }: {
  ids: [string, string]; reason: string; open: (id: string) => void; merged: (name: string) => void
}) {
  const [primary, setPrimary] = useState('')
  const [preview, setPreview] = useState(false)
  const cache = useQueryClient()
  const first = useQuery({ queryKey: ['contact',ids[0]], queryFn: () => request({ action: 'get_contact', id: ids[0] },'contact') })
  const second = useQuery({ queryKey: ['contact',ids[1]], queryFn: () => request({ action: 'get_contact', id: ids[1] },'contact') })
  const records = [first.data?.record, second.data?.record].filter((record): record is Contact => Boolean(record))
  const merge = useMutation({
    mutationFn: ({ keep, archive }: { keep: Contact; archive: Contact }) => {
      const target = (record: Contact) => ({ id: record.id, revision: record.record_revision, epoch: record.data_epoch })
      return request({ action: 'merge_contacts', primary: target(keep), secondary: target(archive) }, 'saved')
    },
    onSuccess: (_, { keep }) => {
      merged(keep.input.display_name)
      return Promise.all(['contacts','contact','contact-fields','activities','inbox','inbox-record','inbox-thread','mail-drafts','dashboard','audit']
        .map(key => cache.invalidateQueries({ queryKey: [key] })))
    },
  })
  const keep = records.find(record => record.id === primary)
  const archive = records.find(record => record.id !== primary)
  const comparison = [
    ['客户姓名', (record: Contact) => record.input.display_name],
    ['公司', (record: Contact) => record.input.company_name],
    ['国家或地区', (record: Contact) => record.input.country_region],
    ['邮箱', (record: Contact) => record.input.methods.filter(method => method.kind === 'email').map(method => method.value).join('、')],
    ['网站', (record: Contact) => record.input.website_url],
    ['阶段', (record: Contact) => stages[record.input.stage]],
    ['产品兴趣', (record: Contact) => record.input.interests.map(interest => interest.product_text).join('、')],
  ] as const
  return <section className="panel review-detail">
    <div className="section-heading"><h3>合并前核对</h3><span className="review-badge">{reason}</span></div>
    {(first.isError || second.isError) && <Problem error={first.error ?? second.error} />}
    {records.length !== 2 && !(first.isError || second.isError) && <Empty>正在读取两条客户记录…</Empty>}
    {records.length === 2 && <>
      <div className="table-wrap merge-comparison"><table aria-label="重复客户字段对比">
        <thead><tr><th scope="col">客户属性</th>{records.map((record, index) => <th key={record.id} scope="col" data-retained={record.id === primary}>
          <label><input type="radio" name="retained-contact" aria-label={`保留 ${record.input.display_name}`} checked={primary === record.id}
            disabled={record.archived_at !== null || merge.isPending} onChange={() => { setPrimary(record.id); setPreview(false) }} />保留记录 {index === 0 ? 'A' : 'B'}</label>
        </th>)}</tr></thead>
        <tbody>{comparison.map(([label, value]) => <tr key={label} data-different={value(records[0]) !== value(records[1])}>
          <th scope="row">{label}</th>{records.map(record => <td key={record.id}>{value(record) || '—'}</td>)}
        </tr>)}<tr><th scope="row">原始资料</th>{records.map(record => <td key={record.id}><button className="text-button" onClick={() => open(record.id)}><ExternalLink aria-hidden="true" />查看详情</button></td>)}</tr></tbody>
      </table></div>
      <p className="merge-warning"><TriangleAlert aria-hidden="true" />同一家公司可能有多位联系人。请核对身份后再选择保留记录。</p>
      {preview && keep && archive && <div className="review-preview merge-preview"><h3><GitMerge aria-hidden="true" />合并预览</h3>
        <p><strong>{keep.input.display_name}</strong> 保持当前阶段，将补充缺失属性并汇总联系方式、产品兴趣、邮件及沟通记录。</p>
        <p><strong>{archive.input.display_name}</strong> 将归档。原记录、自定义属性和审批历史可追溯。</p></div>}
    </>}
    {merge.isError && <Problem error={merge.error} />}
    {records.length === 2 && <div className="form-footer">{preview && keep && archive ?
      <ConfirmButton primary label={`保留「${keep.input.display_name}」，合并并归档「${archive.input.display_name}」？`}
        disabled={merge.isPending || records.some(record => record.archived_at !== null)}
        onConfirm={() => merge.mutate({ keep, archive })}><GitMerge aria-hidden="true" />合并并归档</ConfirmButton>
      : <button className="button primary" disabled={!keep} onClick={() => setPreview(true)}><ClipboardCheck aria-hidden="true" />查看合并预览</button>}
    </div>}
  </section>
}
