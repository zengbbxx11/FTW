import assert from 'node:assert/strict'
import { execFile, spawn } from 'node:child_process'
import { mkdir, mkdtemp } from 'node:fs/promises'
import path from 'node:path'
import { createInterface } from 'node:readline'
import { promisify } from 'node:util'

const target = path.resolve(process.argv[2] ?? 'target')
const coreExe = path.join(target, 'debug', 'ftw-core.exe')
const mcpExe = path.join(target, 'debug', 'ftw-mcp.exe')
const profiles = path.resolve('.ftw-dev')
await mkdir(profiles, { recursive: true })
const profile = await mkdtemp(path.join(profiles, 'agent-protocol-'))

async function exercise(protocol) {
  const child = spawn(mcpExe, ['--dev-root', profile], { windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'] })
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
      'add_customer_activity', 'create_customer', 'create_email_draft', 'create_lead_candidate',
      'get_customer', 'get_customer_state_catalog', 'get_customer_timeline', 'get_email_draft', 'get_email_thread',
      'get_pending_approvals', 'list_customers', 'list_email_accounts', 'list_email_drafts',
      'list_inbox', 'list_lead_candidates', 'request_email_approval', 'send_approved_email',
      'sync_mailbox', 'update_customer',
    ])
    assert.ok(tools.every(tool => tool.inputSchema.type === 'object'))
    const accounts = await send('tools/call', { name: 'list_email_accounts', arguments: {} })
    assert.deepEqual(JSON.parse(accounts.result.content[0].text), { kind: 'accounts', rows: [] })
    async function call(name, fields = {}) {
      const response = await send('tools/call', { name, arguments: fields })
      assert.ok(!response.error && !response.result?.isError, JSON.stringify(response))
      return JSON.parse(response.result.content[0].text)
    }
    const customer = { request_id: 'fixture-' + protocol, display_name: 'MCP fixture ' + protocol }
    const saved = await call('create_customer', customer)
    assert.equal(saved.kind, 'saved')
    assert.deepEqual(await call('create_customer', customer), saved)
    const conflict = await send('tools/call', { name: 'create_customer', arguments: { ...customer, display_name: 'changed' } })
    assert.ok(conflict.error || conflict.result?.isError)
    const contact = await call('get_customer', { contact_id: saved.id })
    assert.equal(contact.record.source, 'agent')
    assert.equal(contact.record.record_revision, 1)
    const stateCatalog = await call('get_customer_state_catalog')
    assert.ok(stateCatalog.options.some(item => item.field === 'customer_level' && item.code === 'A' && item.aliases.includes('重点客户')))
    const lead = await call('create_lead_candidate', {
      request_id: 'lead-' + protocol, display_name: 'MCP candidate ' + protocol,
      source_urls: ['https://example.test/candidate'], research_notes: 'Fixture research',
    })
    assert.ok((await call('list_lead_candidates')).rows.some(row => row.id === lead.id))
    const patch = await call('update_customer', {
      request_id: 'patch-' + protocol, contact_id: saved.id, stage: 'won',
      expected_revision: contact.record.record_revision, expected_epoch: contact.record.data_epoch,
      customer_level: '重点客户', sample_status: '样品运输中',
    })
    assert.equal(patch.id, saved.id)
    assert.equal((await call('get_customer', { contact_id: saved.id })).record.input.stage, 'won')
    const priority = await call('list_customers', { quick_view: 'priority', level: 'A', sample_status: 'in_transit' })
    assert.ok(priority.rows.some(row => row.id === saved.id))
    const stale = await send('tools/call', { name: 'update_customer', arguments: {
      request_id: 'stale-' + protocol, contact_id: saved.id, stage: 'contacted', expected_revision: contact.record.record_revision,
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
    assert.deepEqual((await call('get_pending_approvals')).rows, [])
    assert.deepEqual((await call('list_inbox')).rows, [])
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

try {
  await exercise('2025-06-18')
  await exercise('2025-11-25')
} finally {
  try {
    const { stdout } = await promisify(execFile)(coreExe, ['health', '--dev-root', profile], { windowsHide: true, timeout: 3000 })
    const health = JSON.parse(stdout)
    process.kill(health.processId)
    assert.equal(health.schemaVersion, 6)
  } catch (error) {
    if (error.code !== 1 && error.code !== 'ESRCH') throw error
  }
}
