import assert from 'node:assert/strict'
import { execFile, spawn } from 'node:child_process'
import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { copyFile, mkdir, mkdtemp, readFile, rename, unlink, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { DatabaseSync } from 'node:sqlite'
import { setTimeout as delay } from 'node:timers/promises'
import { promisify } from 'node:util'

// Accept the local Windows review harness built against the current Tauri sources.
const harness = resolve(process.argv[2] ?? '.ftw-dev/ui-native-harness/checked-after.exe')
const target = resolve(process.argv[3] ?? 'target/debug')
await mkdir('.ftw-dev', { recursive: true })
const directory = await mkdtemp(resolve('.ftw-dev/parity-'))
const profile = join(directory, 'profile')
const review = join(profile, 'review')
await mkdir(review, { recursive: true })
await copyFile(harness, join(directory, 'workbench-review.exe'))
for (const name of ['ftw-core.exe', 'ftw-mcp.exe']) await copyFile(join(target, name), join(directory, name))

const env = { ...process.env }
delete env.FTW_DEV_ROOT
delete env.FTW_AGENT_TOKEN
const app = spawn(join(directory, 'workbench-review.exe'), [], { cwd: directory, env, windowsHide: true, stdio: 'ignore' })
let commandNumber = 0

async function eventually(read, label, timeout = 10000) {
  const deadline = Date.now() + timeout
  let last
  do {
    try {
      const result = await read()
      if (result) return result
    } catch (error) { last = error }
    await delay(120)
  } while (Date.now() < deadline)
  throw new Error(label, { cause: last })
}

async function evaluate(expression) {
  const sequence = ++commandNumber
  const filename = join(review, 'command-' + sequence + '.json')
  await writeFile(filename + '.tmp', JSON.stringify({ method: 'Runtime.evaluate',
    params: { expression, awaitPromise: true, returnByValue: true } }))
  await rename(filename + '.tmp', filename)
  try {
    const result = await eventually(async () => JSON.parse(await readFile(join(review, 'result-' + sequence + '.json'), 'utf8')),
      'Timed out waiting for the Windows WebView')
    assert.ok(!result.review_error && !result.exceptionDetails, JSON.stringify(result))
    return result.result?.value
  } finally {
    await unlink(filename)
  }
}

async function visible(text, selector = 'main') {
  const expression = 'Array.from(document.querySelectorAll(' + JSON.stringify(selector) +
    ')).some(element => (element.innerText + " " + (element.getAttribute("aria-label") ?? "")).includes(' + JSON.stringify(text) + '))'
  await eventually(() => evaluate(expression), 'Missing rendered text: ' + text)
}

async function click(selector, label) {
  const expression = '(() => { const elements = Array.from(document.querySelectorAll(' + JSON.stringify(selector) +
    ')); const label = ' + JSON.stringify(label) + '; const node = elements.find(element => ' +
    'element.innerText.trim() === label || element.getAttribute("aria-label") === label) ?? ' +
    'elements.find(element => element.innerText.split("\\n").some(line => line.trim() === label)); ' +
    'if (!node) return false; (node.closest("button") ?? node).click(); return true; })()'
  await visible(label, selector)
  assert.ok(await evaluate(expression), 'No clickable element with label: ' + label)
}

async function fillForm(selector, values) {
  for (const [field, value] of Object.entries(values)) {
    await evaluate('(() => { const input = document.querySelector(' + JSON.stringify(selector) + ').elements.namedItem(' +
      JSON.stringify(field) + '); Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), "value").set.call(input, ' +
      JSON.stringify(value) + '); input.dispatchEvent(new Event("input", { bubbles: true })); })()')
  }
}

async function desktop(service, command, error) {
  const invocation = 'window.__TAURI_INTERNALS__.invoke(' + JSON.stringify(service + '_request') +
    ', ' + JSON.stringify({ command }) + ').then(data => ({ data }), error => ({ error }))'
  const reply = await evaluate(invocation)
  if (error) {
    assert.match(reply?.error?.code ?? '', error)
    return
  }
  assert.ok(reply?.data, JSON.stringify(reply?.error))
  return reply.data
}

async function withMcp(token, work) {
  const clientEnv = { ...env, ...(token ? { FTW_AGENT_TOKEN: token } : {}) }
  const child = spawn(join(directory, 'ftw-mcp.exe'), ['--dev-root', profile],
    { cwd: directory, env: clientEnv, windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'] })
  const lines = createInterface({ input: child.stdout })
  const pending = new Map()
  const closed = new Promise(done => child.once('exit', done))
  let next = 0
  lines.on('line', line => {
    const response = JSON.parse(line)
    pending.get(response.id)?.(response)
  })
  function request(method, params) {
    const id = ++next
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(new Error('MCP timeout: ' + method)) }, 15000)
      pending.set(id, response => { clearTimeout(timer); pending.delete(id); resolve(response) })
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n')
    })
  }
  try {
    const protocolVersion = '2025-11-25'
    const init = await request('initialize', { protocolVersion, capabilities: {}, clientInfo: { name: 'ftw-parity-test', version: '1' } })
    assert.equal(init.result.protocolVersion, protocolVersion)
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n')
    await work(async (name, args = {}, error) => {
      const result = await request('tools/call', { name, arguments: args })
      const content = result.result?.content?.[0]?.text
      const receipt = result.result?.structuredContent
      assert.equal(receipt?.version, 1, name)
      assert.equal(receipt.operation, name)
      if (error) {
        assert.ok(result.error || result.result?.isError, name)
        assert.match([content ?? result.error?.message, receipt.detail].filter(Boolean).join(' '), error)
        return
      }
      assert.ok(!result.error && !result.result?.isError, JSON.stringify(result))
      const data = JSON.parse(content)
      if (data.record?.state === 'delivery_unknown') {
        assert.equal(receipt.status, 'delivery_unknown')
        assert.equal(receipt.success, null)
        assert.equal(receipt.retryable, false)
        assert.equal(receipt.requires_human, true)
        assert.equal(receipt.code, data.record.outcome)
      }
      if (data.record?.state === 'failed') {
        assert.equal(receipt.status, 'failed')
        assert.equal(receipt.success, false)
        assert.equal(receipt.code, data.record.outcome)
      }
      if (['template_saved', 'lead_updated'].includes(data.kind)) {
        assert.equal(receipt.entity_id, data.id)
        assert.equal(receipt.entity_version, data.revision)
      }
      return data
    })
  } finally {
    lines.close()
    child.stdin.end()
    await closed
  }
}

const token = randomBytes(32).toString('hex')
const secondToken = randomBytes(32).toString('hex')
const now = Math.floor(Date.now() / 1000)
const crm = command => desktop('crm', command)
const mail = command => desktop('mail', command)
const template = { name: 'Parity Agent Template', purpose: 'first_outreach', subject: 'Hello {{contact_name}}', body: 'Parity template body' }
const companyInput = { name: 'Parity Shared Company', website: '', notes: 'Initial company notes', aliases: ['Parity Shared Alias'] }

function openFixtureDb(path) {
  const db = new DatabaseSync(path)
  db.exec('PRAGMA busy_timeout=5000')
  return db
}

