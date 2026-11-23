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
      'create_email_draft', 'get_customer', 'get_email_draft', 'list_customers',
      'list_email_accounts', 'list_email_drafts', 'request_email_approval', 'send_approved_email',
    ])
    assert.ok(tools.every(tool => tool.inputSchema.type === 'object'))
    const accounts = await send('tools/call', { name: 'list_email_accounts', arguments: {} })
    assert.deepEqual(JSON.parse(accounts.result.content[0].text), { kind: 'accounts', rows: [] })
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
    assert.equal(health.schemaVersion, 3)
    process.kill(health.processId)
  } catch (error) {
    if (error.code !== 1 && error.code !== 'ESRCH') throw error
  }
}
