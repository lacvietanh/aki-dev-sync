// One clipboard writer for the whole app.
//
// WHY THIS EXISTS: `navigator.clipboard` does not exist in a NON-SECURE context, which is exactly
// what the phone companion is (plain http over the LAN). Every call site that used
// `navigator.clipboard.writeText` alone was silently dead on the phone — the promise rejected into a
// console nobody has open. The textarea + `execCommand('copy')` path is deprecated but is the only
// one that works without https.
//
// The contract is a BOOLEAN, not a throw: every caller must have a visible failure path (a Toast, a
// dialog, anything the user can see), because a copy that silently does nothing is the bug this
// module was extracted to end.

/**
 * Copies `text` to the clipboard.
 * @param {string} text
 * @returns {Promise<boolean>} true when the text is on the clipboard, false when it is not.
 */
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
