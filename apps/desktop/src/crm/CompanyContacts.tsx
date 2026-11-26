import { useDeferredValue, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Building2, ChevronRight, Globe, Link2, Mail, Pencil, Plus, Save, Search, ShieldCheck, UsersRound } from 'lucide-react'
import { OpsTag } from '../data/OpsUI'
import { dateLabel, outreachLabels, request, shortDate, stages, type CompanyProfile, type CompanyProfileInput, type Contact } from './api'
import { ConfirmButton, Empty, Field, Pagination, Problem } from './components'
import { ToolPopover, ToolPopoverClose } from '../ui/WorkbenchUI'
import type { CellTarget } from './viewTypes'
import { ContactEditor, useStateCatalog, stateLabel } from './Contacts'
import { FollowupCustomerPicker } from './Timeline'

function revision(record: Contact) {
  return { id: record.id, revision: record.record_revision, epoch: record.data_epoch }
}

const companyKey = (value: string) => value.replace(/[A-Z]/g, char => char.toLowerCase())
const companyTarget = ({ id, revision, epoch }: CompanyProfile): CellTarget => ({ id, revision, epoch })

export function CompanyContacts({ id, back, open, history }: {
  id: string; back: () => void; open: (id: string) => void; history: (id: string) => void
}) {
  const [offset, setOffset] = useState(0)
  const [candidate, setCandidate] = useState('')
  const [creating, setCreating] = useState<{ identity: { company_name: string; country_region: string }; profile?: CellTarget }>()
  const [notice, setNotice] = useState('')
  const [directoryPick, setDirectoryPick] = useState<CompanyProfile | null>(null)
  const [branch, setBranch] = useState('')
  const [branchEditor, setBranchEditor] = useState<{ contact: CellTarget; profile: CellTarget } | null>(null)
  const [editor, setEditor] = useState<{ anchor: CellTarget; profile: CompanyProfile | null } | null>(null)
  const cache = useQueryClient()
  const catalog = useStateCatalog()
  const anchor = useQuery({ queryKey: ['contact', id], queryFn: () => request({ action: 'get_contact', id }, 'contact') })
  const members = useQuery({ queryKey: ['contacts', 'company', id, offset], queryFn: () => request({ action: 'company_contacts', id, offset }, 'company_contacts') })
  const target = useQuery({ queryKey: ['contact', candidate], enabled: Boolean(candidate),
    queryFn: () => request({ action: 'get_contact', id: candidate }, 'contact') })
  const refresh = () => Promise.all(['contact', 'contacts', 'dashboard', 'audit'].map(key => cache.invalidateQueries({ queryKey: [key] })))
  const profileSave = useMutation({
    mutationFn: ({ anchor, profile, input, include_group }: { anchor: CellTarget; profile: CellTarget | null; input: CompanyProfileInput; include_group: boolean }) =>
      request({ action: 'save_company_profile', anchor, profile, input, include_group }, 'saved'),
    onSuccess: () => { setEditor(null); setOffset(0); setNotice('已保存独立公司档案。'); return refresh() },
  })
  const membership = useMutation({
    mutationFn: ({ contact, profile, branch }: { contact: CellTarget; profile: CellTarget | null; branch: string }) =>
      request({ action: 'set_company_membership', contact, profile, branch }, 'saved'),
    onSuccess: (_, result) => {
      setCandidate(''); setDirectoryPick(null); setBranchEditor(null); setOffset(0); setBranch('')
      setNotice(result.profile ? '已保存联系人所属公司和分支。' : '已解除当前联系人的公司关联。')
      return refresh()
    },
  })
  const associate = useMutation({
    mutationFn: ({ company, member }: { company: Contact; member: Contact }) =>
      request({ action: 'associate_company', anchor: revision(company), contact: revision(member) }, 'saved'),
    onSuccess: (_, { member }) => {
      setCandidate('')
      setOffset(0)
      setNotice('已核对「' + member.input.display_name + '」的公司归属。')
      return refresh()
    },
  })
  const record = anchor.data?.record
  if (!record) return <section className="ops-workspace company-workspace">
    <button className="text-button back-link" onClick={back}><ArrowLeft aria-hidden="true" />返回客户管理</button>
    {anchor.isError ? <Problem error={anchor.error} /> : <Empty>正在读取公司联系人…</Empty>}
  </section>
  const { company_name, country_region, website_url, customer_type } = record.input
  const member = target.data?.record
  const profile = members.data?.profile ?? null
  const editorChanged = Boolean(editor && (editor.anchor.revision !== record.record_revision || editor.anchor.epoch !== record.data_epoch
    || (members.data && (editor.profile?.id !== profile?.id || editor.profile?.revision !== profile?.revision || editor.profile?.epoch !== profile?.epoch))))
  const complete = Boolean(company_name && country_region)
  const active = record.archived_at === null
  const writable = Boolean(active && (profile || complete))
  const existingMember = member && companyKey(member.input.company_name) === companyKey(company_name)
    && companyKey(member.input.country_region) === companyKey(country_region)
  if (creating) return <ContactEditor initialCompany={creating.identity} companyProfile={creating.profile}
    cancel={() => setCreating(undefined)} saved={() => { setCreating(undefined); setOffset(0); setNotice('已新增公司联系人。') }} />

  return <section className="ops-workspace company-workspace">
    <header className="page-heading"><div><button className="text-button back-link" onClick={back}><ArrowLeft aria-hidden="true" />返回客户管理</button>
      <h1>公司与多联系人</h1><p>在同一家公司中查看各联系人的业务阶段、邮件状态与最近联系。</p></div>
      <div className="row-actions">
        <FollowupCustomerPicker primary={false} icon={Link2} label="关联现有联系人" title="选择要关联的联系人" disabled={!writable || associate.isPending || membership.isPending}
          select={next => { associate.reset(); membership.reset(); setBranch(''); setDirectoryPick(null); setCandidate(next); setNotice('') }} />
        <button className="button primary" type="button" disabled={!writable || associate.isPending || membership.isPending} onClick={() => setCreating({
          identity: { company_name: profile?.name ?? company_name, country_region }, profile: profile ? companyTarget(profile) : undefined,
        })}><Plus aria-hidden="true" />新增联系人</button>
      </div>
    </header>
    <section className="panel company-banner" aria-label="当前公司">
      <span className="dashboard-symbol" data-tone="teal"><Building2 aria-hidden="true" /></span>
      <div><h2>{profile?.name ?? (company_name || '待补充公司名称')}</h2><p><Globe aria-hidden="true" />{country_region || '待补充国家或地区'}
        {members.data?.branch && <><span aria-hidden="true">·</span>{members.data.branch}</>}
        {customer_type && <><span aria-hidden="true">·</span>{stateLabel(catalog, 'customer_type', customer_type)}</>}</p></div>
      <div className="company-banner-status">{record.archived_at !== null && <OpsTag tone="gray">当前客户已归档</OpsTag>}
        {members.data && <OpsTag tone="teal">{members.data.total} 位在用联系人</OpsTag>}
        <button className="text-button" onClick={() => open(id)}>查看当前客户<ChevronRight aria-hidden="true" /></button></div>
    </section>
    {!complete && !profile && <p className="relation-notice">可以创建公司档案，或按公司名称与别名查找已有档案。</p>}
    {notice && <p className="relation-notice" role="status">{notice}</p>}
    {candidate && <section className="panel company-confirm" aria-label="核对联系人关联">
      <div><h2>核对联系人关联</h2>
        {target.isPending && <p className="muted">正在读取联系人…</p>}
        {target.isError && <Problem error={target.error} />}
        {member && <p><strong>{member.input.display_name}</strong> · {[member.input.company_name, member.input.country_region].filter(Boolean).join(' / ') || '公司与地区均待补充'}
          {!profile && existingMember && ' · 已归属当前公司'}</p>}
        {profile && member && <Field label="联系人分支"><input maxLength={150} value={branch} onChange={event => setBranch(event.target.value)} /></Field>}
        {associate.isError && <Problem error={associate.error} />}
        {membership.isError && <Problem error={membership.error} />}
      </div><div className="row-actions">
        <button type="button" className="button" disabled={associate.isPending} onClick={() => { setCandidate(''); associate.reset() }}>关闭</button>
        {member && <ConfirmButton key={JSON.stringify([revision(record), revision(member), profile && companyTarget(profile)])} primary disabled={!writable || (profile ? member.id === id : existingMember) || Boolean(member.archived_at) || associate.isPending || membership.isPending}
          label={profile ? `将「${member.input.display_name}」关联到公司档案「${profile.name}」？` : `补齐「${member.input.display_name}」的公司和地区，并关联到「${company_name}」？`}
          onConfirm={() => profile ? membership.mutate({ contact: revision(member), profile: companyTarget(profile), branch })
            : associate.mutate({ company: record, member })}><Link2 aria-hidden="true" />确认关联</ConfirmButton>}
      </div>
    </section>}
    <div className="ops-split crm-insight-grid">
      <section className="panel ops-table crm-insight-table">
        <div className="section-heading"><h2><UsersRound aria-hidden="true" />公司联系人</h2>
          {members.data && <span className="muted">{members.data.total} 位联系人</span>}</div>
        {members.isError && <Problem error={members.error} />}
        {members.isPending && <Empty>正在读取联系人…</Empty>}
        {members.data && !members.data.rows.length && <Empty>当前暂无在用联系人。</Empty>}
        {members.data && members.data.rows.length > 0 && <div className="table-wrap"><table aria-label="公司联系人列表">
          <thead><tr><th scope="col">联系人</th>{profile && <th scope="col">地区 / 分支</th>}<th scope="col">邮箱</th><th scope="col">客户阶段</th><th scope="col">邮件开发状态</th><th scope="col">最近联系</th><th scope="col">操作</th></tr></thead>
          <tbody>{members.data.rows.map(row => <tr key={row.id} data-selected={row.id === id}>
            <td><div className="company-contact-cell"><span className="company-initial" aria-hidden="true">{Array.from(row.display_name)[0]?.toUpperCase()}</span><div>
              <button className="text-button" onClick={() => open(row.id)}>{row.display_name}</button></div></div></td>
            {profile && <td>{row.country_region || '—'}{row.branch && <small>{row.branch}</small>}</td>}
            <td><span className="company-email" title={row.email ?? undefined}>{row.email || '待补充'}</span></td>
            <td><span className="stage-tag" data-stage={row.stage}>{stages[row.stage]}</span></td>
            <td><span className="customer-status" data-status={row.mail_status}>{outreachLabels[row.mail_status]}</span></td>
            <td><span className="company-contact-date" title={dateLabel(row.last_contact_at)}>{shortDate(row.last_contact_at)}</span></td>
            <td><button className="text-button" title={'查看 ' + row.display_name + ' 的邮件往来'} onClick={() => history(row.id)}>
              <Mail aria-hidden="true" />邮件</button></td>
          </tr>)}</tbody>
        </table></div>}
        {members.data && (offset > 0 || members.data.has_more) && <Pagination offset={offset} more={members.data.has_more} change={setOffset} />}
      </section>
        <section className="panel crm-insight-context">
          <h2><Building2 aria-hidden="true" />{profile ? '独立公司档案' : '公司资料与开发提醒'}</h2>
          <dl><div><dt>公司或店铺</dt><dd>{profile?.name ?? (company_name || '待补充')}</dd></div>
            <div><dt>{profile ? '联系人地区' : '国家或地区'}</dt><dd>{country_region || '待补充'}</dd></div>
            <div><dt>网站</dt><dd>{(profile ? profile.website : website_url) || '待补充'}</dd></div>
            <div><dt>客户类型</dt><dd>{customer_type ? stateLabel(catalog, 'customer_type', customer_type) : '待补充'}</dd></div>
            {profile && <><div><dt>公司别名</dt><dd>{profile.aliases.join('、') || '—'}</dd></div>
              <div><dt>当前分支</dt><dd>{members.data?.branch || '—'}</dd></div>
              {profile.notes && <div><dt>公司备注</dt><dd className="long-text">{profile.notes}</dd></div>}</>}
            {members.data && <div><dt>在用联系人</dt><dd>{members.data.total} 位</dd></div>}</dl>
          {!profile && <p className="muted">资料来自当前联系人「{record.input.display_name}」。</p>}
          {active && members.data && !editor && <div className="row-actions company-profile-actions">
            <button className="button" onClick={() => {
              profileSave.reset(); setDirectoryPick(null); setBranchEditor(null); setEditor({ anchor: revision(record), profile })
            }}><Pencil aria-hidden="true" />{profile ? '编辑公司档案' : '创建公司档案'}</button>
            {!profile && <CompanyDirectory select={item => { membership.reset(); setCandidate(''); setBranch(''); setDirectoryPick(item) }} />}
            {profile && <button className="button" onClick={() => {
              membership.reset(); setCandidate(''); setBranch(members.data?.branch ?? ''); setBranchEditor({ contact: revision(record), profile: companyTarget(profile) })
            }}><Link2 aria-hidden="true" />编辑当前分支</button>}
          </div>}
          {editor && <CompanyProfileEditor current={editor.profile} fallback={{ name: company_name, website: website_url, notes: '', aliases: [] }}
            groupCount={members.data?.total ?? 0} busy={profileSave.isPending} changed={editorChanged} cancel={() => setEditor(null)}
            save={(input, include_group) => profileSave.mutate({ anchor: editor.anchor, profile: editor.profile ? companyTarget(editor.profile) : null, input, include_group })} />}
          {profileSave.isError && <Problem error={profileSave.error} />}
          {directoryPick && <div className="quality-guidance"><h3>关联到 {directoryPick.name}</h3>
            <Field label="当前联系人分支"><input value={branch} maxLength={150} onChange={event => setBranch(event.target.value)} /></Field>
            <div className="row-actions company-profile-actions">
              <button className="button" onClick={() => { setDirectoryPick(null); membership.reset() }}>取消</button>
              <ConfirmButton key={JSON.stringify(revision(record))} primary label={`将当前联系人关联到「${directoryPick.name}」？`} disabled={membership.isPending}
                onConfirm={() => membership.mutate({ contact: revision(record), profile: companyTarget(directoryPick), branch })}>确认选择档案</ConfirmButton>
            </div>
          </div>}
          {branchEditor && <div className="quality-guidance">
            <Field label="当前联系人分支"><input value={branch} maxLength={150} onChange={event => setBranch(event.target.value)} /></Field>
            <div className="row-actions company-profile-actions">
              <button className="button" onClick={() => { setBranchEditor(null); membership.reset() }}>取消</button>
              <button className="button primary" disabled={membership.isPending}
                onClick={() => membership.mutate({ ...branchEditor, branch })}><Save aria-hidden="true" />保存分支</button>
              <ConfirmButton label="解除当前联系人与公司档案的关联？" disabled={membership.isPending}
                onConfirm={() => membership.mutate({ contact: branchEditor.contact, profile: null, branch: '' })}>解除关联</ConfirmButton>
            </div>
          </div>}
          {!candidate && membership.isError && <Problem error={membership.error} />}
          <div className="quality-guidance crm-attention">
            <h3><ShieldCheck aria-hidden="true" />邮件开发提醒</h3>
            {members.data && <p><strong>{members.data.contacted}</strong> 位联系人已有开发发信记录。</p>}
            <p>写信前可先查看同公司的邮件往来，再决定是否联系另一位联系人。</p>
          </div>
        </section>
    </div>
  </section>
}

