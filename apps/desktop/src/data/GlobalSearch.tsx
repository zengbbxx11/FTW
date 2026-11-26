import { useEffect, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowRight, FileText, History, Mail, Search, UserRound } from 'lucide-react'
import { dateLabel, outreachLabels, request, stages } from '../crm/api'
import { Empty, Pagination } from '../crm/components'
import { DataError } from './DataTools'
import { send, type SearchCategory, type SearchHit, type SearchTarget } from './api'

const categories = { all: '全部结果', customer: '客户', mail: '邮件', activity: '跟进记录', file: '文件' } as const
const icons = { customer: UserRound, mail: Mail, activity: History, file: FileText }

function hitKey(row: SearchHit) {
  const { target } = row
  return `${target.record_type}:${target.id}:${'index' in target ? target.index : ''}`
}

function hitTitle(row: SearchHit) { return row.title || '（无主题）' }

export function GlobalSearch({ keyword, setKeyword, focus, open }: {
  keyword: string; setKeyword: (keyword: string) => void; focus: number; open: (target: SearchTarget) => void
}) {
  const inputNode = useRef<HTMLInputElement>(null)
  const [input, setInput] = useState(keyword)
  const [category, setCategory] = useState<SearchCategory>('all')
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState('')
  useEffect(() => { inputNode.current?.focus(); inputNode.current?.select() }, [focus])

  const query = useQuery({ queryKey: ['global-search', keyword, category, offset], enabled: Boolean(keyword),
    queryFn: () => send({ action: 'search', keyword, category, offset }, 'search') })
  const rows = query.data?.rows ?? []
  const result = rows.find(row => hitKey(row) === selection) ?? rows[0]
  function submit() {
    setKeyword(input.trim())
    setCategory('all')
    setOffset(0)
    setSelection('')
  }

  return <div className="global-search">
    <header className="page-heading"><div><h1>全局搜索</h1><p>查找客户、邮件、跟进记录和关联文件</p></div></header>
    <form className="panel global-search-form" onSubmit={event => { event.preventDefault(); submit() }}>
      <Search aria-hidden="true" /><input ref={inputNode} type="search" aria-label="全局搜索关键词" placeholder="输入客户名称、邮件主题或文件名"
        maxLength={160} value={input} onChange={event => setInput(event.target.value)} />
      <kbd aria-hidden="true">Ctrl + K</kbd>
      <button type="submit" className="button primary" disabled={!keyword && !input.trim()}><Search aria-hidden="true" />搜索</button>
    </form>
    {query.isError && <DataError error={query.error} />}
    {!keyword && <section className="panel"><Empty>输入关键词后，按 Enter 搜索。</Empty></section>}
    {keyword && query.isPending && <section className="panel"><Empty>正在搜索工作台…</Empty></section>}
    {query.data && <>
      <div className="row-actions global-search-tabs" role="group" aria-label="搜索结果类别">
        {(Object.keys(categories) as SearchCategory[]).map(key => <button key={key} type="button" className="button" aria-pressed={category === key}
          onClick={() => { setCategory(key); setOffset(0); setSelection('') }}>{categories[key]} <span>{query.data.counts[key]}</span></button>)}
      </div>
      <div className="global-search-layout" data-preview={Boolean(result)}>
        <section className="panel global-search-results" aria-label="搜索结果">
          <div className="section-heading"><h2>搜索结果</h2><small>按相关性排序</small></div>
          {rows.length ? <ul>{rows.map(row => {
            const Icon = icons[row.category]
            return <li key={hitKey(row)}><button type="button" aria-pressed={result === row} onClick={() => setSelection(hitKey(row))}
              onDoubleClick={() => open(row.target)}>
              <span className="dashboard-symbol" data-tone="teal"><Icon aria-hidden="true" /></span>
              <span className="search-result-title"><strong>{hitTitle(row)}</strong><small>{categories[row.category]}{row.detail && ` · ${row.detail}`}</small></span>
              <time>{dateLabel(row.occurred_at)}</time>
            </button></li>
          })}</ul> : <Empty>没有找到和「{keyword}」匹配的{category === 'all' ? '记录' : categories[category]}。</Empty>}
          <Pagination offset={offset} more={query.data.counts[category] > offset + 20} change={next => { setOffset(next); setSelection('') }} />
        </section>
        {result && <Preview key={hitKey(result)} result={result} open={() => open(result.target)} />}
      </div>
    </>}
  </div>
}

function Preview({ result, open }: { result: SearchHit; open: () => void }) {
  const id = result.target.record_type === 'customer' ? result.target.id : null
  const query = useQuery({ queryKey: ['contact', id], enabled: Boolean(id),
    queryFn: () => request({ action: 'get_contact', id: id! }, 'contact') })
  const contact = query.data?.record
  const label = { customer: '打开客户详情', activity: '打开跟进记录', mail: '打开邮件',
    file: result.target.record_type === 'document' ? '打开客户文件' : '打开关联邮件' }[result.category]
  return <aside className="panel global-search-preview" aria-label="搜索结果快速预览">
    <div className="section-heading"><h2>快速预览</h2><span className="global-search-badge">{categories[result.category]}</span></div>
    <h3>{hitTitle(result)}</h3>
    <dl className="review-properties">
      {contact ? <>
        {contact.input.company_name && <div><dt>公司</dt><dd>{contact.input.company_name}</dd></div>}
        {contact.input.country_region && <div><dt>国家或地区</dt><dd>{contact.input.country_region}</dd></div>}
        <div><dt>客户阶段</dt><dd>{stages[contact.input.stage as keyof typeof stages] ?? contact.input.stage}</dd></div>
        <div><dt>邮件开发状态</dt><dd>{outreachLabels[contact.mail_outreach.status]}</dd></div>
        <div><dt>最近联系</dt><dd>{dateLabel(contact.effective_last_contact_at)}</dd></div>
      </> : result.detail && <div><dt>{result.category === 'mail' ? '邮箱' : '关联资料'}</dt><dd>{result.detail}</dd></div>}
      <div><dt>{result.category === 'customer' ? '更新时间' : '记录时间'}</dt><dd>{dateLabel(result.occurred_at)}</dd></div>
    </dl>
    {result.preview && <p className="global-search-excerpt">{result.preview}</p>}
    <button type="button" className="button primary" onClick={open}><ArrowRight aria-hidden="true" />{label}</button>
  </aside>
}