async function verifyCompany(call, writableField, hiddenField) {
  const create = async (name, identity = {}) => (await crm({ action: 'save_contact', id: null, allow_duplicate: false,
    input: { display_name: name, ...identity } })).id
  const target = async id => {
    const { record_revision: revision, data_epoch: epoch } = (await call('get_customer', { contact_id: id })).record
    return { id, revision, epoch }
  }
  const companyOf = contact_id => call('get_customer_company', { contact_id })
  const profileTarget = ({ id, revision, epoch }) => ({ id, revision, epoch })
  const anchor = await create('Parity Company Anchor', { company_name: 'Parity Company Identity', country_region: 'UK' })
  const unlinked = await create('Parity Company Peer')
  const foreign = await create('Parity Foreign Office', { country_region: 'US' })
  await call('associate_company_contact', { request_id: 'parity-company-identity-conflict', anchor: await target(anchor),
    contact: await target(foreign) }, /company_identity_conflict/)
  const associate = { request_id: 'parity-company-associate', anchor: await target(anchor), contact: await target(unlinked) }
  assert.equal((await call('associate_company_contact', associate)).id, unlinked)
  assert.equal((await call('get_customer', { contact_id: unlinked })).record.input.country_region, 'UK')

  await click('.app-nav button', '客户管理')
  await click('.customer-name-text', 'Parity Company Anchor')
  await click('button', '公司联系人')
  await visible('Parity Company Peer', '[aria-label="公司联系人列表"]')
  const save = { request_id: 'parity-company-create', anchor: await target(anchor), input: companyInput, include_group: true }
  const saved = await call('save_company_profile', save)
  assert.deepEqual(await call('save_company_profile', save), saved)
  await call('save_company_profile', { ...save, include_group: false }, /agent_request_conflict/)
  await visible(companyInput.name, '.company-banner')
  const first = (await companyOf(anchor)).profile
  assert.deepEqual((await call('find_company_profiles', { keyword: 'Parity Shared Alias' })).rows, [first])
  assert.equal((await companyOf(unlinked)).profile.id, saved.id)

  await click('.company-profile-actions button', '编辑公司档案')
  await fillForm('.company-profile-form', { notes: 'Unsaved desktop company notes' })
  const update = { ...save, request_id: 'parity-company-edit', anchor: await target(anchor), profile: profileTarget(first),
    input: { ...companyInput, notes: 'Agent updated company notes' } }
  await call('save_company_profile', update)
  await call('save_company_profile', { ...update, request_id: 'parity-company-stale' }, /crm_revision_conflict/)
  await visible('当前输入已保留', '.company-profile-form')
  assert.equal(await evaluate('document.querySelector(".company-profile-form textarea[name=notes]").value'), 'Unsaved desktop company notes')
  assert.equal(await evaluate('document.querySelector(".company-profile-form button[type=submit]").disabled'), true)
  await click('.company-profile-form button', '取消')
  await click('.company-profile-actions button', '编辑公司档案')
  await fillForm('.company-profile-form', { notes: 'Desktop verified company notes' })
  await click('.company-profile-form button', '保存公司档案')
  await visible('Desktop verified company notes', '.crm-insight-context')
  const company = (await companyOf(anchor)).profile
  assert.equal(company.notes, 'Desktop verified company notes')
  assert.equal(company.revision, first.revision + 2)

  const link = { request_id: 'parity-company-link', contact: await target(foreign), change: {
    action: 'link', profile: profileTarget(company), branch: 'US office',
  } }
  assert.equal((await call('set_company_membership', link)).id, foreign)
  await visible('US office', '[aria-label="公司联系人列表"]')
  await call('set_company_membership', { ...link, request_id: 'parity-company-stale-unlink', change: { action: 'unlink' } }, /crm_revision_conflict/)
  const otherAnchor = await create('Parity Other Company Anchor')
  const other = await crm({ action: 'save_company_profile', anchor: await target(otherAnchor), profile: null,
    input: { ...companyInput, name: 'Parity Other Company', aliases: [] }, include_group: false })
  const otherProfile = (await call('find_company_profiles', { keyword: 'Parity Other Company' })).rows[0]
  assert.equal(otherProfile.id, other.id)
  await call('set_company_membership', { ...link, request_id: 'parity-company-membership-conflict', contact: await target(foreign),
    change: { action: 'link', profile: profileTarget(otherProfile) } }, /company_membership_conflict/)
  await call('set_company_membership', { ...link, request_id: 'parity-company-branch', contact: await target(foreign),
    change: { ...link.change, branch: 'US distribution' } })
  await visible('US distribution', '[aria-label="公司联系人列表"]')
  await call('set_company_membership', { ...link, request_id: 'parity-company-unlink', contact: await target(foreign), change: { action: 'unlink' } })
  assert.equal((await companyOf(foreign)).profile, null)

  const newContact = { request_id: 'parity-company-new-contact', profile: profileTarget(company),
    input: { display_name: 'Parity New Company Contact' },
    entries: [{ field_id: writableField, value: { type: 'text', value: 'Company contact fixture' } }] }
  await call('create_company_contact', { ...newContact, request_id: 'parity-company-create-rollback',
    profile: { ...newContact.profile, revision: company.revision - 1 } }, /crm_revision_conflict/)
  assert.deepEqual((await call('list_customers', { keyword: newContact.input.display_name })).rows, [])
  await call('create_company_contact', { ...newContact, request_id: 'parity-company-restricted-field',
    entries: [{ field_id: hiddenField, value: { type: 'text', value: 'Denied entry' } }] }, /crm_not_found/)
  assert.deepEqual((await call('list_customers', { keyword: newContact.input.display_name })).rows, [])
  const added = await call('create_company_contact', newContact)
  assert.deepEqual(await call('create_company_contact', newContact), added)
  await visible(newContact.input.display_name, '[aria-label="公司联系人列表"]')
  assert.equal((await companyOf(added.id)).profile.id, company.id)
  assert.ok((await call('get_customer_field_values', { contact_id: added.id })).record.entries.some(entry =>
    entry.field_id === writableField && entry.value?.value === 'Company contact fixture'))
  const audit = await desktop('data', { action: 'audit_workspace', entity: 'contact', source: 'agent',
    keyword: 'parity-company-unlink', days: 7, offset: 0 })
  assert.ok(audit.rows.some(row => row.entity_id === foreign && row.actor_name === '本机 Agent'
    && row.task_id === 'parity-company-unlink' && row.operation === 'remove_company_membership'))
  assert.ok((await call('list_agent_runs', { keyword: newContact.request_id })).rows.some(row => row.record.entity_id === added.id))
  console.log('Company profile and membership parity verified with a live editor, revision protection, atomic create and audit')
  return link
}

