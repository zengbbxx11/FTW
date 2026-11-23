import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { outreachLabels, request, stages, type Contact, type QualityIssue } from './api'
import { ConfirmButton, Empty, Pagination, Problem } from './components'
import { CustomerIcon } from './CustomerIcon'

const categories = [
  ['duplicate', '可能重复'], ['missing_email', '缺少邮箱'], ['mail_problem', '发信异常'], ['missing_market', '待补充国家或地区'],
] as const
const descriptions: Record<string, string> = {
  email: '相同邮箱', domain: '相同网站域名', company_country: '公司与国家或地区相同',
  missing_email: '该客户尚未添加邮箱', missing_market: '国家或地区为空',
  failed: outreachLabels.failed, delivery_unknown: outreachLabels.delivery_unknown, bounced: outreachLabels.bounced,
}

export function CustomerQuality({ back, open, openMail }: { back: () => void; open: (id: string) => void; openMail: (id: string) => void }) {
  const [category, setCategory] = useState('')
  const [offset, setOffset] = useState(0)
  const [active, setActive] = useState<QualityIssue | null>(null)
  const [notice, setNotice] = useState('')
  const query = useQuery({
    queryKey: ['contacts', 'quality', category, offset],
    queryFn: () => request({ action: 'customer_quality', category, offset }, 'customer_quality'),
  })
  const counts = new Map(query.data?.counts)
  const total = Array.from(counts.values()).reduce((sum, count) => sum + count, 0)
  const selected = query.data?.rows.find(item => item.contact_id === active?.contact_id
    && item.other_id === active?.other_id && item.category === active?.category) ?? query.data?.rows[0]
  const select = (value: string) => { setCategory(value); setOffset(0); setActive(null); setNotice('') }
  return <>
    <header className="page-heading"><div><button className="text-button back-link" onClick={back}>返回客户管理</button>
      <h1>客户数据质量检查</h1><p>核对可能重复的客户，补全资料并处理发信异常。</p></div>
      <button className="button" disabled={query.isFetching} onClick={() => void query.refetch()}>重新检查</button>
    </header>
    {query.data && <div className="quality-metrics" aria-label="客户质量分类">
      <button aria-pressed={!category} onClick={() => select('')}><CustomerIcon name="grid" /><span><strong>{total}</strong>全部问题</span></button>
      {categories.map(([id, label]) => <button key={id} aria-pressed={category === id} onClick={() => select(id)}>
        <CustomerIcon name={id === 'duplicate' ? 'users' : id === 'missing_email' || id === 'mail_problem' ? 'mail' : 'contact'} />
        <span><strong>{counts.get(id) ?? 0}</strong>{label}</span>
      </button>)}
    </div>}
    {notice && <p className="review-notice" role="status">{notice}</p>}
    {query.isError && <Problem error={query.error} />}
    {query.isPending && <Empty>正在检查客户数据…</Empty>}
    {query.data?.rows.length === 0 && <Empty>当前没有这类待核对问题。</Empty>}
    {selected && <div className="review-layout">
      <section className="panel review-list"><h3>检查结果</h3>
        {query.data!.rows.map(issue => <button key={[issue.category,issue.contact_id,issue.other_id].join(':')}
          className="review-list-item" aria-pressed={issue === selected} onClick={() => { setActive(issue); setNotice('') }}>
          <span className="review-item-icon"><CustomerIcon name={issue.other_id ? 'users' : 'contact'} /></span>
          <span><strong>{issue.contact_name}{issue.other_name && ' / ' + issue.other_name}</strong><small>{descriptions[issue.detail]}</small></span>
          <span className="review-badge">{categories.find(([id]) => id === issue.category)?.[1]}</span>
        </button>)}
        <Pagination offset={offset} more={query.data!.has_more} change={next => { setOffset(next); setActive(null) }} />
      </section>
      {selected.other_id ? <CompareCustomers key={selected.contact_id + selected.other_id}
        ids={[selected.contact_id,selected.other_id]} reason={descriptions[selected.detail]} open={open}
        merged={name => { setNotice('客户已合并，保留 ' + name); setActive(null); if (query.data?.rows.length === 1 && offset) setOffset(offset - 20) }} />
        : <section className="panel review-detail"><div className="section-heading"><h3>{selected.contact_name}</h3>
          <span className="review-badge">{descriptions[selected.detail]}</span></div>
          {selected.category === 'mail_problem' && <p className="muted">从邮件往来核对发送结果。提交结果未知的邮件需要先核实发件记录。</p>}
          <div className="form-footer"><button className="button primary"
            onClick={() => selected.category === 'mail_problem' ? openMail(selected.contact_id) : open(selected.contact_id)}>
            {selected.category === 'mail_problem' ? '查看邮件往来' : '打开客户资料'}</button></div>
        </section>}
    </div>}
  </>
}

function CompareCustomers({ ids, reason, open, merged }: {
  ids: [string, string]; reason: string; open: (id: string) => void; merged: (name: string) => void
}) {
  const [primary, setPrimary] = useState('')
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
  return <section className="panel review-detail">
    <div className="section-heading"><h3>合并前核对</h3><span className="review-badge">{reason}</span></div>
    {(first.isError || second.isError) && <Problem error={first.error ?? second.error} />}
    {records.length !== 2 && !(first.isError || second.isError) && <Empty>正在读取两条客户记录…</Empty>}
    <div className="comparison-cards">{records.map(record => <article key={record.id}>
      <label className="merge-target"><input type="radio" name="retained-contact" checked={primary === record.id}
        disabled={record.archived_at !== null || merge.isPending} onChange={() => setPrimary(record.id)} />保留此客户</label>
      <h3>{record.input.display_name}</h3>
      <dl className="review-properties">
        <div><dt>公司</dt><dd>{record.input.company_name || '—'}</dd></div>
        <div><dt>国家或地区</dt><dd>{record.input.country_region || '—'}</dd></div>
        <div><dt>邮箱</dt><dd>{record.input.methods.filter(method => method.kind === 'email').map(method => method.value).join('、') || '—'}</dd></div>
        <div><dt>阶段</dt><dd>{stages[record.input.stage]}</dd></div>
        <div><dt>产品兴趣</dt><dd>{record.input.interests.map(interest => interest.product_text).join('、') || '—'}</dd></div>
      </dl><button className="text-button" onClick={() => open(record.id)}>查看完整资料</button>
    </article>)}</div>
    {records.length === 2 && <p className="review-preview">合并会保留所选客户的阶段和已有属性，将缺失属性、联系渠道、产品兴趣和历史沟通汇总到该客户。另一条记录归档，原有资料与审批历史可追溯。</p>}
    {merge.isError && <Problem error={merge.error} />}
    <div className="form-footer"><ConfirmButton primary label={keep && archive ? '保留「' + keep.input.display_name + '」，合并并归档「' + archive.input.display_name + '」？' : '请选择保留的客户。'}
      disabled={!keep || !archive || merge.isPending || records.some(record => record.archived_at !== null)}
      onConfirm={() => { if (keep && archive) merge.mutate({ keep, archive }) }}>合并并归档</ConfirmButton></div>
  </section>
}
