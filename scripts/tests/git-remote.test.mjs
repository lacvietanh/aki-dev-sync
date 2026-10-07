// Pure-function coverage for src/utils/gitRemote.js, same runner convention as config-store.test.mjs.

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { remoteToWebUrl } from '../../src/utils/gitRemote.js'

test('scp-style SSH remote maps to its HTTPS page', () => {
  assert.equal(remoteToWebUrl('git@github.com:lacvietanh/tuvi.akinet.me.git'), 'https://github.com/lacvietanh/tuvi.akinet.me')
})

test('ssh:// remote drops user and port', () => {
  assert.equal(remoteToWebUrl('ssh://git@gitlab.com:2222/group/sub/repo.git'), 'https://gitlab.com/group/sub/repo')
})

test('HTTPS remote drops credentials and .git, keeps a name without .git', () => {
  assert.equal(remoteToWebUrl('https://user:token@github.com/lacvietanh/aki-dev-sync.git'), 'https://github.com/lacvietanh/aki-dev-sync')
  assert.equal(remoteToWebUrl('https://github.com/lacvietanh/app.akinet.me'), 'https://github.com/lacvietanh/app.akinet.me')
})

test('local path or empty remote has no web page', () => {
  assert.equal(remoteToWebUrl('/Volumes/DEV/repo.git'), '')
  assert.equal(remoteToWebUrl(''), '')
})
