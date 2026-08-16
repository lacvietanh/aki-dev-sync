// Plain-node test runner for src/utils/ansiStrip.js (BRIEF.md § Pinned module contracts).
import { createAnsiParser } from '../src/utils/ansiStrip.js'

const PASS = []
const FAIL = []

function check(name, fn) {
  try {
    fn()
    PASS.push(name)
    console.log(`PASS  ${name}`)
  } catch (e) {
    FAIL.push(name)
    console.log(`FAIL  ${name} — ${e.message}`)
  }
}

function deepEqual(a, b, label) {
  const sa = JSON.stringify(a)
  const sb = JSON.stringify(b)
  if (sa !== sb) throw new Error(`${label}: expected ${sb}, got ${sa}`)
}

// Case 1: 20-frame CR spinner parity (all-at-once vs split-chunk feeds).
check('CR spinner — one chunk vs many split chunks yield same tokens', () => {
  const frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷', '⣿', '⡿']
  const frameChunks = frames.map((f) => '\r' + f)

  const p1 = createAnsiParser()
  const r1 = [...p1.feed(frameChunks.join('')), ...p1.feed('')]

  const p2 = createAnsiParser()
  const r2 = []
  for (const fc of frameChunks) r2.push(...p2.feed(fc))
  r2.push(...p2.feed(''))

  deepEqual(r1, r2, 'spinner parity')
  if (!r1.every((tok) => tok.t === 'cr' || tok.t === 'text')) throw new Error('unexpected token type in spinner output')
  if (r1.some((tok) => tok.t === 'nl')) throw new Error('spinner should not emit nl')

  const texts = r1.filter((tok) => tok.t === 'text').map((tok) => tok.v)
  deepEqual(texts, frames, 'spinner glyphs in order')
})

// Case 2: Split CSI across chunks ("\x1b[2" + "Khello").
check('split CSI (ESC [ 2 then K) — recognised as eraseLine, no literal leak', () => {
  const p = createAnsiParser()
  const r1 = p.feed('\x1b[2')
  if (r1.length !== 0) throw new Error(`expected 0 tokens from incomplete CSI, got ${r1.length}`)

  const r2 = p.feed('Khello')
  deepEqual(r2, [{ t: 'eraseLine' }, { t: 'text', v: 'hello' }], 'split CSI recognition')
})

// Case 3: CRLF split across chunks ("before\r" + "\nafter").
check('CRLF split across chunks — one nl, zero cr', () => {
  const p = createAnsiParser()
  const r1 = p.feed('before\r')
  deepEqual(r1, [{ t: 'text', v: 'before' }], 'CRLF split — first chunk text only')

  const r2 = p.feed('\nafter')
  deepEqual(r2, [{ t: 'nl' }, { t: 'text', v: 'after' }], 'CRLF split — second chunk no cr, only nl')
})

// Case 4: Pending tail exceeding 64 bytes without terminator flushes at cap.
check('overloaded pending tail → flush as text at 64-byte cap, parser resets', () => {
  const p = createAnsiParser()
  const r1 = p.feed('\x1b[2')
  deepEqual(r1, [], 'incomplete CSI — nothing emitted')

  const r2 = p.feed('0'.repeat(100))
  if (r2.length !== 1 || r2[0].t !== 'text' || r2[0].v.length !== 64) {
    throw new Error('expected single 64-byte text flush on overflow')
  }

  const r3 = p.feed('hello')
  deepEqual(r3, [{ t: 'text', v: 'hello' }], 'parser clean after cap flush')
})

// Case 5: 4-line status box redrawn 7 times via \x1b[5A + \x1b[2K (ordering verification).
check('4-line status box redrawn 7 times — up+eraseLine ordering', () => {
  const boxLines = ['Status: OK', 'CPU: 42%', 'Mem: 3.2G', 'Tasks: 12']
  const p = createAnsiParser()
  p.feed(boxLines.join('\n') + '\n')

  const redraws = 7
  const allTokens = []

  for (let r = 0; r < redraws; r++) {
    allTokens.push(...p.feed('\x1b[5A'))
    for (const line of boxLines) {
      allTokens.push(...p.feed('\x1b[2K'))
      allTokens.push(...p.feed(line + '\n'))
    }
  }

  const upTokens = allTokens.filter((t) => t.t === 'up')
  const eraseTokens = allTokens.filter((t) => t.t === 'eraseLine')

  if (upTokens.length !== redraws) throw new Error(`expected ${redraws} up tokens, got ${upTokens.length}`)
  if (eraseTokens.length !== redraws * boxLines.length) throw new Error(`expected ${redraws * boxLines.length} eraseLine tokens`)
  if (!upTokens.every((u) => u.n === 5)) throw new Error('all up tokens must have n=5')

  let idx = 0
  for (let r = 0; r < redraws; r++) {
    const upTok = allTokens[idx++]
    if (upTok.t !== 'up' || upTok.n !== 5) throw new Error(`redraw ${r}: expected up n=5`)

    for (let l = 0; l < boxLines.length; l++) {
      const erTok = allTokens[idx++]
      if (erTok.t !== 'eraseLine') throw new Error(`redraw ${r} line ${l}: expected eraseLine`)
      const txtTok = allTokens[idx++]
      if (txtTok.t !== 'text' || txtTok.v !== boxLines[l]) throw new Error(`redraw ${r} line ${l}: text mismatch`)
      const nlTok = allTokens[idx++]
      if (nlTok.t !== 'nl') throw new Error(`redraw ${r} line ${l}: expected nl`)
    }
  }
  console.log(`  (${allTokens.length} total tokens, order verified)`)
})

console.log(`\n${PASS.length} passed, ${FAIL.length} failed`)
if (FAIL.length > 0) {
  console.log('FAILURES:')
  for (const f of FAIL) console.log(`  - ${f}`)
  process.exit(1)
}
