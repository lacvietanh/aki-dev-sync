import { ref } from "vue";
import {
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
  availableMonitors,
  primaryMonitor,
  currentMonitor,
} from "@tauri-apps/api/window";

const PIN_STORAGE_KEY = "aki-devsync-pin-all-spaces";
const VIEW_REMEMBER_KEY = "aki-devsync-remember-view";
const VIEW_STORAGE_KEY = "aki-devsync-window-view";

const isPinned = ref(localStorage.getItem(PIN_STORAGE_KEY) === "true");
const rememberView = ref(localStorage.getItem(VIEW_REMEMBER_KEY) === "true");

function readSavedView() {
  try {
    return JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)) || {};
  } catch {
    return {};
  }
}

// The last preset picked on each axis, or {} when nothing is remembered. Reactive so the menu
// can show which preset is currently the remembered one instead of describing it in words.
const savedView = ref(readSavedView());

// Width presets (logical px). NARROW matches tauri.conf.json's minWidth (420) exactly.
const NARROW_WIDTH = 420;
const WIDE_WIDTH = 768;

// tauri.conf.json's windows[0].minHeight - mirrored here so the "Stick Top-Left"
// auto-fit never programmatically requests a height smaller than the OS-enforced floor.
const MIN_WINDOW_HEIGHT = 500;

/**
 * Sums the actual on-screen chrome + the project grid's natural (unclamped) content
 * height, so "Stick Top-Left" can size the window to fit the whole project list without
 * hardcoding a row-count guess. Falls back gracefully (0) for any piece not yet mounted.
 */
function measureRequiredContentHeight() {
  const heightOf = (selector) => document.querySelector(selector)?.offsetHeight || 0;
  const topHeader = heightOf(".top-header");
  const usageSection = heightOf(".agent-usage-section");
  const gridHeader = heightOf(".grid-header");
  const gridBody = document.querySelector(".grid-body")?.scrollHeight || 0;
  const dashboardBottom = heightOf(".dashboard-bottom");
  // Small buffer for borders/box-shadow rounding between the summed pieces.
  return topHeader + usageSection + gridHeader + gridBody + dashboardBottom + 4;
}

export function useAppWindow() {
  const appWindow = getCurrentWindow();

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

  /**
   * Sets the window width to `widthLogical`, keeping current height untouched, and only
   * nudges x back on-screen if the new width would push the window past the monitor's
   * work-area edge (never moves it otherwise).
   */
  async function setWidthPreset(widthLogical) {
    const [scaleFactor, outerSize, outerPos, monitor] = await Promise.all([
      appWindow.scaleFactor(),
      appWindow.outerSize(),
      appWindow.outerPosition(),
      currentMonitor(),
    ]);
    const heightLogical = outerSize.height / scaleFactor;
    await appWindow.setSize(new LogicalSize(widthLogical, heightLogical));

    if (!monitor) return;
    const widthPhysical = widthLogical * scaleFactor;
    const minX = monitor.workArea.position.x;
    const maxX = monitor.workArea.position.x + monitor.workArea.size.width;
    let x = outerPos.x;
    if (x + widthPhysical > maxX) x = maxX - widthPhysical;
    if (x < minX) x = minX;
    x = Math.round(x);
    if (x !== outerPos.x) {
      await appWindow.setPosition(new PhysicalPosition(x, outerPos.y));
    }
  }

  /**
   * The window presets the menu offers, grouped by the axis each one owns: `width` resizes,
   * `place` positions. Two axes rather than one flat list because they are independent - a
   * remembered "narrow" must survive picking "Center Primary" afterwards.
   */
  const VIEWS = {
    width: {
      narrow: () => setWidthPreset(NARROW_WIDTH),
      wide: () => setWidthPreset(WIDE_WIDTH),
    },
    place: {
      stick: () => stickTopLeft(),
      center: () => centerPrimary(),
    },
  };

  /**
   * The two presets ⌘1 / ⌘2 fire, one per column of the menu's 2x2 preset grid: the whole
   * column, both axes, so one keystroke lands a complete layout instead of half of one.
   */
  const VIEW_COMBOS = {
    1: { width: "narrow", place: "stick" },
    2: { width: "wide", place: "center" },
  };

  /** Applies one preset and, while "remember" is on, records it as that axis' saved choice. */
  function applyView(axis, name) {
    if (rememberView.value) {
      savedView.value = { ...savedView.value, [axis]: name };
      localStorage.setItem(VIEW_STORAGE_KEY, JSON.stringify(savedView.value));
    }
    return VIEWS[axis][name]();
  }

  /** Width before placement, for the same reason `restoreView` does it in that order. */
  async function applyViewCombo(slot) {
    const combo = VIEW_COMBOS[slot];
    if (!combo) return;
    await applyView("width", combo.width);
    await applyView("place", combo.place);
  }

  function toggleRememberView() {
    rememberView.value = !rememberView.value;
    localStorage.setItem(VIEW_REMEMBER_KEY, String(rememberView.value));
    // Turning it off drops what was remembered - leaving a stale preset behind would make the
    // next toggle-on silently restore a layout the user never picked in that session.
    if (!rememberView.value) {
      savedView.value = {};
      localStorage.removeItem(VIEW_STORAGE_KEY);
    }
  }

  /**
   * Replays the remembered presets on launch. Width first: "Stick Top-Left" measures height
   * against the *current* width, so restoring it before the width would fit the wrong layout.
   */
  async function restoreView() {
    if (!rememberView.value) return;
    const { width, place } = savedView.value;
    if (VIEWS.width[width]) await VIEWS.width[width]();
    if (VIEWS.place[place]) await VIEWS.place[place]();
  }

  /**
   * Moves the window flush against the top-left-most connected monitor's work area
   * (excludes the macOS menu bar / any docked taskbar, so "flush" doesn't tuck the
   * window under it) and resizes height so the whole project list fits without
   * scrolling, clamped to that monitor's available height.
   */
  async function stickTopLeft() {
    const monitors = await availableMonitors();
    if (!monitors.length) return;

    // "Topmost-leftmost" = smallest (x + y) among all connected monitors' origins  - 
    // not necessarily the primary monitor.
    const target = monitors.reduce((best, m) =>
      m.position.x + m.position.y < best.position.x + best.position.y ? m : best
    );

    const { position: workPos, size: workSize } = target.workArea;
    await appWindow.setPosition(new PhysicalPosition(workPos.x, workPos.y));

    const scaleFactor = target.scaleFactor;
    const outerSize = await appWindow.outerSize();
    const widthLogical = outerSize.width / scaleFactor;
    const maxHeightLogical = workSize.height / scaleFactor;

    const requiredHeight = measureRequiredContentHeight();
    const heightLogical = Math.max(
      MIN_WINDOW_HEIGHT,
      Math.min(requiredHeight, maxHeightLogical)
    );
    await appWindow.setSize(new LogicalSize(widthLogical, heightLogical));
  }

  /** Centers the window on the primary monitor specifically. Repositions only, no resize. */
  async function centerPrimary() {
    const monitor = await primaryMonitor();
    if (!monitor) return;
    const outerSize = await appWindow.outerSize();
    const x = Math.round(monitor.position.x + (monitor.size.width - outerSize.width) / 2);
    const y = Math.round(monitor.position.y + (monitor.size.height - outerSize.height) / 2);
    await appWindow.setPosition(new PhysicalPosition(x, y));
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
  };
}
