import assert from 'node:assert/strict'
import { execFile, spawn } from 'node:child_process'
import { createHash, randomBytes } from 'node:crypto'
import { mkdir, mkdtemp } from 'node:fs/promises'
import path from 'node:path'
import { createInterface } from 'node:readline'
import { DatabaseSync } from 'node:sqlite'
import { promisify } from 'node:util'
import { setTimeout as delay } from 'node:timers/promises'

const target = path.resolve(process.argv[2] ?? 'target')
const coreExe = path.join(target, 'debug', 'ftw-core.exe')
const mcpExe = path.join(target, 'debug', 'ftw-mcp.exe')
const profiles = path.resolve('.ftw-dev')
await mkdir(profiles, { recursive: true })
const profile = await mkdtemp(path.join(profiles, 'agent-protocol-'))

let previousContact
async function exercise(protocol, token) {
  const env = { ...process.env }
  delete env.FTW_AGENT_TOKEN
  if (token) env.FTW_AGENT_TOKEN = token
  const child = spawn(mcpExe, ['--dev-root', profile], { env, windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'] })
  const reader = createInterface({ input: child.stdout })
  const responses = new Map()
  let number = 0
  reader.on('line', line => {
    const message = JSON.parse(line)
    responses.get(message.id)?.(message)
  })

  function send(method, params = {}) {
    const id = ++number
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        responses.delete(id)
        reject(new Error('Timed out: ' + method))
      }, 25_000)
      responses.set(id, value => {
        clearTimeout(timer)
        responses.delete(id)
        resolve(value)
      })
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n')
    })
  }

  try {
    const init = await send('initialize', {
      protocolVersion: protocol, capabilities: {}, clientInfo: { name: 'ftw-stdio-regression', version: '1' },
    })
    assert.equal(init.result.protocolVersion, protocol)
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n')
    const listing = await send('tools/list')
    const tools = listing.result.tools
    assert.deepEqual(tools.map(tool => tool.name).sort(), [
      'add_contact_evidence', 'add_customer_activity', 'add_lead_evidence', 'attach_email_assets', 'check_customer_outreach', 'create_customer', 'create_email_draft', 'create_lead_candidate', 'fetch_inbox_message',
      'get_customer', 'get_customer_company', 'get_customer_field_values', 'get_customer_state_catalog', 'get_customer_timeline', 'get_email_draft', 'get_email_thread',
      'get_pending_approvals', 'list_contact_evidence', 'list_customer_fields', 'list_customer_views', 'list_customers', 'list_email_accounts', 'list_email_assets', 'list_email_drafts',
      'list_email_templates', 'list_inbox', 'list_lead_candidates', 'list_lead_evidence', 'query_customers', 'read_inbox_body', 'render_email_template', 'request_email_approval', 'send_approved_email', 'send_authorized_email', 'set_customer_field_values',
      'suggest_customer_detail', 'sync_mailbox', 'update_customer',
      'archive_customer_view', 'edit_customer_activity', 'edit_email_draft', 'get_customer_change_history', 'get_mail_task', 'list_customer_documents', 'query_customer_view', 'read_customer_document', 'save_customer_view', 'upload_customer_document',
      'list_sample_records', 'save_customer_field_definition',
      'list_customer_duplicates', 'merge_customers', 'set_customer_archived',
      'import_customer_csv', 'export_customer_csv',
      'get_customer_field_history', 'get_reply_link_candidates', 'list_agent_runs', 'list_contact_suppressions',
      'list_reply_review', 'read_email_attachment', 'suggest_reply_link', 'update_mail_job', 'upload_customer_document_chunk',
      'save_email_template', 'set_email_template_archived', 'get_lead_candidate', 'update_lead_candidate',
      'list_archived_customer_views', 'restore_customer_view',
      'find_company_profiles', 'save_company_profile', 'set_company_membership', 'associate_company_contact', 'create_company_contact',
      'search_workbench', 'list_archived_customer_fields', 'set_customer_field_archived',
      'list_customer_quality_issues', 'preview_customer_batch', 'update_customer_batch',
      'get_mail_analytics', 'export_mail_analytics_csv', 'list_business_audit',
      'review_lead_candidate', 'review_customer_evidence', 'confirm_reply_link',
      'move_customer_field', 'move_customer_view',
      'create_customer_profile', 'create_reply_draft', 'get_mailbox_sync_status',
      'get_workbench_context', 'get_workbench_definitions', 'get_customers_batch', 'create_customers_batch',
      'add_customer_activities_batch', 'get_email_statuses', 'update_customer_profile', 'update_customers_batch',
    ].sort())
    assert.ok(tools.every(tool => tool.inputSchema.type === 'object'))
    assert.deepEqual(tools.find(tool => tool.name === 'save_email_template').inputSchema.required,
      ['request_id', 'input'])
    if (!token) {
      for (const name of ['list_customers', 'list_email_accounts', 'list_inbox', 'get_workbench_context']) {
        const result = await send('tools/call', { name, arguments: {} })
        assert.ok(!result.error && !result.result?.isError, JSON.stringify(result))
      }
      return
    }
    const db = new DatabaseSync(path.join(profile, 'data', 'workbench.db'))
    db.exec("INSERT OR IGNORE INTO mail_templates(id,name,purpose,subject,body) VALUES ('mcp-template-fixture','Fixture','first_outreach','Hello {{contact_name}}','Market: {{country_region}}')")
    db.close()
    async function call(name, fields = {}) {
      const response = await send('tools/call', { name, arguments: fields })
      assert.ok(!response.error && !response.result?.isError, JSON.stringify(response))
      return JSON.parse(response.result.content[0].text)
    }
    const context = await call('get_workbench_context')
    assert.equal(context.schema_version, 27)
    assert.equal(context.agent.id, 'local-mcp')
    assert.deepEqual(context.sender_accounts, [])
    const full = await call('get_workbench_definitions', { kind: 'states' })
    assert.equal(full.version, context.definitions.states)
    const unchanged = await call('get_workbench_definitions', { kind: 'states', known_version: full.version })
    assert.equal(unchanged.unchanged, true)
    assert.ok(JSON.stringify(unchanged).length < JSON.stringify(full).length)
    if (previousContact) {
      const record = (await call('get_customer', { contact_id: previousContact })).record
      const result = await call('update_customer_profile', {
        request_id: 'cross-agent-edit', target: { id: record.id, revision: record.record_revision, epoch: record.data_epoch },
        patch: { company_name: 'Shared handoff fixture' },
      })
      assert.equal(result.id, previousContact)
    }
    const customer = { request_id: 'fixture-' + protocol, display_name: 'MCP fixture ' + protocol, email: 'original-' + protocol + '@example.test' }
    const saved = await call('create_customer', customer)
    previousContact = saved.id
    assert.equal(saved.kind, 'saved')
    assert.deepEqual(await call('create_customer', customer), saved)
    const conflict = await send('tools/call', { name: 'create_customer', arguments: { ...customer, display_name: 'changed' } })
    assert.ok(conflict.error || conflict.result?.isError)
    assert.equal(conflict.result.structuredContent.code, 'agent_request_conflict')
    assert.equal(conflict.result.structuredContent.retryable, false)
    const contact = await call('get_customer', { contact_id: saved.id })
    assert.equal(contact.record.source, 'agent')
    assert.equal(contact.record.record_revision, 1)
    assert.equal((await call('get_customer_company', { contact_id: saved.id })).total, 1)
    const customerEvidence = { request_id: 'customer-evidence-' + protocol, contact_id: saved.id, evidence: {
      subject: 'Formal customer evidence', note: 'An unverified observation',
    } }
    const customerEvidenceSaved = await call('add_contact_evidence', customerEvidence)
    assert.deepEqual(await call('add_contact_evidence', customerEvidence), customerEvidenceSaved)
    const formalEvidence = await call('list_contact_evidence', { contact_id: saved.id })
    assert.equal(formalEvidence.rows[0].id, customerEvidenceSaved.id)
    assert.equal(formalEvidence.rows[0].status, 'pending')
    assert.deepEqual((await call('list_customer_fields')).fields, [])
    assert.equal((await call('get_customer_field_values', { contact_id: saved.id })).record.revision, 1)
    const stateCatalog = await call('get_customer_state_catalog')
    assert.ok(stateCatalog.options.some(item => item.field === 'customer_level' && item.code === 'A' && item.aliases.includes('重点客户')))
    const lead = await call('create_lead_candidate', {
      request_id: 'lead-' + protocol, display_name: 'MCP candidate ' + protocol,
      source_urls: ['https://example.test/candidate'], research_notes: 'Fixture research',
    })
    assert.ok((await call('list_lead_candidates')).rows.some(row => row.id === lead.id))
    const leadDetail = (await call('get_lead_candidate', { lead_id: lead.id })).record
    assert.equal(leadDetail.input.research_notes, 'Fixture research')
    assert.equal(leadDetail.record_revision, 1)
    const evidence = { request_id: 'evidence-' + protocol, lead_id: lead.id, evidence: {
      subject: 'Independent evidence', url: 'https://example.test/observations', note: 'Preliminary public information', observed_at: 1740000000,
    } }
    const evidenceId = await call('add_lead_evidence', evidence)
    assert.deepEqual(await call('add_lead_evidence', evidence), evidenceId)
    const evidenceList = await call('list_lead_evidence', { lead_id: lead.id })
    assert.equal(evidenceList.total, 2)
    const observed = evidenceList.rows.find(row => row.id === evidenceId.id)
    assert.equal(observed.observed_at, evidence.evidence.observed_at)
    assert.equal(observed.source, 'agent')
    assert.equal(observed.status, 'pending')
    assert.equal(evidenceList.verified, 0)
    const patch = await call('update_customer', {
      request_id: 'patch-' + protocol, contact_id: saved.id, stage: 'won',
      expected_revision: contact.record.record_revision, expected_epoch: contact.record.data_epoch,
      customer_level: '重点客户', sample_status: '样品运输中',
    })
    assert.equal(patch.id, saved.id)
    assert.equal((await call('get_customer', { contact_id: saved.id })).record.input.stage, 'won')
    const current = (await call('get_customer', { contact_id: saved.id })).record
    const correction = await call('suggest_customer_detail', {
      request_id: 'correction-' + protocol, contact_id: saved.id, expected_revision: current.record_revision, expected_epoch: current.data_epoch,
      detail: { type: 'correct_contact_method', method_id: current.input.methods[0].id, value: 'corrected-' + protocol + '@example.test' },
      evidence: { kind: 'research' },
    })
    assert.equal(correction.kind, 'agent_review_pending')
    assert.equal((await call('get_customer', { contact_id: saved.id })).record.input.methods[0].value, customer.email)
    const suggestion = {
      request_id: 'detail-' + protocol, contact_id: saved.id, expected_revision: current.record_revision, expected_epoch: current.data_epoch,
      detail: { type: 'product_interest', product_text: 'DC226', intent_quantity: 5, quantity_is_estimate: true },
      evidence: { kind: 'inferred', url: 'https://example.test/product' },
    }
    const pending = await call('suggest_customer_detail', suggestion)
    assert.equal(pending.kind, 'agent_review_pending')
    assert.equal(pending.count, 1)
    assert.deepEqual(await call('suggest_customer_detail', suggestion), pending)
    assert.deepEqual((await call('get_customer', { contact_id: saved.id })).record.input.interests, [])
    const shouldReview = await call('update_customer', {
      request_id: 'inferred-' + protocol, contact_id: saved.id, stage: 'lost',
      expected_revision: current.record_revision, expected_epoch: current.data_epoch, evidence: { kind: 'inferred' },
    })
    assert.equal(shouldReview.kind, 'agent_review_pending')
    assert.equal((await call('get_customer', { contact_id: saved.id })).record.input.stage, 'won')
    const priority = await call('list_customers', { quick_view: 'priority', level: 'A', sample_status: 'in_transit' })
    assert.ok(priority.rows.some(row => row.id === saved.id))
    const summary = priority.rows.find(row => row.id === saved.id)
    assert.deepEqual(Object.keys(summary.mail_outreach), ['status'])
    assert.ok(!('input' in summary) && !('custom_values' in summary))
    const views = await call('list_customer_views')
    const priorityView = views.views.find(view => view.id === 'priority')
    assert.equal(priorityView.config.query.quick_view, 'priority')
    const sameView = await call('query_customers', { ...priorityView.config.query,
      filter: { type: 'rule', field: 'stage', operator: 'eq', value: 'won' } })
    assert.ok(sameView.rows.some(row => row.id === saved.id))
    const stale = await send('tools/call', { name: 'update_customer', arguments: {
      request_id: 'stale-' + protocol, contact_id: saved.id, stage: 'contacted',
      expected_revision: contact.record.record_revision, expected_epoch: contact.record.data_epoch,
    } })
    assert.ok(stale.result?.isError, JSON.stringify(stale))
    assert.match(stale.result.content[0].text, /crm_revision_conflict/)
    assert.deepEqual((await call('get_customer_timeline', { contact_id: saved.id })).rows, [])
    const activity = {
      request_id: 'activity-' + protocol, contact_id: saved.id, event_type: 'note',
      occurred_at: 1700000000, summary: 'Agent fixture follow-up',
    }
    const logged = await call('add_customer_activity', activity)
    assert.deepEqual(await call('add_customer_activity', activity), logged)
    const timeline = await call('get_customer_timeline', { contact_id: saved.id })
    assert.equal(timeline.rows.length, 1)
    assert.equal(timeline.rows[0].source, 'agent')
    assert.equal(timeline.rows[0].id, logged.id)
    assert.equal(timeline.rows[0].summary, activity.summary)
    const batch = await call('add_customer_activities_batch', { items: [activity, { ...activity, request_id: 'missing-' + protocol, contact_id: 'missing' }] })
    assert.equal(batch.rows[0].id, logged.id)
    assert.equal(batch.rows[1].code, 'crm_not_found')
    assert.equal((await call('get_email_statuses', { items: ['missing'] })).rows[0].code, 'mail_not_found')
    const sample = { model: 'DC226', quantity: 2 }
    await call('add_customer_activity', { ...activity, request_id: 'sample-' + protocol, event_type: 'sample', sample })
    const samples = await call('get_customer_timeline', { contact_id: saved.id, event_type: 'sample' })
    assert.deepEqual(samples.rows[0].sample, sample)
    const templates = await call('list_email_templates', { purpose: 'first_outreach' })
    assert.equal(templates.rows[0].id, 'mcp-template-fixture')
    assert.deepEqual((await call('list_email_templates', { archived: true })).rows, [])
    const rendered = await call('render_email_template', { template_id: templates.rows[0].id, contact_id: saved.id })
    assert.equal(rendered.subject, 'Hello ' + customer.display_name)
    assert.deepEqual(rendered.missing, ['country_region'])
    assert.deepEqual((await call('get_pending_approvals')).rows, [])
    assert.deepEqual((await call('list_inbox')).rows, [])
    assert.deepEqual(await call('get_mailbox_sync_status'), { kind: 'sync_status', rows: [] })
    const forbidden = await send('tools/call', { name: 'approve_email', arguments: { draft_id: 'fixture' } })
    assert.ok(forbidden.error || forbidden.result?.isError)
    console.log('FTW stdio tool discovery, invocation and approval isolation passed for ' + protocol)
  } finally {
    const done = new Promise(resolve => child.once('exit', resolve))
    reader.close()
    child.stdin.end()
    if (child.exitCode === null) await done
  }
}

const core = spawn(coreExe, ['serve', '--dev-root', profile], { windowsHide: true, stdio: 'ignore' })
try {
  const health = await (async () => {
    for (let attempt = 0; attempt < 40; attempt++) {
      try {
        const { stdout } = await promisify(execFile)(coreExe, ['health', '--dev-root', profile], { windowsHide: true, timeout: 1000 })
        return JSON.parse(stdout)
      } catch {
        await delay(150)
      }
    }
    throw new Error('Core did not start for the isolated MCP fixture')
  })()
  assert.equal(health.schemaVersion, 27)
  await exercise('2025-06-18')
  for (const protocol of ['2025-06-18', '2025-11-25']) {
    const token = randomBytes(32).toString('hex')
    const db = new DatabaseSync(path.join(profile, 'data', 'workbench.db'))
    db.prepare('INSERT INTO mail_agent_clients(id,name,token_digest) VALUES (?,?,?)')
      .run(protocol, 'Fixture ' + protocol, createHash('sha256').update(token).digest())
    db.close()
    await exercise(protocol, token)
  }
} finally {
  core.kill()
}
