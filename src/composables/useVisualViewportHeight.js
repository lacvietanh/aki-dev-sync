// Publishes the live visible viewport height as `--vvh` on the root element, so `vh`-based layout
// math (main.css's `.dashboard-layout`/`.dashboard-bottom`, useDockLayout.js's height expressions)
// can be rewritten to track the ACTUAL visible area instead of the layout viewport.
//
// Why this exists: on iOS Safari the CSS `vh` unit is computed against the layout viewport, which
// does NOT shrink when the on-screen keyboard opens - only `window.visualViewport.height` does.
// Without this, a 100vh app shell never adjusts and the keyboard simply covers whatever sits at its
// bottom edge (docs/plan/done/terminal-mobile-keyboard-viewport.md §1, F1-F3).
//
// No host/companion branch (ENV-1's capability-pattern spirit): on the Mac, `visualViewport.height`
// already always equals the real window height (no on-screen keyboard ever changes it), so this is
// inert there without needing to be told so.
import { onMounted, onUnmounted } from 'vue'

function publish(px) {
  document.documentElement.style.setProperty('--vvh', `${px}px`)
}

export function useVisualViewportHeight() {
  const vv = window.visualViewport

  function onResize() {
    publish(vv.height)
  }

  onMounted(() => {
    if (!vv) {
      // No visualViewport support (old WebKit / desktop without it): main.css's dvh/vh fallback chain covers this case, nothing to publish.
      return
    }
    publish(vv.height)
    // `scroll` too, not just `resize`: iOS has a documented bug where `visualViewport.offsetTop` does not reset to 0 when the keyboard closes, which only a scroll listener catches.
    vv.addEventListener('resize', onResize)
    vv.addEventListener('scroll', onResize)
  })

  onUnmounted(() => {
    if (!vv) return
    vv.removeEventListener('resize', onResize)
    vv.removeEventListener('scroll', onResize)
  })
}
