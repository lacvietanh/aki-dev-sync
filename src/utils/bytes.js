// Byte-size presentation formatter (docs/feat/claudecode-cleanup.md § Sizes).
// Detects OS unit base: macOS Finder (1000^3) vs Windows (1024^3).
export function detectByteBase() {
  const ua = (typeof navigator !== 'undefined' && (navigator.userAgent || '')) || ''
  if (/Windows|Win32|Win64/i.test(ua)) return 1024
  return 1000
}

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

// Formats bytes for display (integers for sub-unit; 1 decimal below 10).
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
