/**
 * Plain-node test runner for src/utils/ansiStrip.js — no framework, no new dependencies.
 *
 * Tests the createAnsiParser() contract from BRIEF.md § Pinned module contracts:
 *   - CR-only spinner, one-chunk vs split-chunk parity
 *   - split escape sequence recognition
 *   - CRLF split across chunk boundary
 *   - overloaded pending tail → 64-byte cap with code-point-safe truncation
 *   - status box redraw (cursor-up + erase-line ordering)
 *
 * Run: node scripts/test-ansistrip.mjs
 */

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

// ---------------------------------------------------------------------------
// Case 1 — 20-frame CR-only spinner, one chunk vs many split chunks
//   Simulates a "⣾⣽⣻⢿⡿⣟⣯⣷" spinner where each frame is "\r<glyph>".
//   The glyph is UTF-8: ⣾ = three bytes 0xE2 0xA3 0xBE — this is printable,
//   not a control char, so the parser accumulates it as text after the CR.
// ---------------------------------------------------------------------------
check('CR spinner — one chunk vs many split chunks yield same tokens', () => {
  const frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏',
                  '⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷', '⣿', '⡿']
  // Each frame: \r + braille glyph
  const frameChunks = frames.map(f => '\r' + f)

  // One chunk
  const p1 = createAnsiParser()
  const allAtOnce = frameChunks.join('')
  const r1 = p1.feed(allAtOnce)
  // Also drain any pending
  const r1d = p1.feed('')

  // Many split chunks
  const p2 = createAnsiParser()
  const r2 = []
  for (const fc of frameChunks) {
    r2.push(...p2.feed(fc))
  }
  const r2d = p2.feed('')

  const combined1 = [...r1, ...r1d]
  const combined2 = [...r2, ...r2d]
  deepEqual(combined1, combined2, 'spinner parity')

  // Verify the tokens make sense: each frame emits {t:'cr'} then {t:'text',v:'<glyph>'}
  // for 20 frames that's 40 tokens. No `nl` tokens, no `pending` leftovers.
  const isAllCrOrText = combined1.every(tok => tok.t === 'cr' || tok.t === 'text')
  if (!isAllCrOrText) throw new Error('unexpected token type in spinner output')
  if (combined1.some(tok => tok.t === 'nl')) throw new Error('spinner should not emit nl')

  // Verify the glyphs are present in order
  const texts = combined1.filter(tok => tok.t === 'text').map(tok => tok.v)
  deepEqual(texts, frames, 'spinner glyphs in order')
})

// ---------------------------------------------------------------------------
// Case 2 — ESC [ 2 K split across two chunks
//   Chunk 1: "\x1b[2"   Chunk 2: "Khello"
//   Must emit eraseLine then text "hello". Never leaks literal "[2K" or "\x1b".
// ---------------------------------------------------------------------------
check('split CSI (ESC [ 2 then K) — recognised as eraseLine, no literal leak', () => {
  const p = createAnsiParser()
  const r1 = p.feed('\x1b[2')
  // First chunk yields nothing — it's an incomplete CSI
  if (r1.length !== 0) throw new Error(`expected 0 tokens from incomplete CSI, got ${r1.length}`)

  const r2 = p.feed('Khello')
  // Should emit eraseLine then text "hello"
  deepEqual(r2, [
    { t: 'eraseLine' },
    { t: 'text', v: 'hello' },
  ], 'split CSI recognition')
})

// ---------------------------------------------------------------------------
// Case 3 — CRLF split across chunk boundary
//   Chunk 1 ends with \r, Chunk 2 starts with \n
//   Must emit exactly one nl and zero cr.
// ---------------------------------------------------------------------------
check('CRLF split across chunks — one nl, zero cr', () => {
  const p = createAnsiParser()
  const r1 = p.feed('before\r')
  // Deferred \r — should emit text "before" and nothing for the CR
  deepEqual(r1, [{ t: 'text', v: 'before' }], 'CRLF split — first chunk text only')

  const r2 = p.feed('\nafter')
  // The deferred \r + the new \n → nl, then text "after"
  deepEqual(r2, [
    { t: 'nl' },
    { t: 'text', v: 'after' },
  ], 'CRLF split — second chunk no cr, only nl')
})

// ---------------------------------------------------------------------------
// Case 4 — Lone ESC followed by 100 filler bytes
//   The pending tail grows past 64 without a terminator → flushed as literal text,
//   parser state resets.
// ---------------------------------------------------------------------------
check('overloaded pending tail → flush as text at 64-byte cap, parser resets', () => {
  const p = createAnsiParser()
  // Feed incomplete CSI: ESC [ 2 — '2' (0x32) is a parameter byte, not a final byte,
  // so the sequence stays open.
  const r1 = p.feed('\x1b[2')
  deepEqual(r1, [], 'incomplete CSI — nothing emitted')

  // Feed 100 '0' bytes (0x30, all parameter bytes — never terminate the CSI).
  const r2 = p.feed('0'.repeat(100))

  // Pending grew to 102 bytes → capped at 64, flushed as text, parser reset.
  if (r2.length !== 1) throw new Error(`expected 1 token from cap flush, got ${r2.length}`)
  if (r2[0].t !== 'text') throw new Error(`expected text token, got ${r2[0].t}`)
  if (r2[0].v.length !== 64) throw new Error(`expected 64-byte flush, got ${r2[0].v.length}`)

  // Parser is clean — subsequent normal feed works
  const r3 = p.feed('hello')
  deepEqual(r3, [{ t: 'text', v: 'hello' }], 'parser clean after cap flush')
})

