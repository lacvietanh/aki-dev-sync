// Which panel the bottom dock is showing, plus the one cross-component gesture that targets the
// in-app terminal ("open this project in the in-app terminal", from ProjectTable's popup).
//
// SSOT-1 / SYNC-1 (docs/plan/done/remote-control.md §9): these refs deliberately live in a composable
// and NOT in src/store/*.js. services/mirror.js auto-discovers every `isRef` export under
// src/store/ and mirrors it — which is exactly wrong here: which tab a screen is looking at is
// navigation, local to each device, and mirroring it would yank the Mac's panel around whenever
// the phone switched tabs. The same reasoning already applies to `editingProject` /
// `showConfigModal`. `isLogExpanded` IS in the store and IS mirrored; that is pre-existing and
// intentional (panel geometry, not tab selection).
import { ref } from 'vue'
import { isLogExpanded } from '../store/logStore'

/** 'log' | 'terminal' — read/written by AppConsole.vue. */
export const activePanel = ref('log')

/** A directory the terminal should `cd` into as soon as it is mounted and started. Set by
 *  `openInAppTerminal`, consumed exactly once by TerminalView.vue. It is a queue-of-one rather
 *  than a direct call because the terminal may not be mounted yet at the moment of the click —
 *  switching the tab is what mounts it. */
export const pendingCd = ref(null)

export function useTerminalPanel() {
  /** Project popup → in-app terminal. Opens the dock, switches to the TERMINAL tab and queues a
   *  `cd` into the project. On a phone this is the whole point of the feature: reaching a
   *  project's shell without touching the Mac. */
  function openInAppTerminal(localPath) {
    isLogExpanded.value = true
    activePanel.value = 'terminal'
    if (localPath) pendingCd.value = localPath
  }

  return { activePanel, pendingCd, openInAppTerminal }
}
