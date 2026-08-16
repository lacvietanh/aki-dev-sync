// Publishes live visible viewport height as `--vvh` on root element for layout math (.dashboard-layout, .dashboard-bottom, useDockLayout.js) to track actual visible area instead of layout viewport.
// Why: on iOS Safari CSS `vh` is computed against layout viewport (which does not shrink on keyboard open; only `window.visualViewport.height` shrinks), covering app bottom (docs/plan/done/terminal-mobile-keyboard-viewport.md §1, F1-F3).
// Capability pattern (ENV-1): on desktop/Mac `visualViewport.height` always equals window height so this is inert without host/companion branching.
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
      // No visualViewport support (old WebKit/desktop fallback): main.css dvh/vh chain covers this case.
      return
    }
    publish(vv.height)
    // iOS bug workaround: visualViewport.offsetTop does not reset to 0 when keyboard closes without a scroll listener.
    vv.addEventListener('resize', onResize)
    vv.addEventListener('scroll', onResize)
  })

  onUnmounted(() => {
    if (!vv) return
    vv.removeEventListener('resize', onResize)
    vv.removeEventListener('scroll', onResize)
  })
}