function CompanyProfileEditor({ current, fallback, groupCount, busy, changed, cancel, save }: {
  current: CompanyProfile | null; fallback: CompanyProfileInput; groupCount: number; busy: boolean; changed: boolean
  cancel: () => void; save: (input: CompanyProfileInput, includeGroup: boolean) => void
}) {
  const [draft, setDraft] = useState(current ?? fallback)
  const [aliases, setAliases] = useState((current ?? fallback).aliases.join('\n'))
  const [group, setGroup] = useState(false)
  return <form className="company-profile-form" onSubmit={event => {
    event.preventDefault()
    if (busy || changed) return
    save({ name: draft.name, website: draft.website, notes: draft.notes, aliases: aliases.split('\n').map(value => value.trim()).filter(Boolean) }, group)
  }}>
    {changed && <p role="status" className="alert warning">公司资料已有新版本。当前输入已保留，可复制所需内容后重新打开编辑。</p>}
    <Field label="公司档案名称"><input name="name" required maxLength={300} value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></Field>
    <Field label="公司网址"><input type="url" maxLength={1000} value={draft.website} onChange={event => setDraft({ ...draft, website: event.target.value })} /></Field>
    <Field label="公司别名（每行一个）"><textarea name="aliases" rows={3} value={aliases} onChange={event => setAliases(event.target.value)} /></Field>
    <Field label="独立公司备注"><textarea name="notes" rows={3} maxLength={4000} value={draft.notes} onChange={event => setDraft({ ...draft, notes: event.target.value })} /></Field>
    {!current && groupCount > 1 && <label><input type="checkbox" checked={group} onChange={event => setGroup(event.target.checked)} /> 一并关联原同名同地区列表中的联系人</label>}
    <div className="row-actions"><button className="button" type="button" disabled={busy} onClick={cancel}>取消</button>
      <button className="button primary" type="submit" disabled={busy || changed}><Save aria-hidden="true" />保存公司档案</button></div>
  </form>
}

