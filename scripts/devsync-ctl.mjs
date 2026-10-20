#!/usr/bin/env node
// Client for the debug build's control socket (src-tauri/src/control.rs); usage in docs/ref/agent-control.md.
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'

// app_paths::app_data_dir() of a debug build + control.rs SOCKET_NAME.
const SOCKET = path.join(os.homedir(), '.aki', 'devsync-dev', 'control.sock')

const [op, ...rest] = process.argv.slice(2)
if (!op) {
  console.error('usage: npm run ctl -- <op> [arg]\nops: status, remote.on, remote.off, remote.status, eval "<JS function body>"')
  process.exit(2)
}

const request = { id: 1, op, ...(rest.length ? { arg: rest.join(' ') } : {}) }
const socket = net.createConnection(SOCKET)
let buffer = ''

socket.on('connect', () => socket.write(JSON.stringify(request) + '\n'))
socket.on('data', (chunk) => {
  buffer += chunk
  const newline = buffer.indexOf('\n')
  if (newline < 0) return
  const reply = JSON.parse(buffer.slice(0, newline))
  socket.end()
  if (!reply.ok) {
    console.error(`error: ${reply.error}`)
    process.exit(1)
  }
  console.log(JSON.stringify(reply.data, null, 2))
})
socket.on('error', (e) => {
  console.error(`cannot reach ${SOCKET}: ${e.code || e.message}. Start the debug app first: npm run dev:agent`)
  process.exit(1)
})
