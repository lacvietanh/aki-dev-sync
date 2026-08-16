import { ref } from "vue";
import {
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
  availableMonitors,
  primaryMonitor,
  currentMonitor,
} from "@tauri-apps/api/window";
// Boundary (ENV-1, docs/plan/done/remote-control.md §9): native window control is meaningless on a companion.
import { isHost } from "../services/bridge";

const PIN_STORAGE_KEY = "aki-devsync-pin-all-spaces";
const VIEW_REMEMBER_KEY = "aki-devsync-remember-view";
// `-pt` = logical points; an earlier build stored physical px, which read as a different window per display scale.
const BOUNDS_STORAGE_KEY = "aki-devsync-window-bounds-pt";
const LEGACY_KEYS = ["aki-devsync-window-view", "aki-devsync-window-bounds"];

const isPinned = ref(localStorage.getItem(PIN_STORAGE_KEY) === "true");
const rememberView = ref(localStorage.getItem(VIEW_REMEMBER_KEY) === "true");

function readSavedBounds() {
  try {
    const raw = JSON.parse(localStorage.getItem(BOUNDS_STORAGE_KEY));
    if (!raw) return null;
    const { width, height, x, y } = raw;
    if (![width, height, x, y].every(Number.isFinite)) return null;
    if (width <= 0 || height <= 0) return null;
    return { width, height, x, y };
  } catch {
    return null;
  }
}

// The exact window geometry to restore next launch, all in logical points.
const savedBounds = ref(readSavedBounds());

// Session-only menu-highlight state, not restored on next launch.
const savedView = ref({});

// Width presets (logical px). NARROW matches tauri.conf.json's minWidth (440) exactly.
const NARROW_WIDTH = 440;
const WIDE_WIDTH = 768;
const ULTRAWIDE_WIDTH = 1400;

/** Companion shape: same 13 keys as host (AppHeader destructures all — missing key crashes); the three async ones must return a Promise (call sites use `.catch`). */
function companionWindow() {
  const noop = () => {};
  const noopAsync = () => Promise.resolve();
  return {
    startDragging: noop,
    minimize: noop,
    closeWin: noop,
    isPinned,
    togglePin: noop,
    restorePin: noop,
    applyView: noopAsync,
    applyViewCombo: noopAsync,
    savedView,
    rememberView,
    toggleRememberView: noop,
    restoreView: noopAsync,
    nativeWindow: false,
  };
}

