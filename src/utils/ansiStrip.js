/**
 * ANSI-aware PTY byte-stream parser for SimpleView.
 *
 * SimpleView never mounts xterm.js — it renders the PTY byte stream as a plain text line stream.
 * This parser strips ANSI escape sequences and emits only five token types: text, newline,
 * carriage-return, cursor-up, and line-erase. Everything else (SGR colours, cursor positioning,
 * scroll regions, all other CSI sequences) is silently dropped.
 *
 * The phone honours cursor-up, line-erase and carriage-return ONLY. It never implements columns,
 * cursor addressing, scroll regions, or any grid geometry, and never learns cols/rows.
 *
 * Parser is stateful because chunks can fragment an ANSI sequence or a CRLF pair across a
 * transport boundary. The pending tail bridges those gaps; a 64-byte cap prevents unbounded
 * memory growth when binary garbage arrives without a terminator.
 *
 * See docs/plan/wish-terminal-split-simpleview.md
 * See BRIEF.md § Pinned module contracts
 */

/**
 * @typedef {{ t: 'text', v: string }} TokenText
 * @typedef {{ t: 'nl' }} TokenNl
 * @typedef {{ t: 'cr' }} TokenCr
 * @typedef {{ t: 'up', n: number }} TokenUp
 * @typedef {{ t: 'eraseLine' }} TokenEraseLine
 * @typedef {TokenText | TokenNl | TokenCr | TokenUp | TokenEraseLine} Token
 */

/**
 * Create a stateful ANSI parser that emits tokens for SimpleView consumption.
 *
 * @returns {{ feed(chunk: string): Token[], reset(): void }}
 */
export function createAnsiParser() {
  /** @type {string} incomplete escape sequence or bare CR held across chunk boundaries */
  let pending = ''

  /**
   * Find the end index of an escape sequence starting at `start` (where `str[start] === '\x1b'`).
   *
   * Returns the index of the final byte for complete escapes, or -1 when the sequence is
   * incomplete (chunk ends before the final byte, or the bytes do not form a valid CSI sequence).
   *
   * Non-CSI escapes (ESC 7, ESC =, etc.) are always exactly two bytes — return `start + 1`
   * when both bytes are present. CSI sequences (ESC [...final) are variable-length and return
   * the index of the final byte after scanning.
   */
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

  /**
   * Truncate `str` to at most `max` UTF-16 code units, landing on a code-point boundary.
   * Never splits a surrogate pair.
   */
  function safeTruncate(str, max) {
    if (str.length <= max) return str
    let end = max
    if (end > 0) {
      const prev = str.charCodeAt(end - 1)
      // High surrogate at the cut point → the pair is split; back up one to drop it.
      if (prev >= 0xd800 && prev <= 0xdbff) end--
    }
    return str.slice(0, end)
  }

  return {
    /**
     * Feed a chunk of decoded PTY text through the parser.
     *
     * @param {string} chunk decoded UTF-8 text (real code points, not raw bytes) —
     *   the sole caller (usePtyStream.js) runs bytes through a TextDecoder first.
     *   Control sequences this parser matches are all ASCII, so this holds either way.
     * @returns {Token[]} tokens extracted from this chunk + any deferred tail from prior chunks
     */
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

    /**
     * Reset parser state — clears any pending tail.
     * Must be called alongside line/buffer clears in the consumer.
     */
    reset() {
      pending = ''
    },
  }
}
