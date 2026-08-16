// Clipboard writer with fallback for non-secure HTTP LAN companion context (returns boolean success status).
export async function copyText(text) {
  if (text == null) return false
  const value = String(text)
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value)
      return true
    }
  } catch (e) {
    console.error('[clipboard] clipboard API refused, falling back', e)
  }
  try {
    const ta = document.createElement('textarea')
    ta.value = value
    // Off-screen but focusable: display:none/hidden makes the selection unreadable, and iOS needs a non-zero-size, non-readonly element for the selection to take.
    ta.setAttribute('readonly', '')
    ta.style.cssText = 'position:fixed; top:0; left:-9999px; opacity:0;'
    document.body.appendChild(ta)
    ta.select()
    ta.setSelectionRange(0, value.length)
    const ok = document.execCommand('copy')
    document.body.removeChild(ta)
    return ok
  } catch (e) {
    console.error('[clipboard] fallback copy failed', e)
    return false
  }
}