function CompanyDirectory({ select }: { select: (item: CompanyProfile) => void }) {
  const [text, setText] = useState('')
  const keyword = useDeferredValue(text)
  const results = useQuery({ queryKey: ['contacts', 'company-directory', keyword],
    queryFn: () => request({ action: 'find_company_profiles', keyword }, 'company_profiles') })
  return <ToolPopover label="选择公司档案" trigger={<button className="button"><Search aria-hidden="true" />查找公司档案</button>}>
    <label className="followup-search"><Search aria-hidden="true" /><input type="search" aria-label="查找公司名称或别名" maxLength={300}
      value={text} onChange={event => setText(event.target.value)} /></label>
    {results.isError && <Problem error={results.error} />}
    {results.isPending && <Empty>正在读取公司档案…</Empty>}
    {results.data?.rows.map(item => <ToolPopoverClose key={item.id} asChild><button type="button" className="followup-picker-item"
      onClick={() => select(item)}><strong>{item.name}</strong>{item.aliases.length > 0 && <small>{item.aliases.join('、')}</small>}</button></ToolPopoverClose>)}
    {results.data?.rows.length === 0 && <Empty>没有匹配的公司档案。</Empty>}
    {results.data?.has_more && <p className="muted">输入更多关键词以缩小匹配范围。</p>}
  </ToolPopover>
}
