# Right-Dock Terminal Layout — feature

> updated 2026-08-17 · v1.25.0

Dedicated two-column layout on wide screens (window width ≥ 900px): moves the in-app terminal stack out of the bottom dock into a dedicated, full-height right-side column taking 100% of remaining width (`flex: 1`), while the main project list column is capped at 440px.

Cross-reference: [In-app Terminal](in-app-terminal.md) § Right-Dock Layout.

---

## 1. Why it exists

When running on a large desktop monitor (1440px, 1920px+), a bottom dock consumes vertical space from the project table while underutilizing horizontal screen real estate. The Right-Dock layout turns the application into a side-by-side workspace:
- **Left column (`dashboard-left`)**: Holds Agent Usage, Project List Table, and the collapsed/docked Global Event Log.
- **Right column (`AppConsole` / `.dashboard-bottom`)**: Dedicated interactive terminal pane filling all remaining horizontal space.

On narrow screens (< 900px) or mobile companion viewports, the application returns to the single-column bottom dock layout.

---

## 2. Core Architecture & SSoT

### Breakpoint & Width SSoT Constants
1. **Right-Dock Trigger**: `RIGHT_DOCK_BREAKPOINT = 900px` (window width).
   - Driven reactively in JS by `src/composables/useRightDockLayout.js` via a `ResizeObserver`.
   - Single source of truth: `rightDockActive` (ref boolean) is consumed across the app (`App.vue`, `AppConsole.vue`).
2. **Main View Cap**: `MAIN_VIEW_MAX_WIDTH = 440px`.
   - Synchronized with `NARROW_WIDTH = 440px` in `src/composables/useAppWindow.js` and `"minWidth": 440` in `src-tauri/tauri.conf.json`.
   - Injected as CSS variable `--main-view-max-width: 440px` on `document.documentElement`.

---

## 3. Breakpoint & Layout Impact Matrix

| File / Component | Role in Right-Dock & Narrow Breakpoint |
| :--- | :--- |
| **`src/composables/useRightDockLayout.js`** | **SSoT**: Exports `MAIN_VIEW_MAX_WIDTH = 440`, `RIGHT_DOCK_BREAKPOINT = 900`, and `rightDockActive` ref. |
| **`src/composables/useAppWindow.js`** | **SSoT**: `NARROW_WIDTH = 440` preset, synchronizing window resize with the mainview cap. |
| **`src-tauri/tauri.conf.json`** | Sets window `"minWidth": 440` to maintain layout floor integrity. |
| **`src/assets/main.css`** | Defines container query `container-name: main-view; container-type: inline-size;` on `.dashboard-left`. Sets `.dashboard-main.is-right-dock` to `flex-direction: row`, caps `.dashboard-left` to `var(--main-view-max-width, 440px)`, and sets `.dashboard-bottom` to `flex: 1`. |
| **`src/assets/main.css` (Utility Queries)** | Container-first hide/show rules: `.u-wide-hide` defaults to `none`, switches to `inline !important` under `@container main-view (max-width: 700px)` or `@media (max-width: 700px)`. Prevents dual-hiding bugs when window is wide but container is narrow. |
| **`src/App.vue`** | Toggles `.is-right-dock` class on `.dashboard-main`. Conditionally mounts `<LogStack v-if="rightDockActive" />` inside `.dashboard-left`. |
| **`src/components/AppConsole.vue`** | Serves as the right-side terminal container in right-dock mode; hides splitter controls and maximize toggles for dedicated terminal display. |
| **`src/components/AppHeader.vue`** | Sits at the root window level above `.dashboard-main`, spanning full window width. Window controls and quick actions stay at top right. |
| **`src/components/ProjectTable.vue`** | Responsive layout driven by `@container main-view (max-width: 700px)`, ensuring identical column alignment in both 440px narrow window and 440px right-dock container. |
| **`src/components/AgentUsageSection.vue`** | Driven by `@container main-view (max-width: 700px)` for compact 2-column slot layout. |
| **`src/components/modals/ClaudeCleanupModal.vue`** | `container-style="width: 440px;"` matching narrow floor. |
| **`src/components/modals/GeminiAllowlistModal.vue`** | `container-style="width: 440px;"` matching narrow floor. |

---

## 4. Cross References

- **In-app Terminal feature**: [docs/feat/in-app-terminal.md](in-app-terminal.md)
- **Terminal Stack Architecture**: [docs/arch/terminal-stack.md](../arch/terminal-stack.md)
- **Right-dock Research**: [docs/research/terminal-stack-right-dock.md](../research/terminal-stack-right-dock.md)
- **Milestone & Done Plan**: [docs/plan/done/remaining-1.25.md](../plan/done/remaining-1.25.md) § Item 3 (`TERM-STACK-R`)
- **Original Wish Plan**: [docs/plan/done/wish-terminal-split-simpleview.md](../plan/done/wish-terminal-split-simpleview.md) § Right-Dock Mac
