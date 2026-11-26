import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, FileText, Image, Mail, Paperclip, Search, Upload, UserRound } from 'lucide-react'
import { dateLabel, request } from '../crm/api'
import { Empty, Pagination } from '../crm/components'
import { FollowupCustomerPicker } from '../crm/Timeline'
import { exportCustomerDocument, importCustomerDocuments, mailCall, mailError, type CustomerMailFile } from '../mail/api'
import { AttachmentActions } from '../mail/AttachmentActions'

const directions = [['', '全部文件'], ['document', '客户资料'], ['inbox', '来信附件'], ['outbound', '发件附件']] as const

function fileKey(row: CustomerMailFile) { return row.document_id ?? `${row.origin}:${row.message_id}:${row.index}` }
function originLabel(origin: CustomerMailFile['origin']) { return directions.find(([key]) => key === origin)?.[1] ?? origin }
function fileSize(size: number | null) {
  if (size === null) return '—'
  if (size >= 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`
  return size >= 1024 ? `${(size / 1024).toFixed(1)} KB` : `${size} 字节`
}

export function CustomerFiles({ contactId, initialDocument, openContact, openMail }: {
  contactId?: string; initialDocument?: string; openContact: (id: string) => void; openMail: (file: CustomerMailFile) => void
}) {
  const [input, setInput] = useState('')
  const keyword = useDeferredValue(input.trim())
  const [direction, setDirection] = useState('')
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState('')
  const [focused, setFocused] = useState(initialDocument)
  const [notice, setNotice] = useState('')
  const cache = useQueryClient()
  const query = useQuery({ queryKey: ['customer-files', contactId, keyword, direction, offset, focused],
    queryFn: () => mailCall({ action: 'file_workspace', contact_id: contactId ?? null, keyword, direction, offset, document_id: focused }, 'file_workspace') })
  const contact = useQuery({ queryKey: ['contact', contactId], enabled: Boolean(contactId),
    queryFn: () => request({ action: 'get_contact', id: contactId! }, 'contact') })
  const upload = useMutation({ mutationFn: importCustomerDocuments, onMutate: () => setNotice(''), onSuccess: async count => {
    if (count === null) return
    setDirection('document'); setInput(''); setOffset(0); setSelection(''); setFocused(undefined)
    await cache.invalidateQueries({ queryKey: ['customer-files'] })
    setNotice(count > 0 ? `已导入 ${count} 个客户文件。` : '选中的文件已关联到这位客户。')
  } })
  const download = useMutation({ mutationFn: ({ id, name }: { id: string; name: string }) => exportCustomerDocument(id, name),
    onMutate: () => setNotice(''), onSuccess: saved => { if (saved) setNotice('客户文件已导出。') } })
  const busy = upload.isPending || download.isPending
  const rows = query.data?.rows ?? []
  const file = rows.find(row => fileKey(row) === selection) ?? rows[0]
  return <section className="file-workspace" aria-label="客户文件与邮件附件">
    <div className="section-heading"><div><h2>客户文件与邮件附件</h2><p className="muted">{contact.data ? `${contact.data.record.input.display_name} · ` : ''}按客户查找资料和邮件附件。</p></div>
      <div className="row-actions">
        {focused && <button type="button" className="button" onClick={() => setFocused(undefined)}><Paperclip aria-hidden="true" />查看客户全部文件</button>}
        {contactId ? <>
          <button className="button" onClick={() => openContact(contactId)}><UserRound aria-hidden="true" />返回客户</button>
          <button className="button primary" disabled={busy || !contact.data || contact.data.record.archived_at !== null} onClick={() => upload.mutate(contactId)}><Upload aria-hidden="true" />导入本地文件</button>
        </> : <FollowupCustomerPicker label="导入本地文件" title="选择文件所属客户" icon={Upload} disabled={busy} select={id => upload.mutate(id)} />}
      </div></div>
    {!focused && <><div className="account-settings-tabs" role="group" aria-label="文件来源">{directions.map(([value, label]) =>
      <button type="button" className="button" key={value} aria-pressed={direction === value} onClick={() => { setDirection(value); setOffset(0); setSelection('') }}>{label}</button>)}
    </div>
    <div className="sample-toolbar">
      <label className="followup-search"><Search aria-hidden="true" /><input type="search" aria-label="搜索客户附件" maxLength={320}
        placeholder="搜索文件名、客户或邮件主题" value={input} onChange={event => { setInput(event.target.value); setOffset(0); setSelection('') }} /></label>
    </div></>}
    {notice && <p className="review-notice" role="status">{notice}</p>}
    {(upload.isError || download.isError || contact.isError) && <p className="alert error" role="alert">{mailError(upload.error ?? download.error ?? contact.error)}</p>}
    {query.isError && <p className="alert error" role="alert">{mailError(query.error)}</p>}
    {query.isPending && <Empty>正在读取附件…</Empty>}
    {query.data && <div className="sample-layout file-layout" data-selected={Boolean(file)}>
      <section className="panel sample-list"><h3>已关联文件</h3>
        {rows.length ? <div className="table-wrap"><table aria-label="客户附件列表"><thead><tr><th>文件名</th><th>关联客户</th><th>来源</th><th>大小</th><th>记录时间</th></tr></thead>
          <tbody>{rows.map(row => <tr key={fileKey(row)} data-selected={file === row}>
            <td><button type="button" className="text-button" aria-pressed={file === row} onClick={() => setSelection(fileKey(row))}><Paperclip aria-hidden="true" />{row.filename}</button></td>
            <td>{row.contact_name}</td><td>{originLabel(row.origin)}</td><td>{fileSize(row.size)}</td><td>{dateLabel(row.occurred_at)}</td>
          </tr>)}</tbody></table></div> : <Empty>当前没有符合条件的附件。</Empty>}
        <Pagination offset={offset} more={query.data.has_more} change={next => { setOffset(next); setSelection('') }} />
      </section>
      {file && <aside className="panel file-detail">
        <div className="file-symbol"><FileText aria-hidden="true" /></div>
        <h3>{file.filename}</h3>
        <dl className="review-properties">
          <div><dt>关联客户</dt><dd>{file.contact_name}</dd></div><div><dt>来源</dt><dd>{originLabel(file.origin)}</dd></div>
          {file.source && <div><dt>导入主体</dt><dd>{file.source === 'agent' ? file.created_by_name || 'Agent' : '工作台用户'}</dd></div>}
          {file.origin !== 'document' && <div><dt>邮件主题</dt><dd>{file.subject || '（无主题）'}</dd></div>}
          {file.content_type && <div><dt>文件类型</dt><dd>{file.content_type}</dd></div>}
          {file.size !== null && <div><dt>大小</dt><dd>{fileSize(file.size)}</dd></div>}
          {file.sha256 && <div><dt>SHA-256</dt><dd><code>{file.sha256}</code></dd></div>}
        </dl>
        {file.document_id ? <>
          {file.content_type && ['image/png', 'image/jpeg', 'image/webp'].includes(file.content_type) &&
            <PhotoPreview key={file.document_id} id={file.document_id} mime={file.content_type} name={file.filename} />}
          <button className="button primary" disabled={busy} onClick={() => download.mutate({ id: file.document_id!, name: file.filename })}>
            <Download aria-hidden="true" />另存文件</button>
        </> : <>
          <AttachmentActions key={fileKey(file)} source={file.origin === 'inbox' ? 'inbox' : 'outbound'}
            id={file.message_id} index={file.index} filename={file.filename} />
          <button className="button primary" onClick={() => openMail(file)}><Mail aria-hidden="true" />打开关联邮件</button>
        </>}
      </aside>}
    </div>}
  </section>
}

function PhotoPreview({ id, mime, name }: { id: string; mime: string; name: string }) {
  const [source, setSource] = useState('')
  const preview = useMutation({ mutationFn: async () => {
    const parts: string[] = []
    let offset: number | null = 0
    while (offset !== null) {
      const chunk: { next_offset: number | null; content_base64: string } =
        await mailCall({ action: 'preview_customer_image', id, offset }, 'document_chunk')
      if (chunk.next_offset !== null && chunk.next_offset <= offset) throw new Error('图片读取位置无效。')
      parts.push(chunk.content_base64)
      offset = chunk.next_offset
    }
    return `data:${mime};base64,${parts.join('')}`
  }, onSuccess: setSource })
  return <div className="customer-file-preview">
    {source ? <img src={source} alt={name} /> :
      <button type="button" className="button" disabled={preview.isPending} onClick={() => preview.mutate()}>
        <Image aria-hidden="true" />{preview.isPending ? '正在读取图片…' : '预览图片'}</button>}
    {preview.isError && <p className="alert error" role="alert">{mailError(preview.error)}</p>}
  </div>
}