// ---------------------------------------------------------------------------
// Case 5 — 4-line status box redrawn 7 times via \x1b[5A + \x1b[2K
//   Each redraw: move up 5 lines, then erase 2K to clear the line.
//   The 5 up tokens and 2 eraseLine tokens must arrive in correct order.
// ---------------------------------------------------------------------------
check('4-line status box redrawn 7 times — up+eraseLine ordering', () => {
  // First draw: 4 lines of text
  const boxLines = ['Status: OK', 'CPU: 42%', 'Mem: 3.2G', 'Tasks: 12']
  // Redraw pattern: \x1b[5A (up 5 to go above box) then for each line: text + \x1b[2K + \n
  // Wait, \x1b[5A moves up 5 lines. But we have only 4 lines in the box.
  // The contract says: CSI <param>A emits up with n=param. We just verify tokens emit in order.
  // Let's construct: initial draw 4 lines, then 7 redraws each: \x1b[5A then 4 lines of text with \x1b[2K

  const p = createAnsiParser()

  // Initial draw: 4 lines with \n
  feedAll(p, boxLines.join('\n') + '\n')

  const redraws = 7
  let allTokens = []

  for (let r = 0; r < redraws; r++) {
    // Move up 5
    const t1 = p.feed('\x1b[5A')
    allTokens.push(...t1)
    // Redraw each line: eraseLine + text + nl
    for (const line of boxLines) {
      const te = p.feed('\x1b[2K')
      allTokens.push(...te)
      const tx = p.feed(line + '\n')
      allTokens.push(...tx)
    }
  }

  // We should have exactly 7 up-5 tokens and 28 eraseLine tokens (7 redraws × 4 lines)
  const upTokens = allTokens.filter(t => t.t === 'up')
  const eraseTokens = allTokens.filter(t => t.t === 'eraseLine')

  if (upTokens.length !== redraws) throw new Error(`expected ${redraws} up tokens, got ${upTokens.length}`)
  if (eraseTokens.length !== redraws * boxLines.length) {
    throw new Error(`expected ${redraws * boxLines.length} eraseLine tokens, got ${eraseTokens.length}`)
  }

  // Verify all up tokens have n=5
  for (const u of upTokens) {
    if (u.n !== 5) throw new Error(`expected up n=5, got n=${u.n}`)
  }

  // Verify order: first up then eraseLine-text-nl repeated for each line
  let idx = 0
  for (let r = 0; r < redraws; r++) {
    if (idx >= allTokens.length) throw new Error(`redraw ${r}: ran out of tokens`)
    const upTok = allTokens[idx++]
    if (upTok.t !== 'up' || upTok.n !== 5) {
      throw new Error(`redraw ${r}: expected up n=5 at position ${idx - 1}, got ${JSON.stringify(upTok)}`)
    }
    for (let l = 0; l < boxLines.length; l++) {
      const erTok = allTokens[idx++]
      if (erTok.t !== 'eraseLine') {
        throw new Error(`redraw ${r} line ${l}: expected eraseLine at position ${idx - 1}, got ${JSON.stringify(erTok)}`)
      }
      const txtTok = allTokens[idx++]
      if (txtTok.t !== 'text' || txtTok.v !== boxLines[l]) {
        throw new Error(`redraw ${r} line ${l}: expected text "${boxLines[l]}" at position ${idx - 1}, got ${JSON.stringify(txtTok)}`)
      }
      // CRLF in the \n — after text we fed line + '\n', which is CRLF in raw PTY.
      // Actually we just fed line + '\n' (no \r). So we get an nl token.
      const nlTok = allTokens[idx++]
      if (nlTok.t !== 'nl') {
        throw new Error(`redraw ${r} line ${l}: expected nl at position ${idx - 1}, got ${JSON.stringify(nlTok)}`)
      }
    }
  }
  console.log(`  (${allTokens.length} total tokens, order verified)`)
})

// ---------------------------------------------------------------------------
// Helper — feed all at once for convenience
// ---------------------------------------------------------------------------
function feedAll(p, str) { return p.feed(str) }

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------
console.log(`\n${PASS.length} passed, ${FAIL.length} failed`)
if (FAIL.length > 0) {
  console.log('FAILURES:')
  for (const f of FAIL) console.log(`  - ${f}`)
  process.exit(1)
}