export function useAppWindow() {
  // getCurrentWindow() throws on a companion (no __TAURI_INTERNALS__) and takes the app down — bail out first.
  if (!isHost) return companionWindow();

  LEGACY_KEYS.forEach((k) => localStorage.removeItem(k));

  const appWindow = getCurrentWindow();

  // Own scale factor, not a monitor's: outerSize/outerPosition both measure this window.
  async function captureBounds() {
    const [scaleFactor, outerSize, outerPos] = await Promise.all([
      appWindow.scaleFactor(),
      appWindow.outerSize(),
      appWindow.outerPosition(),
    ]);
    const size = outerSize.toLogical(scaleFactor);
    const pos = outerPos.toLogical(scaleFactor);
    const bounds = { width: size.width, height: size.height, x: pos.x, y: pos.y };
    savedBounds.value = bounds;
    localStorage.setItem(BOUNDS_STORAGE_KEY, JSON.stringify(bounds));
  }

  function minimize() {
    appWindow.minimize();
  }

  function closeWin() {
    appWindow.close();
  }

  function startDragging() {
    appWindow.startDragging();
  }

  function applyPinned(pinned) {
    appWindow.setAlwaysOnTop(pinned);
    appWindow.setVisibleOnAllWorkspaces(pinned);
  }

  function togglePin() {
    isPinned.value = !isPinned.value;
    localStorage.setItem(PIN_STORAGE_KEY, String(isPinned.value));
    applyPinned(isPinned.value);
  }

  function restorePin() {
    if (isPinned.value) applyPinned(true);
  }

  /** Sets width to widthLogical capped at the work area, keeps height; nudges x back on-screen only if the new width would push past the monitor edge. */
  async function setWidthPreset(widthLogical) {
    const [scaleFactor, outerSize, outerPos, monitor] = await Promise.all([
      appWindow.scaleFactor(),
      appWindow.outerSize(),
      appWindow.outerPosition(),
      currentMonitor(),
    ]);
    const heightLogical = outerSize.toLogical(scaleFactor).height;

    if (!monitor) {
      await appWindow.setSize(new LogicalSize(widthLogical, heightLogical));
      return;
    }
    const waPos = monitor.workArea.position.toLogical(monitor.scaleFactor);
    const waSize = monitor.workArea.size.toLogical(monitor.scaleFactor);
    // Cap before resizing: a preset wider than the display would otherwise hang off the right edge, and nudging x alone cannot bring it back.
    const targetWidth = Math.min(widthLogical, waSize.width);
    await appWindow.setSize(new LogicalSize(targetWidth, heightLogical));

    const outerPosLogical = outerPos.toLogical(scaleFactor);
    const minX = waPos.x;
    const maxX = waPos.x + waSize.width;
    let x = outerPosLogical.x;
    if (x + targetWidth > maxX) x = maxX - targetWidth;
    if (x < minX) x = minX;
    if (x !== outerPosLogical.x) {
      await appWindow.setPosition(new LogicalPosition(x, outerPosLogical.y));
    }
  }

  /** Menu presets grouped by independent axis (`width` resizes, `place` positions) — e.g. narrow must survive Center Primary. */
  const VIEWS = {
    width: {
      narrow: () => setWidthPreset(NARROW_WIDTH),
      wide: () => setWidthPreset(WIDE_WIDTH),
      ultrawide: () => setWidthPreset(ULTRAWIDE_WIDTH),
    },
    place: {
      stick: () => stickTopLeft(),
      center: () => centerPrimary(),
    },
  };

  /** A combo fires a full grid column (both axes) so one keystroke lands a complete layout. */
  const VIEW_COMBOS = {
    1: { width: "narrow", place: "stick" },
    2: { width: "wide", place: "center" },
  };

  /** Applies one preset and, while "remember" is on, snapshots the bounds it produced. */
  async function applyView(axis, name) {
    savedView.value = { ...savedView.value, [axis]: name };
    await VIEWS[axis][name]();
    if (rememberView.value) await captureBounds();
  }

  /** Width before placement: `setWidthPreset` may nudge x to keep the window on-screen, so running it after a placement would undo that placement. */
  async function applyViewCombo(slot) {
    const combo = VIEW_COMBOS[slot];
    if (!combo) return;
    await applyView("width", combo.width);
    await applyView("place", combo.place);
  }

  // Turning off doesn't erase saved bounds; turning back on always re-captures fresh.
  function toggleRememberView() {
    rememberView.value = !rememberView.value;
    localStorage.setItem(VIEW_REMEMBER_KEY, String(rememberView.value));
    if (rememberView.value) captureBounds().catch((e) => console.error("Failed to capture window bounds:", e));
  }

  /** Re-applies the remembered bounds, clamping both to the target work area so a monitor that is now gone or smaller still lands the window fully on-screen. */
  async function restoreView() {
    if (!rememberView.value) return;
    const bounds = savedBounds.value;
    // Flag on but nothing stored yet — capture now, or the toggle sits ticked and inert.
    if (!bounds) {
      await captureBounds().catch((e) => console.error("Failed to capture window bounds:", e));
      return;
    }

    const monitors = await availableMonitors();
    if (!monitors.length) return;
    const holdsOrigin = (m) => {
      const waPos = m.workArea.position.toLogical(m.scaleFactor);
      const waSize = m.workArea.size.toLogical(m.scaleFactor);
      return (
        bounds.x >= waPos.x &&
        bounds.y >= waPos.y &&
        bounds.x < waPos.x + waSize.width &&
        bounds.y < waPos.y + waSize.height
      );
    };
    const target = monitors.find(holdsOrigin) || (await primaryMonitor()) || monitors[0];

    const waPos = target.workArea.position.toLogical(target.scaleFactor);
    const waSize = target.workArea.size.toLogical(target.scaleFactor);
    // Clamp to the target work area first — a smaller destination monitor must not leave the far edge off-screen.
    const width = Math.min(bounds.width, waSize.width);
    const height = Math.min(bounds.height, waSize.height);
    const maxX = waPos.x + waSize.width - width;
    const maxY = waPos.y + waSize.height - height;
    const x = Math.max(waPos.x, Math.min(bounds.x, maxX));
    const y = Math.max(waPos.y, Math.min(bounds.y, maxY));
    // Position before size to avoid a visible resize-then-jump flash at launch.
    await appWindow.setPosition(new LogicalPosition(x, y));
    await appWindow.setSize(new LogicalSize(width, height));
  }

  /** Moves the window flush against the top-left-most monitor's work area, spanning its full height. */
  async function stickTopLeft() {
    const monitors = await availableMonitors();
    if (!monitors.length) return;

    // Topmost-leftmost = smallest (x + y) among monitor origins, not necessarily the primary.
    const target = monitors.reduce((best, m) =>
      m.position.x + m.position.y < best.position.x + best.position.y ? m : best
    );

    const workPos = target.workArea.position.toLogical(target.scaleFactor);
    const workSize = target.workArea.size.toLogical(target.scaleFactor);
    await appWindow.setPosition(new LogicalPosition(workPos.x, workPos.y));

    const outerSize = await appWindow.outerSize();
    const widthLogical = outerSize.toLogical(await appWindow.scaleFactor()).width;
    await appWindow.setSize(new LogicalSize(widthLogical, workSize.height));
  }

  /** Centers the window on the primary monitor specifically. Repositions only, no resize. */
  async function centerPrimary() {
    const monitor = await primaryMonitor();
    if (!monitor) return;
    const monitorPos = monitor.position.toLogical(monitor.scaleFactor);
    const monitorSize = monitor.size.toLogical(monitor.scaleFactor);
    const outerSize = (await appWindow.outerSize()).toLogical(await appWindow.scaleFactor());
    const x = monitorPos.x + (monitorSize.width - outerSize.width) / 2;
    const y = monitorPos.y + (monitorSize.height - outerSize.height) / 2;
    await appWindow.setPosition(new LogicalPosition(x, y));
  }

  return {
    minimize,
    closeWin,
    startDragging,
    isPinned,
    togglePin,
    restorePin,
    applyView,
    applyViewCombo,
    savedView,
    rememberView,
    toggleRememberView,
    restoreView,
    nativeWindow: true,
  };
}
