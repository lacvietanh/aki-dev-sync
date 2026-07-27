/**
 * Byte-size formatting for anything the backend reports in raw bytes.
 *
 * Rust never formats sizes — it returns `u64` and this module owns the presentation, including the
 * one genuinely platform-dependent part: the unit base. macOS Finder counts a GB as 1000^3; Windows
 * Explorer counts it as 1024^3 while still printing "GB". Showing a number that disagrees with the
 * user's own file manager makes the app look wrong even when its arithmetic is right, so the base
 * follows the host OS rather than picking a side.
 *
 * See docs/feat/claudecode-cleanup.md § Sizes.
 */

/**
 * Unit base for the current host. The app ships macOS-only today (CLAUDE.md § THIS PROJECT), so the
 * Windows branch is written but unreachable until a Windows bundle actually exists — it is here so
 * that shipping one is a build-target change, not a hunt for hardcoded 1000s.
 */
export function detectByteBase() {
  const ua = (typeof navigator !== 'undefined' && (navigator.userAgent || '')) || ''
  if (/Windows|Win32|Win64/i.test(ua)) return 1024
  return 1000
}

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

/**
 * Formats bytes for display. Sub-unit values stay integers ("512 B", "0 B"); anything larger gets
 * one decimal below 10 and none above, so a column of sizes stays the same visual width.
 *
 * @param {number} bytes
 * @param {number} [base] override the detected base — tests and unit-comparison views only
 */
export function formatBytes(bytes, base = detectByteBase()) {
  const n = Number(bytes)
  if (!Number.isFinite(n) || n <= 0) return '0 B'

  let value = n
  let unit = 0
  while (value >= base && unit < UNITS.length - 1) {
    value /= base
    unit++
  }
  if (unit === 0) return `${Math.round(value)} B`
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${UNITS[unit]}`
}
