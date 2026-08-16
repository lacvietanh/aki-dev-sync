// ANSI-aware PTY byte-stream parser for SimpleView (docs/plan/wish-terminal-split-simpleview.md, BRIEF.md § Pinned module contracts).
// Stateful parser emitting 5 token types: text, nl, cr, up, eraseLine (dropping SGR colors, cursor addressing, scroll regions).
// 64-byte pending tail cap prevents memory growth on unterminated binary streams.

export function createAnsiParser() {
  /** @type {string} incomplete escape sequence or bare CR held across chunk boundaries */
  let pending = ''

  // Finds end index of escape sequence starting at start ('\x1b'); returns -1 if incomplete.
  function findEscEnd(str, start) {
    // ESC at end of chunk — defer
    if (start + 1 >= str.length) return -1

    if (str[start + 1] !== '[') {
      // Non-CSI escape — always 2 bytes total. Both present → strip.
      return start + 1
    }

    // CSI: ESC [ parameter-bytes(0x30-0x3F) intermediate-bytes(0x20-0x2F)* final-byte(0x40-0x7E)
    let i = start + 2
    while (i < str.length) {
      const c = str.charCodeAt(i)
      if (c >= 0x40 && c <= 0x7E) return i // final byte — sequence complete
      if (c < 0x20 || c > 0x7E) return -1 // byte outside CSI grammar — defer, let cap handle it
      // parameter or intermediate byte — keep scanning
      i++
    }
    return -1 // ran off end of chunk without final byte
  }

  /**
   * Interpret a complete CSI sequence (ESC [...final) and push zero or more tokens.
   * Only cursor-up and erase-line are honoured; everything else is silently dropped.
   */
  function handleCsi(seq, tokens) {
    // seq always starts with ESC[ and ends with a final byte
    const body = seq.slice(2, -1) // parameter + intermediate bytes between [ and final
    const final = seq[seq.length - 1]

    if (final === 'A') {
      // CSI <n>A — cursor up. Missing/empty param means n = 1.
      const n = body.length > 0 ? (parseInt(body, 10) || 1) : 1
      tokens.push({ t: 'up', n })
    } else if (final === 'K') {
      // CSI K / CSI 0K / CSI 1K / CSI 2K — erase line. All variants collapse.
      tokens.push({ t: 'eraseLine' })
    }
    // All other CSI sequences (SGR colours, cursor positioning, scroll regions, etc.) — drop
  }

  // Truncates string to at most max UTF-16 code units, never splitting a surrogate pair.
  function safeTruncate(str, max) {
    if (str.length <= max) return str
    let end = max
    if (end > 0) {
      const prev = str.charCodeAt(end - 1)
      if (prev >= 0xd800 && prev <= 0xdbff) end--
    }
    return str.slice(0, end)
  }

  return {
    // Feeds decoded PTY chunk and returns extracted tokens + deferred tail.
    feed(chunk) {
      const input = pending + chunk
      pending = ''
      const tokens = []
      let i = 0

      while (i < input.length) {
        const ch = input[i]

        if (ch === '\x1b') {
          const end = findEscEnd(input, i)
          if (end === -1) {
            // Incomplete escape sequence — save remainder as pending tail for next chunk
            pending = input.slice(i)
            break
          }
          // Complete escape (CSI or non-CSI) — interpret and consume
          handleCsi(input.slice(i, end + 1), tokens)
          i = end + 1
        } else if (ch === '\r') {
          if (i + 1 < input.length && input[i + 1] === '\n') {
            // CRLF → newline only (a bare CR that turned out to be CRLF)
            tokens.push({ t: 'nl' })
            i += 2
          } else if (i === input.length - 1) {
            // Bare CR at the very end of input — defer to next chunk.
            // The \r vs \r\n decision waits until the next chunk shows whether \n follows.
            pending = '\r'
            break
          } else {
            // Bare CR not followed by LF — emit cr token
            tokens.push({ t: 'cr' })
            i++
          }
        } else if (ch === '\n') {
          tokens.push({ t: 'nl' })
          i++
        } else {
          // Printable run — accumulate until the next control character
          const start = i
          while (i < input.length && input[i] !== '\x1b' && input[i] !== '\r' && input[i] !== '\n') {
            i++
          }
          tokens.push({ t: 'text', v: input.slice(start, i) })
        }
      }

      // Safety cap — pending tail at most 64 bytes, code-point-safe truncation
      if (pending.length > 64) {
        tokens.push({ t: 'text', v: safeTruncate(pending, 64) })
        pending = ''
      }

      return tokens
    },

    // Resets parser state and clears pending tail.
    reset() {
      pending = ''
    },
  }
}