try {
  await visible('客户管理', '.app-nav button')

  const account = 'parity-fixture-account'
  const dbpath = join(profile, 'data', 'workbench.db')

  await click('.app-nav button', '客户管理')
  const humanName = 'Parity Human Customer'
  await click('main button', '新增客户')
  await visible('客户名称 *', 'main form')
  await evaluate("(() => { const input = document.querySelector('input[required]'); " +
    "Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, " + JSON.stringify(humanName) +
    "); input.dispatchEvent(new Event('input', { bubbles: true })); })()")
  await click('main button', '保存客户')
  await visible(humanName)
  await click('.app-nav button', '客户管理')
  console.log('Desktop started and created a contact before Agent registration')

  const agent = { id: 'local-mcp' }
  const db = openFixtureDb(dbpath)
  // The account only enables draft validation. This test never queues network mail.
  db.exec("INSERT INTO local_secrets(id,protected) VALUES('parity-fixture',X''); " +
    "INSERT INTO email_accounts(id,display_name,email_address,smtp_host,smtp_port,smtp_security,smtp_username,secret_ref) " +
    "VALUES('parity-fixture-account','Parity fixture','sender@example.test','invalid.example.test',465,'tls','sender@example.test','parity-fixture')")
  db.close()
  await withMcp(undefined, call => call('list_customers'))
  await withMcp(secondToken, async call => {
    const found = await call('search_workbench', { keyword: humanName })
    assert.deepEqual(found.counts, { all: 1, customer: 1, mail: 0, activity: 0, file: 0 })
    assert.deepEqual((await call('list_inbox')).rows, [])
    await call('create_customer', { request_id: 'parity-secondary-create', display_name: 'Secondary fixture' })
  })

  await withMcp(token, async call => {
    const human = (await call('list_customers', { keyword: humanName })).rows[0]
    assert.equal(human.display_name, humanName)
    const selector = '.customer-name-text'
    const agentName = 'Parity Agent Customer'
    await visible(humanName, selector)
    const created = await call('create_customer', { display_name: agentName, email: 'buyer@example.test', request_id: 'parity-agent-create' })
    await visible(agentName, selector)
    console.log('MCP read the desktop customer and its own creation refreshed the open customer list')
    const completeCustomer = await call('create_customer_profile', { request_id: 'parity-complete-profile', input: {
      display_name: 'Complete customer fixture', company_name: 'Complete Industrial', country_region: 'Chile',
      main_need: 'Test sourcing request', methods: [
        { kind: 'email', value: 'complete@example.test', label: 'Purchasing', is_primary: true, is_verified: false },
        { kind: 'phone', value: '+56 2 2222 2222', label: 'Office', is_primary: false, is_verified: false },
      ], interests: [{ product_text: 'Fixture model', intent_quantity: 20, quantity_is_estimate: true, requirement: 'Trial batch' }],
    } })
    assert.equal((await call('get_customer', { contact_id: completeCustomer.id })).record.input.interests[0].intent_quantity, 20)
    await visible('Complete customer fixture', selector)

    const original = (await call('get_customer', { contact_id: created.id })).record
    await crm({ action: 'save_contact_checked', id: created.id, allow_duplicate: false,
      expected_revision: original.record_revision, expected_epoch: original.data_epoch,
      input: { ...original.input, customer_level: 'B' } })
    const humanEdit = (await call('get_customer', { contact_id: created.id })).record
    assert.equal(humanEdit.input.customer_level, 'B')
    await call('update_customer', { request_id: 'parity-unguarded', contact_id: created.id, customer_level: 'A' }, /agent_crm_revision_required/)
    await call('update_customer', { request_id: 'parity-stale-contact', contact_id: created.id, stage: 'contacted',
      expected_revision: original.record_revision, expected_epoch: original.data_epoch }, /crm_revision_conflict/)

    await click('.customer-name-text', agentName)
    await click('.detail-sections button', '变更历史')
    await visible('新值 B')
    await call('update_customer', { request_id: 'parity-current-contact', contact_id: created.id, customer_level: 'A',
      expected_revision: humanEdit.record_revision, expected_epoch: humanEdit.data_epoch })
    await visible('本机 Agent')
    await visible('新值 A')
    const history = await call('get_customer_change_history', { contact_id: created.id })
    assert.deepEqual(history.rows.slice(0, 2).map(row => [row.actor_name, row.fields[0].before, row.fields[0].after]),
      [['本机 Agent', 'B', 'A'], ['desktop-user', '', 'B']])
    console.log('Both contact writes and actor diffs propagated, and a stale write was rejected')

    const field = await crm({ action: 'save_field', definition: { id: null, expected_revision: null,
      name: 'Parity Procurement Code', kind: 'text', color: 'gray', agent_access: 'write',
      show_in_details: true, default_value: null, options: [] } })
    assert.ok((await call('list_customer_fields')).fields.some(row => row.id === field.id && row.agent_access === 'write'))
    const { revision, epoch } = (await call('get_customer_field_values', { contact_id: created.id })).record
    await call('set_customer_field_values', { request_id: 'parity-agent-field', patch: {
      contact_id: created.id, expected_revision: revision, expected_epoch: epoch,
      entries: [{ field_id: field.id, value: { type: 'text', value: 'PARITY-CODE-931' } }],
    } })
    await click('.detail-sections button', '基本资料')
    await visible('PARITY-CODE-931')
    const definition = { id: null, expected_revision: null, name: 'Parity Agent field',
      kind: 'text', color: 'gray', agent_access: 'hidden', show_in_details: true, default_value: null, options: [] }
    const agentField = await call('save_customer_field_definition', definition)
    assert.ok((await crm({ action: 'list_fields', include_archived: false })).fields.some(row => row.id === agentField.id))
    assert.ok((await call('list_customer_fields')).fields.some(row => row.id === agentField.id && row.agent_access === 'write'))
    await crm({ action: 'save_field', definition: { ...definition, id: agentField.id, expected_revision: 1 } })
    await call('save_customer_field_definition', { ...definition, id: agentField.id, expected_revision: 1, agent_access: 'write' }, /crm_read_only/)
    const latest = (await call('get_customer_field_values', { contact_id: created.id })).record
    await crm({ action: 'patch_field_values', patch: { contact_id: created.id,
      expected_revision: latest.revision, expected_epoch: latest.epoch,
      entries: [
        { field_id: field.id, value: { type: 'text', value: 'PARITY-DESKTOP-UPDATED' } },
        { field_id: agentField.id, value: { type: 'text', value: 'PARITY-HIDDEN-FIXTURE' } },
      ] } })
    const fieldHistory = (await call('get_customer_field_history', { contact_id: created.id })).rows
    assert.equal(fieldHistory[0].before.value, 'PARITY-CODE-931')
    assert.equal(fieldHistory[0].after.value, 'PARITY-DESKTOP-UPDATED')
    assert.equal(fieldHistory[1].task_id, 'parity-agent-field')
    assert.ok(!fieldHistory.some(row => row.field_id === agentField.id))
    await click('.detail-sections button', '变更历史')
    await click('button', '自定义字段记录')
    await visible('PARITY-DESKTOP-UPDATED')
    await visible('PARITY-HIDDEN-FIXTURE')
    console.log('Custom field history preserves typed values, task attribution and hidden-field access rules')

    const view = name => ({ id: null, expected_revision: null, name, hidden: false, is_default: false, config: { query: { keyword: agentName } } })
    const desktopView = await crm({ action: 'save_view', view: view('Parity Desktop View') })
    assert.deepEqual((await call('query_customer_view', { view_id: desktopView.id })).rows.map(row => row.id), [created.id])
    const agentView = await call('save_customer_view', view('Parity Agent View'))
    const viewRecord = (await crm({ action: 'list_views', include_archived: false })).views.find(row => row.id === agentView.id)
    assert.equal(viewRecord.name, 'Parity Agent View')
    await crm({ action: 'save_view', view: { ...view('Parity Renamed View'), id: viewRecord.id, expected_revision: viewRecord.revision } })
    assert.equal((await call('list_customer_views')).views.find(row => row.id === agentView.id).name, 'Parity Renamed View')
    await call('archive_customer_view', { view_id: agentView.id, expected_revision: viewRecord.revision }, /crm_revision_conflict/)
    await call('archive_customer_view', { view_id: agentView.id, expected_revision: viewRecord.revision + 1 })
    assert.ok((await crm({ action: 'list_views', include_archived: true })).views.find(row => row.id === agentView.id).archived)
    const privateView = await crm({ action: 'save_view', view: { ...view('Parity Private View'), hidden: true } })
    const privateRecord = (await crm({ action: 'list_views', include_archived: false })).views.find(row => row.id === privateView.id)
    await crm({ action: 'archive_view', id: privateView.id, expected_revision: privateRecord.revision, archived: true })
    const archives = (await call('list_archived_customer_views')).views
    assert.ok(!archives.some(row => row.id === privateView.id))
    await call('restore_customer_view', { view_id: privateView.id, expected_revision: privateRecord.revision + 1 }, /crm_read_only/)
    const archivedView = archives.find(row => row.id === agentView.id)
    assert.ok(archivedView.archived)
    await eventually(async () => !await evaluate('document.querySelector(".customer-view-tabs").innerText.includes("Parity Renamed View")'),
      'Archived view disappears from the open desktop tabs')
    await call('restore_customer_view', { view_id: archivedView.id, expected_revision: archivedView.revision - 1 }, /crm_revision_conflict/)
    await call('restore_customer_view', { view_id: archivedView.id, expected_revision: archivedView.revision })
    assert.ok((await crm({ action: 'list_views', include_archived: false })).views.some(row => row.id === agentView.id))
    assert.deepEqual((await call('query_customer_view', { view_id: agentView.id })).rows.map(row => row.id), [created.id])
    await visible('Parity Renamed View', '.customer-view-tabs button')
    console.log('Both clients used and managed shared views, including permission-checked archive and restore')

    const input = { contact_id: created.id, event_type: 'note', occurred_at: now,
      summary: 'Parity desktop follow-up', details: '' }
    const note = await crm({ action: 'save_activity', id: null, input })
    const timeline = () => call('get_customer_timeline', { contact_id: created.id })
    assert.ok((await timeline()).rows.some(row => row.id === note.id))
    await click('.detail-sections button', '跟进记录')
    await call('add_customer_activity', { ...input, summary: 'Parity Agent follow-up', request_id: 'parity-followup' })
    await visible('Parity Agent follow-up')
    const noteRevision = (await timeline()).rows.find(row => row.id === note.id).revision
    await call('edit_customer_activity', { activity_id: note.id, expected_revision: noteRevision,
      input: { ...input, summary: 'Parity edited follow-up' } })
    await visible('Parity edited follow-up')
    await call('edit_customer_activity', { activity_id: note.id, expected_revision: noteRevision, input }, /crm_revision_conflict/)
    await call('add_customer_activity', { ...input, event_type: 'sample', request_id: 'parity-sample-batch',
      summary: 'Parity sample progress', sample: { model: 'PX-931', quantity: 2, batch_ref: 'parity-sample-931' } })
    const sampleQuery = { keyword: 'PX-931', status: '', by_batch: true, offset: 0 }
    const sampleRows = await call('list_sample_records', sampleQuery)
    assert.equal(sampleRows.rows[0].model, 'PX-931')
    assert.deepEqual(sampleRows, await crm({ action: 'sample_workspace', ...sampleQuery }))
    console.log('Both activity directions and stale activity edits verified')

    const draft = { contact_id: created.id, account_id: account, recipient: 'buyer@example.test',
      subject: 'Parity desktop draft', body: 'Synthetic message, never sent.' }
    const humanDraft = await mail({ action: 'save_draft', id: null, input: draft })
    assert.ok((await call('list_email_drafts', { contact_id: created.id })).rows.some(row => row.id === humanDraft.id))
    const agentDraft = await call('create_email_draft', { contact_id: draft.contact_id,
      subject: 'Parity Agent draft', body: draft.body, request_id: 'parity-draft-create' })
    const opened = (await mail({ action: 'get_draft', id: agentDraft.id })).record
    assert.equal(opened.subject, 'Parity Agent draft')
    assert.equal(opened.account_id, account)
    assert.equal(opened.recipient, draft.recipient)
    await mail({ action: 'update_draft', id: agentDraft.id, expected_revision: opened.revision, asset_ids: null,
      input: { ...draft, subject: 'Parity desktop edited Agent draft' } })
    await call('edit_email_draft', { ...draft, draft_id: agentDraft.id, expected_revision: opened.revision }, /mail_revision_conflict/)
    await call('edit_email_draft', { ...draft, draft_id: agentDraft.id, expected_revision: opened.revision + 1, subject: 'Parity shared edited draft' })
    assert.equal((await mail({ action: 'get_draft', id: agentDraft.id })).record.subject, 'Parity shared edited draft')
    console.log('Both draft directions and stale draft edits verified')

    const bytes = Buffer.alloc(200 * 1024)
    bytes.write('%PDF-1.4\n')
    const uploaded = await call('upload_customer_document', { contact_id: created.id,
      filename: 'parity-agent-document.pdf', content_base64: bytes.toString('base64') })
    assert.equal((await call('list_customer_documents', { contact_id: created.id })).rows[0].created_by, agent.id)
    const chunk = await call('read_customer_document', { document_id: uploaded.id })
    const rest = await call('read_customer_document', { document_id: uploaded.id, offset: chunk.next_offset })
    assert.equal(rest.next_offset, null)
    assert.deepEqual(Buffer.concat([Buffer.from(chunk.content_base64, 'base64'), Buffer.from(rest.content_base64, 'base64')]), bytes)
    await call('upload_customer_document', { contact_id: created.id, filename: '../invalid.pdf',
      content_base64: bytes.toString('base64') }, /invalid_attachments/)
    const large = Buffer.alloc(780000)
    large.write('%PDF-1.4\n')
    const uploadId = randomUUID()
    const uploadPart = (offset, part) => ({ upload_id: uploadId, contact_id: created.id,
      filename: 'parity-large-document.pdf', total_size: large.length, offset, content_base64: part.toString('base64') })
    const pieceSize = 192 * 1024
    const firstUpload = uploadPart(0, large.subarray(0, pieceSize))
    const firstUploaded = await call('upload_customer_document_chunk', firstUpload)
    assert.deepEqual(await call('upload_customer_document_chunk', firstUpload), firstUploaded)
    await call('upload_customer_document_chunk', uploadPart(0, Buffer.alloc(pieceSize, 1)), /upload_chunk_conflict/)
    assert.ok(!(await call('list_customer_documents', { contact_id: created.id })).rows.some(row => row.filename === firstUpload.filename))
    let upload = firstUploaded
    while (!upload.document_id) {
      const part = uploadPart(upload.next_offset, large.subarray(upload.next_offset, upload.next_offset + pieceSize))
      upload = await call('upload_customer_document_chunk', part)
      if (upload.document_id) assert.deepEqual(await call('upload_customer_document_chunk', part), upload)
    }
    const verified = await call('read_customer_document', { document_id: upload.document_id })
    assert.equal(verified.sha256, createHash('sha256').update(large).digest('hex'))
    assert.equal(verified.size, large.length)
    await call('upload_customer_document', { contact_id: created.id, filename: 'parity-preview.png',
      content_base64: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/lqkAAAAASUVORK5CYII=' })
    await click('.detail-sections button', '邮件往来')
    await click('button', '查看客户附件')
    await visible('parity-agent-document.pdf')
    await visible('parity-large-document.pdf')
    await click('table[aria-label="客户附件列表"] button', 'parity-preview.png')
    await click('button', '预览图片')
    await eventually(() => evaluate('document.querySelector(".customer-file-preview img")?.naturalWidth === 1'), 'Native image preview')
    await click('table[aria-label="客户附件列表"] button', 'parity-agent-document.pdf')
    await visible('另存文件', 'main button')
    console.log('Segmented upload retries, offset conflicts, final hash and native image preview verified')

    const saveDuplicateFixture = name => crm({ action: 'save_contact', id: null, allow_duplicate: true,
      input: { display_name: name, company_name: 'Parity duplicate test company', country_region: 'US',
        methods: [{ kind: 'email', value: 'parity-duplicate@example.test', label: '', is_primary: true, is_verified: true }] } })
    const keep = await saveDuplicateFixture('Parity Duplicate Record A')
    const remove = await saveDuplicateFixture('Parity Duplicate Record B')
    const findContact = async contact_id => (await call('get_customer', { contact_id })).record
    const target = record => ({ id: record.id, revision: record.record_revision, epoch: record.data_epoch })
    assert.equal((await call('list_customer_duplicates', { keyword: 'Parity Duplicate Record' })).rows.length, 1)
    const originalSecondary = await findContact(remove.id)
    await call('set_customer_archived', { target: target(originalSecondary), archived: true })
    assert.ok((await crm({ action: 'get_contact', id: remove.id })).record.archived_at)
    await call('set_customer_archived', { target: target(originalSecondary), archived: false }, /crm_revision_conflict/)
    await call('set_customer_archived', { target: target(await findContact(remove.id)), archived: false })
    const primary = target(await findContact(keep.id))
    const secondary = target(await findContact(remove.id))
    const mergeActivity = await crm({ action: 'save_activity', id: null, input: {
      contact_id: remove.id, event_type: 'note', occurred_at: now, summary: 'Transfer parity fixture', details: '',
    } })
    await call('merge_customers', { primary, secondary })
    const merged = (await crm({ action: 'get_contact', id: remove.id })).record
    assert.equal(merged.merged_into, keep.id)
    assert.ok((await call('get_customer_timeline', { contact_id: keep.id })).rows.some(row => row.id === mergeActivity.id))
    console.log('Duplicate discovery, archive conflicts and permission-gated merging verified')

    const csv = { csv: 'display_name,email\nParity CSV Agent,parity-csv@example.test\n', default_stage: 'prospect' }
    const preview = await call('import_customer_csv', csv)
    assert.equal(preview.accepted, 1)
    assert.equal(preview.applied, false)
    assert.deepEqual((await call('list_customers', { keyword: 'Parity CSV Agent' })).rows, [])
    const applyCsv = { ...csv, apply: true, request_id: 'parity-import-csv' }
    const imported = await call('import_customer_csv', applyCsv)
    assert.equal(imported.applied, true)
    assert.deepEqual(await call('import_customer_csv', applyCsv), imported)
    const exported = await call('export_customer_csv', { scope: 'active', fields: ['display_name', 'email'] })
    assert.match(exported.csv, /Parity CSV Agent,parity-csv@example\.test/)
    assert.equal(exported.next_offset, null)
    console.log('CSV preview rolls back and retryable import and export use Core validation')

    assert.deepEqual((await call('get_mail_task', { task_id: 'unused-parity-task' })).rows, [])
    const runs = await desktop('data', { action: 'agent_runs', filter: 'all', keyword: '本机 Agent', offset: 0 })
    assert.ok(runs.rows.some(row => row.record.request_id === 'parity-agent-create' && row.record.agent_name === '本机 Agent'))
    assert.ok((await call('list_agent_runs', { keyword: 'parity-agent-create' })).rows.some(row =>
      row.record.agent_id === agent.id && row.record.success === true && row.record.retryable === false))
    assert.ok((await call('list_agent_runs', { keyword: 'parity-stale-contact' })).rows.some(row =>
      row.record.success === false && row.record.retryable === true))
    assert.ok((await call('list_agent_runs', { keyword: '本机 Agent' })).rows.some(row => row.record.request_id === 'parity-secondary-create'))

    // Only terminal SMTP fixtures are inserted in the desktop profile. Loopback testing of live queue transitions is in Rust.
    const cases = openFixtureDb(dbpath)
    const taskFixture = cases.prepare(`INSERT INTO email_outbox_jobs(id,draft_id,policy_revision,queued_source,agent_id,
      task_id,request_id,content_sha256,purpose,normalized_recipient,revision,message_id_header,state,attempt_count,outcome)
      VALUES(?,?,1,'agent',?,'parity-mail-controls',?,'fixture','outreach_followup','buyer@example.test',1,?,?,1,?)`)
    for (const [fixture, state, outcome] of [['parity-rejected', 'failed', 'smtp_rejected'],
      ['parity-unknown', 'delivery_unknown', 'smtp_result_unknown']]) {
      cases.prepare(`INSERT INTO email_drafts(id,contact_id,account_id,recipient,subject,body_text,attachments_json,state,source)
        VALUES(?,?,?,'buyer@example.test',?,'Fixture','[]',?,'agent')`).run(fixture, created.id, account, fixture, state)
      taskFixture.run(fixture, fixture, agent.id, fixture, '<' + fixture + '@example.test>', state, outcome)
    }
    const pdf = Buffer.from('%PDF-1.4\nattachment fixture\n%%EOF\n', 'ascii')
    const content = Buffer.from([
      'From: external@example.test', 'To: sender@example.test', 'Subject: Parity reply with file',
      'Message-ID: <parity-inbox@example.test>', 'MIME-Version: 1.0', 'Content-Type: multipart/mixed; boundary="sample"', '',
      '--sample', 'Content-Type: text/plain', '', 'Reply body.', '--sample',
      'Content-Type: application/octet-stream; name="note.txt"', 'Content-Disposition: attachment; filename="note.txt"',
      'Content-Transfer-Encoding: base64', '', 'RldUIGZpeHR1cmU=', '--sample',
      'Content-Type: application/pdf; name="reply.pdf"', 'Content-Disposition: attachment; filename="reply.pdf"',
      'Content-Transfer-Encoding: base64', '', ...pdf.toString('base64').match(/.{1,76}/g), '--sample--', '',
    ].join('\r\n'))
    cases.prepare(`INSERT INTO inbox_messages(id,account_id,thread_id,message_id_header,raw_sha256,sender,subject,
      body_text,attachment_names,received_at,raw,kind,message_role,match_state)
      VALUES('parity-inbox',?,'inbox:parity-inbox','parity-inbox@example.test','fixture','external@example.test',
       'Parity reply with file','Reply body.','["note.txt","reply.pdf"]',?,?,'message','correspondence','pending')`).run(account, now, content)
    cases.prepare(`INSERT INTO inbox_messages(id,account_id,contact_id,thread_id,message_id_header,raw_sha256,sender,subject,
      body_text,attachment_names,received_at,raw,kind,message_role,match_state)
      VALUES('parity-threaded',?,?,'inbox:parity-threaded','confirmed-inbox@example.test','thread-fixture','buyer@example.test',
       'Parity linked quotation','Quote request','[]',?,?,'message','correspondence','manual')`).run(account, created.id, now, Buffer.from('fixture'))
    cases.close()
    const attachment = await call('read_email_attachment', { source: 'inbox', message_id: 'parity-inbox', index: 0 })
    assert.equal(Buffer.from(attachment.content_base64, 'base64').toString(), 'FWT fixture')
    const inline = pdf
    const outbound = await mail({ action: 'save_draft', id: null, input: { ...draft, subject: 'Parity outbound attachment',
      attachments: [{ filename: 'outbound.pdf', content_base64: inline.toString('base64') }] } })
    const outboundBytes = await call('read_email_attachment', { source: 'outbound', message_id: outbound.id, index: 0 })
    assert.deepEqual(Buffer.from(outboundBytes.content_base64, 'base64'), inline)
    const incomingPdf = await call('read_email_attachment', { source: 'inbox', message_id: 'parity-inbox', index: 1 })
    assert.deepEqual(Buffer.from(incomingPdf.content_base64, 'base64'), pdf)
    const queued = await call('list_reply_review')
    assert.ok(queued.rows.some(row => row.id === 'parity-inbox'))
    await click('.app-nav button', '邮件中心')
    await click('.mail-folders button', '回复待确认')
    await visible('Parity reply with file')
    await call('suggest_reply_link', { message_id: 'parity-inbox', contact_id: created.id })
    await visible('建议人 本机 Agent')
    const candidates = await call('get_reply_link_candidates', { message_id: 'parity-inbox' })
    assert.deepEqual(candidates.rows.find(row => row.id === created.id).suggested_by, ['本机 Agent'])
    assert.equal((await desktop('inbox', { action: 'get', id: 'parity-inbox' })).record.contact_id, null)
    await desktop('inbox', { action: 'review_link', id: 'parity-inbox', contact_id: created.id, expected_contact: null, expected_match: 'pending' })
    await call('suggest_reply_link', { message_id: 'parity-inbox', contact_id: created.id }, /mail_link_changed/)
    await click('.mail-folders button', '收件箱')
    await click('.mail-list-item', 'Parity reply with file')
    await visible('reply.pdf', '.mail-reader')
    await visible('另存附件', '.mail-reader button')
    assert.deepEqual((await call('get_mailbox_sync_status', { account_id: account })).rows,
      [{ account_id: account, configured: false, last_sync: null, outcome: null, complete: false }])
    assert.equal((await call('list_inbox', { account_id: account, keyword: 'Parity linked quotation' })).rows[0].id, 'parity-threaded')
    const reply = await call('create_reply_draft', { request_id: 'parity-threaded-draft', inbox_message_id: 'parity-threaded',
      subject: 'Agent threaded draft', body: 'Agent fixture response.' })
    const threaded = (await mail({ action: 'get_draft', id: reply.id })).record
    assert.equal(threaded.thread_id, 'inbox:parity-threaded')
    assert.equal(threaded.reply_to_inbox_id, 'parity-threaded')
    await click('.mail-list-item', 'Parity linked quotation')
    await click('.mail-reader button', '回复此邮件')
    await visible('回复邮件', '.mail-main')
    await evaluate(`(() => { const node = document.querySelector('.mail-main textarea');
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(node, 'Desktop fixture response.');
      node.dispatchEvent(new Event('input', { bubbles: true })); })()`)
    await click('.mail-main button', '保存草稿')
    await visible('Desktop fixture response.', '.mail-reader')
    const desktopReply = (await call('list_email_drafts', { keyword: 'Parity linked quotation', folder: 'drafts' })).rows[0]
    assert.equal((await call('get_email_draft', { draft_id: desktopReply.id })).record.reply_to_inbox_id, 'parity-threaded')
    console.log('Confirmed inbox, sync status, and native draft threading verified in both directions')
    await click('.mail-folders button', '草稿与审批')
    await click('.mail-list-item', 'Parity outbound attachment')
    await visible('outbound.pdf', '.mail-reader')
    await visible('另存附件', '.mail-reader button')
    console.log('Inbound and outbound attachments and reply suggestion review verified')
    const task = await call('get_mail_task', { task_id: 'parity-mail-controls' })
    assert.equal(task.rows.find(row => row.id === 'parity-rejected').can_reopen, true)
    assert.equal(task.rows.find(row => row.id === 'parity-unknown').can_reopen, false)
    assert.equal((await call('get_email_draft', { draft_id: 'parity-rejected' })).record.state, 'failed')
    assert.equal((await call('get_email_draft', { draft_id: 'parity-unknown' })).record.state, 'delivery_unknown')
    await call('update_mail_job', { job_id: 'parity-unknown', action: 'reopen' }, /mail_job_action_unavailable/)
    const reopened = await call('update_mail_job', { job_id: 'parity-rejected', action: 'reopen' })
    assert.equal(reopened.draft_revision, 2)
    assert.equal((await mail({ action: 'get_draft', id: reopened.draft_id })).record.state, 'draft')
    await mail({ action: 'set_suppression', target: 'blocked.example.test', blocked: true })
    assert.ok((await call('list_contact_suppressions')).rows.includes('blocked.example.test'))
    console.log('Own task receipts, safe retry boundaries and suppression visibility verified')
    await click('.mail-folders button', '邮件模板')
    await click('main button', '新增模板')
    await fillForm('.template-editor form', { name: 'Parity Desktop Template', subject: template.subject, body: template.body })
    await click('.template-editor button', '保存模板')
    await visible('Parity Desktop Template', '.template-list-row')
    const userTemplate = (await call('list_email_templates')).rows.find(row => row.name === 'Parity Desktop Template')
    assert.equal((await call('render_email_template', { template_id: userTemplate.id, contact_id: created.id })).subject, `Hello ${agentName}`)

    const createTemplate = { request_id: 'parity-template-create', input: template }
    await call('save_email_template', { ...createTemplate, request_id: 'parity-template-invalid',
      input: { ...template, subject: '{{unknown}}' } }, /invalid_template_variable/)
    const savedTemplate = await call('save_email_template', createTemplate)
    assert.equal(savedTemplate.revision, 1)
    assert.deepEqual(await call('save_email_template', createTemplate), savedTemplate)
    await call('save_email_template', { ...createTemplate, input: { ...template, body: 'Changed retry' } }, /agent_request_conflict/)
    await click('.template-list-row', template.name)
    await fillForm('.template-editor form', { body: 'Unsaved desktop input' })
    const edit = { request_id: 'parity-template-edit', template_id: savedTemplate.id,
      expected_revision: savedTemplate.revision, input: { ...template, body: 'Agent updated template' } }
    const editedTemplate = await call('save_email_template', edit)
    assert.equal(editedTemplate.revision, 2)
    await call('save_email_template', { ...edit, request_id: 'parity-template-stale' }, /template_revision_conflict/)
    await visible('当前输入已保留', '.template-editor')
    const openTemplateBody = () => evaluate('document.querySelector(".template-editor textarea").value')
    assert.equal(await openTemplateBody(), 'Unsaved desktop input')
    assert.equal(await evaluate('document.querySelector(".template-editor button[type=submit]").disabled'), true)
    await click('.template-editor button', '载入最新版本')
    assert.equal(await openTemplateBody(), 'Agent updated template')
    await fillForm('.template-editor form', { body: 'Saved desktop revision' })
    await click('.template-editor button', '保存模板')
    await visible('模板已保存', '.template-list')
    const editedByDesktop = (await call('list_email_templates')).rows.find(row => row.id === savedTemplate.id)
    assert.equal(editedByDesktop.body, 'Saved desktop revision')
    assert.equal(editedByDesktop.revision, 3)

    await fillForm('.template-editor form', { body: 'Desktop notes while archived' })
    const archive = { request_id: 'parity-template-archive', template_id: savedTemplate.id,
      expected_revision: editedByDesktop.revision, archived: true }
    const archivedTemplate = await call('set_email_template_archived', archive)
    assert.equal(archivedTemplate.revision, 4)
    await visible('当前输入已保留', '.template-editor')
    assert.equal(await openTemplateBody(), 'Desktop notes while archived')
    assert.equal((await call('list_email_templates', { archived: true })).rows[0].id, savedTemplate.id)
    assert.ok(!(await call('list_email_templates')).rows.some(row => row.id === savedTemplate.id))
    await call('render_email_template', { template_id: savedTemplate.id, contact_id: created.id }, /mail_not_found/)
    await click('.template-editor button', '载入最新版本')
    const restored = await call('set_email_template_archived', { ...archive,
      request_id: 'parity-template-restore', expected_revision: archivedTemplate.revision, archived: false })
    assert.equal(restored.revision, 5)
    await visible(template.name, '.template-list-row')
    assert.equal(await openTemplateBody(), 'Saved desktop revision')
    const audit = await desktop('data', { action: 'audit_workspace', entity: 'email_template', keyword: '', source: '', days: 7, offset: 0 })
    const audited = audit.rows.filter(row => row.entity_id === savedTemplate.id)
    assert.ok(audited.some(row => row.actor === 'desktop-user' && row.operation === 'edit'))
    assert.ok(audited.some(row => row.actor_name === '本机 Agent' && row.task_id === archive.request_id && row.operation === 'archive'))
    assert.ok((await call('list_agent_runs', { keyword: archive.request_id })).rows.some(row => row.record.entity_id === savedTemplate.id))
    console.log('Shared templates verified in both directions, with scoped access, idempotency, edit preservation and audit')

    await click('.app-nav button', '候选客户')
    await click('main button', '新增候选客户')
    const leadName = 'Parity Desktop Lead'
    await fillForm('.lead-editor', { display_name: leadName, company_name: 'Parity candidate company', research_notes: 'Desktop research' })
    await click('.lead-editor button', '保存候选客户')
    await visible(leadName, '.lead-workspace, .lead-evidence-workspace')
    const leadId = (await call('list_lead_candidates')).rows.find(row => row.display_name === leadName).id
    const getLead = async () => (await call('get_lead_candidate', { lead_id: leadId })).record
    const version = lead => ({ revision: lead.record_revision, epoch: lead.data_epoch })
    const originalLead = await getLead()
    assert.equal(originalLead.input.research_notes, 'Desktop research')
    if (await evaluate('Boolean(document.querySelector(".lead-workspace"))')) {
      await click('.lead-row-name', leadName)
      await click('.lead-drawer button', '查看调查证据')
    }
    await click('main button', '编辑候选')
    await fillForm('.lead-editor', { research_notes: 'Unsaved desktop research' })
    const leadChange = { lead_id: leadId, expected: version(originalLead), request_id: 'parity-update-lead',
      patch: { business_summary: 'Agent research summary', add_source_urls: ['https://candidate.example.test/research'] } }
    const leadSaved = await call('update_lead_candidate', leadChange)
    assert.equal(leadSaved.revision, 2)
    assert.deepEqual(await call('update_lead_candidate', leadChange), leadSaved)
    const updatedLead = await getLead()
    assert.equal(updatedLead.input.company_name, 'Parity candidate company')
    assert.equal(updatedLead.input.research_notes, 'Desktop research')
    assert.deepEqual(updatedLead.input.source_urls, leadChange.patch.add_source_urls)
    const research = (await call('list_lead_evidence', { lead_id: leadId })).rows.find(row => row.url === leadChange.patch.add_source_urls[0])
    assert.equal(research.source, 'agent')
    assert.equal(research.request_id, leadChange.request_id)
    await call('update_lead_candidate', { ...leadChange, request_id: 'parity-lead-stale' }, /crm_revision_conflict/)
    await call('update_lead_candidate', { ...leadChange, expected: { ...version(updatedLead), epoch: 'stale' },
      request_id: 'parity-lead-stale-epoch' }, /crm_revision_conflict/)
    const duplicateLead = await call('update_lead_candidate', { ...leadChange, expected: version(updatedLead), request_id: 'parity-lead-duplicate',
      patch: { email: 'buyer@example.test' } })
    assert.equal(duplicateLead.kind, 'duplicates')
    await desktop('crm', { action: 'promote_lead', id: leadId, allow_duplicate: false, expected: version(originalLead) }, /crm_revision_conflict/)
    await desktop('crm', { action: 'set_lead_review', id: leadId, rejected: true, expected: version(originalLead) }, /crm_revision_conflict/)
    await visible('当前输入已保留', '.lead-editor')
    const leadNotes = () => evaluate('document.querySelector(".lead-editor textarea[name=research_notes]").value')
    assert.equal(await leadNotes(), 'Unsaved desktop research')
    assert.equal(await evaluate('document.querySelector(".lead-editor button[type=submit]").disabled'), true)
    await click('.lead-editor button', '载入最新版本')
    assert.equal(await leadNotes(), 'Desktop research')
    await fillForm('.lead-editor', { research_notes: 'Confirmed desktop follow-up' })
    await click('.lead-editor button', '保存候选客户')
    await visible('Confirmed desktop follow-up', '.lead-evidence-workspace')
    const humanLead = await getLead()
    assert.equal(humanLead.input.research_notes, 'Confirmed desktop follow-up')
    assert.equal(humanLead.record_revision, 3)
    await crm({ action: 'set_lead_review', id: leadId, rejected: true, expected: version(humanLead) })
    const rejectedLead = await getLead()
    assert.equal(rejectedLead.review_status, 'rejected')
    await call('update_lead_candidate', { ...leadChange, request_id: 'parity-lead-rejected', expected: version(rejectedLead) }, /crm_read_only/)
    const auditLead = await desktop('data', { action: 'audit_workspace', entity: 'lead', keyword: '', source: '', days: 7, offset: 0 })
    assert.ok(auditLead.rows.some(row => row.entity_id === leadId && row.actor_name === '本机 Agent' && row.task_id === leadChange.request_id))
    console.log('Lead candidate edits verified in both directions, including review protection, provenance and stale versions')

    await verifyCompany(call, field.id, agentField.id)
    const querySearch = (keyword, category = 'all') => call('search_workbench', { keyword, category })
    const assetsDb = openFixtureDb(dbpath)
    try {
      assetsDb.exec(`INSERT INTO mail_assets(id,filename,sha256,size,content_type) VALUES
        ('parity-private-search','parity-private-material.pdf','parity-hash',4,'application/pdf')`)
      assetsDb.prepare('UPDATE email_drafts SET asset_ids_json=? WHERE id=?').run('["parity-private-search"]', outbound.id)
    } finally { assetsDb.close() }
    const privateFile = 'parity-private-material.pdf'
    assert.equal((await desktop('data', { action: 'search', keyword: privateFile, category: 'file', offset: 0 })).counts.file, 1)
    assert.equal((await querySearch(privateFile, 'file')).counts.file, 0)
    const term = 'Parity reply with file'
    assert.deepEqual(await querySearch(term), await desktop('data', { action: 'search', keyword: term, category: 'all', offset: 0 }))
    assert.ok((await querySearch('reply.pdf', 'file')).rows.some(row => row.target.record_type === 'inbox_attachment'))
    assert.ok((await querySearch('parity-agent-document.pdf', 'file')).rows.some(row => row.target.record_type === 'document'))
    await withMcp(secondToken, async read => {
      assert.deepEqual(await read('search_workbench', { keyword: term }), await querySearch(term))
      assert.equal((await read('search_workbench', { keyword: 'reply.pdf', category: 'file' })).counts.file, 1)
      assert.equal((await read('search_workbench', { keyword: privateFile, category: 'file' })).counts.file, 0)
      const contact = (await read('get_customer', { contact_id: created.id })).record
      await read('update_customer_profile', { request_id: 'parity-secondary-edit',
        target: { id: contact.id, revision: contact.record_revision, epoch: contact.data_epoch },
        patch: { alibaba_buyer_id: 'shared-buyer' } })
    })
    assert.equal((await call('get_customer', { contact_id: created.id })).record.input.alibaba_buyer_id, 'shared-buyer')
    console.log('Both local MCP clients share search and can edit the same customer')

    const fieldVersion = (await call('list_customer_fields')).fields.find(row => row.id === field.id).revision
    await click('.app-nav button', '客户管理')
    await click('.customer-name-text', agentName)
    await click('.detail-sections button', '基本资料')
    await visible('PARITY-DESKTOP-UPDATED', '.contact-drawer')
    const archiveField = { request_id: 'parity-field-archive', field_id: field.id, expected_revision: fieldVersion, archived: true }
    const viewUsingField = await crm({ action: 'save_view', view: { ...view('Parity Filtering Field'), config: { query: {
      filter: { type: 'group', logic: 'and', children: [{ type: 'rule', field: field.id, operator: 'contains', value: 'PARITY' }] },
    } } } })
    const columnView = await call('save_customer_view', { ...view('Parity Field Column'), config: { query: { custom_columns: [field.id] },
      columns: [{ field: field.id, label: null, width: 150, hidden: false }] } })
    await call('set_customer_field_archived', { ...archiveField, request_id: 'parity-field-dependency' }, /field_in_use_by_view/)
    await crm({ action: 'archive_view', id: viewUsingField.id, expected_revision: 1, archived: true })
    const privateField = (await crm({ action: 'list_fields', include_archived: false })).fields.find(row => row.id === agentField.id)
    await desktop('crm', { action: 'archive_field', id: agentField.id, expected_revision: privateField.revision, archived: true })
    await call('set_customer_field_archived', { ...archiveField, request_id: 'parity-field-hidden', field_id: agentField.id }, /crm_read_only/)
    const beforeArchive = (await call('get_customer_field_values', { contact_id: created.id })).record.entries.find(row => row.field_id === field.id)
    const archivedField = await call('set_customer_field_archived', archiveField)
    assert.deepEqual(await call('set_customer_field_archived', archiveField), archivedField)
    await eventually(async () => !await evaluate('document.querySelector(".contact-drawer").innerText.includes("PARITY-DESKTOP-UPDATED")'),
      'Archived custom field is removed from the open customer detail')
    assert.ok(!(await call('list_customer_fields')).fields.some(row => row.id === field.id))
    const archivedFields = (await call('list_archived_customer_fields')).fields
    assert.deepEqual(archivedFields.map(row => row.id), [field.id])
    const withoutColumn = (await call('list_customer_views')).views.find(row => row.id === columnView.id)
    assert.deepEqual(withoutColumn.config.columns, [])
    assert.deepEqual(withoutColumn.config.query.custom_columns, [])
    await call('save_customer_view', { ...view('Parity Field Column Renamed'), id: withoutColumn.id,
      expected_revision: withoutColumn.revision, config: withoutColumn.config })
    await desktop('crm', { action: 'archive_view', id: viewUsingField.id, expected_revision: 2, archived: false }, /crm_invalid_field/)
    await call('set_customer_field_archived', { ...archiveField, request_id: 'parity-field-stale-restore', archived: false }, /crm_revision_conflict/)
    await call('set_customer_field_archived', { ...archiveField, request_id: 'parity-field-restore',
      expected_revision: archivedFields[0].revision, archived: false })
    await visible('PARITY-DESKTOP-UPDATED', '.contact-drawer')
    assert.deepEqual((await call('get_customer_field_values', { contact_id: created.id })).record.entries.find(row => row.field_id === field.id), beforeArchive)
    const restoredColumnView = (await call('list_customer_views')).views.find(row => row.id === columnView.id)
    assert.equal(restoredColumnView.name, 'Parity Field Column Renamed')
    assert.deepEqual(restoredColumnView.config.columns, [{ field: field.id, label: null, width: 150, hidden: false }])
    assert.deepEqual(restoredColumnView.config.query.custom_columns, [field.id])
    await crm({ action: 'archive_view', id: viewUsingField.id, expected_revision: 2, archived: false })
    assert.deepEqual((await call('query_customer_view', { view_id: viewUsingField.id })).rows.map(row => row.id), [created.id])
    console.log('Shared custom field archive and restore preserved values and view filters with field-level scope checks')

    await mail({ action: 'set_agent_paused', id: agent.id, paused: true })
    await call('get_customer', { contact_id: created.id }, /mail_agent_paused/)
    await mail({ action: 'set_agent_paused', id: agent.id, paused: false })
    assert.equal((await call('get_customer', { contact_id: created.id })).record.id, created.id)
    console.log('Local access, pause and resume verified')
  })
  await click('.app-nav button', '邮件中心')
  await click('.mail-folders button', 'Agent 发信权限')
  await visible('本机 Agent 接入')
  await visible('最近连接', '.agent-administration')
  await click('.agent-administration button', '暂停接入')
  await visible('Agent 已暂停')
  await withMcp(secondToken, call => call('get_workbench_context', {}, /mail_agent_paused/))
  await click('.agent-administration button', '恢复接入')
  await visible('Agent 已恢复')
  await withMcp(secondToken, call => call('get_workbench_context'))
  console.log('Desktop pause and resume control live MCP access')
} finally {
  app.kill()
  try {
    const { stdout } = await promisify(execFile)(join(directory, 'ftw-core.exe'),
      ['health', '--dev-root', profile], { windowsHide: true, timeout: 3000 })
    process.kill(JSON.parse(stdout).processId)
  } catch (error) {
    if (!['ESRCH', 1].includes(error.code)) throw error
  }
}
