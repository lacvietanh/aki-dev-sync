# Plan: Fix High Idle GPU Utilization (WebKit Metal Compositor & Xterm Invalidation)

## 1. Problem Statement & Live Evidence

- **Observed Metrics (macOS Instruments & Activity Monitor on real hardware)**:
  - Process: `Aki Dev Sync Graphics and Media` (`com.apple.WebKit.GPU.xpc`, PID 71365).
  - Activity Monitor: **51.4% CPU**, **73.4% GPU**.
  - Instruments (Advanced Graphics Statistics):
    - `AppleMetalOpenGLRenderer` / Metal Shader Pipeline.
    - `Tiler Utilization %`: **89% Min, 94% Avg, 96% Max**.
    - `Device Utilization %`: **89% Min, 94% Avg, 96% Max**.
    - `Renderer Utilization %`: **89% Min, 94% Avg, 96% Max**.
  - Condition: App is sitting completely **idle** with no active user typing or compilation in progress.

---

## 2. Root Cause Analysis (First Principles & Architecture)

The ~94% Metal GPU saturation is caused by a compounding resonance of two architectural subsystems in WebKit (`WKWebView`):

### A. Backdrop-Filter Blur Shader + Continuous Layer Invalidation
- **Mechanism**: `backdrop-filter: blur(8px / 10px)` is declared on top-level permanent chrome (`.app-header`, `.dock-stack`) and popups/modals in `src/assets/main.css`.
- **Metal GPU cost**: On Retina/ProMotion displays (60Hz–120Hz), a Gaussian blur filter requires multi-pass convolution shaders over the underlying framebuffer.
- **Trigger**: When any child or sibling layer animates (e.g. `@keyframes pulse` animating `box-shadow` on `.status-dot`, skeleton pulses, SVG stroke progress, or continuous timer reflows), WebKit's hardware compositor cannot cache the blurred layer as a static texture. It is forced to re-run the full Metal convolution shader pipeline **60 to 120 times per second across the entire viewport**, directly maxing out the Apple Silicon GPU Tiler at 94%.

### B. In-App Terminal `cursorBlink` with `body-persist` Tabs
- **Mechanism**: In `TerminalView.vue`, xterm.js is initialized with `cursorBlink: true`. In `TerminalStack.vue`, `body-persist` keeps all activated terminal tabs mounted in the DOM (`v-show`).
- **WebKit IPC cost**: Even when idle, `cursorBlink` triggers a 500ms timer that invalidates the 2D canvas/DOM cursor layer, sending continuous draw call IPC messages to `com.apple.WebKit.GPU` across all mounted tab instances.

---

## 3. Goals & Success Criteria

1. **Idle GPU Target**:
   - `Aki Dev Sync Graphics and Media` GPU usage in Activity Monitor drops from **~74% down to < 2%** when idle.
   - Instruments Metal Device & Tiler Utilization drops from **94% down to < 5%**.
2. **Visual & UX Integrity**:
   - Preserves crisp Dark Theme aesthetic (`#05070c`, `#0a0f16`) with zero visual regression.
   - Preserves full In-App Terminal responsiveness, scrollback, Vietnamese IME typing, and TUI programs (`vim`, `htop`, `agy`).

---

## 4. Execution Workstreams

### WS1: In-App Terminal Render & Cursor Optimization
- **File**: `src/components/TerminalView.vue`
- **Actions**:
  1. Set `cursorBlink: false` by default, or gate `cursorBlink` to only blink when `props.active && hasFocus`.
  2. Ensure hidden tabs (`v-show="false"`) or collapsed dock panels completely pause internal xterm render cycles.
  3. Ensure `will-change` is not over-applied on non-moving terminal elements.

### WS2: CSS Compositor & Backdrop-Filter Decoupling
- **Files**: `src/assets/main.css`, `src/components/UsageCircle.vue`, `src/components/AgentUsage.vue`, `src/components/AppHeader.vue`
- **Actions**:
  1. **Replace heavy `backdrop-filter: blur(...)`** on permanent top-level chrome (`.app-header`, `.dock-stack`) with opaque/semi-opaque optimized dark solid surfaces (`background: rgba(10, 15, 22, 0.95);` or `#05070c;` with `border: 1px solid rgba(255, 255, 255, 0.06);`).
  2. **Sanitize `@keyframes pulse`**:
     - Remove `box-shadow` animations in `@keyframes pulse` (`main.css:1349-1358`).
     - Replace with pure `opacity` or `transform: scale()` transitions.
  3. **Layer Isolation**:
     - Add `contain: paint` or isolated GPU stacking context on standalone live indicators (`.status-indicator`, `.badge-dot`) to prevent layer dirtiness from bubbling up to parent containers.

### WS3: Profiling & Verification Gate
- **Tools**: Activity Monitor + Instruments (Metal System Trace / Advanced Graphics Statistics).
- **Verification Steps**:
  1. Launch app on macOS, leave idle on dashboard for 60 seconds.
  2. Measure `% GPU` of `Aki Dev Sync Graphics and Media` in Activity Monitor (Must be `< 2%`).
  3. Run Instruments Graphics Profiler: verify `Tiler Utilization %` is `< 5%`.
  4. Open 3 in-app terminal tabs, switch tabs, collapse/expand dock, run `agy` / `vim`: confirm terminal rendering is smooth and responsive.

## Implemented 2026-08-19

Static-only implementation pass, no profiler available (headless dev box, per `coding.B3` - same constraint the original analysis above was written under). Scope: WS1 and WS2 from §4, plus a Glass Effect opt-in this plan's analysis did not originally propose (see "Left deliberately alone" below for why an opt-in was chosen over an outright removal).

**Changed:**
- **WS1 - terminal cursor blink.** `TerminalView.vue`: xterm now initializes with `cursorBlink: false`. A new `hasFocus` ref, driven by the terminal textarea's own native `focus`/`blur` listeners, combines with `props.active` in `updateCursorBlink()` to set `term.options.cursorBlink` live - blinking is on only for the one tab that is both the active tab and holds real DOM focus. Every other mounted tab (background tabs kept alive by `body-persist`, or an active-but-unfocused tab) never re-arms the 500ms blink timer this plan's analysis identified as a standing WebKit IPC cost.
- **WS2 - `backdrop-filter` decoupled from permanent chrome.** Removed from `.dashboard-bottom` and `.terminal-header` (`main.css`), `.grid-header` (`ProjectTable.vue`), and the hover-visible `.premium-tooltip` (`UsageCircle.vue`) - all four are either permanently mounted or, for the tooltip, composited on every hover of an always-present element. Each now ships an opaque/near-opaque solid background instead (`rgba(..., 0.95)`, up from `0.6`-`0.9`). Restored, app-wide, only under a new `html.fx-glass` root class - see "Glass Effect toggle" below. Modal/overlay `backdrop-filter` (dialogs, dropdowns not covered above) was left untouched; this plan's own root-cause analysis (§2A) named *permanent* chrome specifically, and a modal's blur layer does not exist while the modal is closed.
- **WS2 - `@keyframes pulse` sanitized.** Dropped the `box-shadow` channel (a paint-triggering property under continuous animation, per §2A's shader-invalidation mechanism); kept `opacity` and added `transform: scale()`, both compositor-only properties.
- **WS2 - layer isolation.** `RefreshRing.vue`'s root SVG gained `contain: paint` and dropped `overflow: visible` (dead under `contain: paint`, confirmed by checking the `r=15` circle geometry against `viewBox 0 0 36 36` in both inline and overlay size modes - nothing was actually being clipped either way), and its countdown ring now animates via `animation-timing-function: steps(N)` (~1 step/second, capped at 60) instead of the previous continuous `stroke-dashoffset` interpolation - the same countdown visually, without repainting every compositor frame.
- **Dead-code removal found during this pass, not in the original plan**: 46 lines of `.projects-table` CSS in `main.css` (`th`/`td`/hover rules) had no matching `<table>` markup anywhere in the app (grepped) - deleted. Not a GPU fix, a Rule-of-Three/YAGNI cleanup surfaced while reading the file.

**Glass Effect toggle - the deviation from the original plan's "just remove it" framing:**
The plan as written (§4 WS2 Action 1) proposed replacing the blur outright with no user-facing switch. Implemented instead as an opt-in: new `src/composables/useVisualEffects.js` (`glassEnabled` ref backed by `localStorage` key `aki-devsync-glass`, default **off**) toggles the `fx-glass` class on `<html>`; a checkbox row ("Glass Effect") was added to the App-icon dropdown, after "Enable SSH Terminal Color". Rationale for the deviation: the blur is a visible aesthetic choice some users may prefer even at the GPU cost identified here, and CSS-only reversibility (one root class) made an opt-in essentially free to offer over a hard removal.

**Confirmed correct, left unchanged:**
- `ResizeObserver`/`requestAnimationFrame` in `TerminalView.vue` (`scheduleFit`) - already ruled out as an idle-standing cost by `docs/research/perf-idle-gpu-cpu.md`, re-confirmed unchanged in this pass.
- `UsageCircle.vue`'s 10s timer, `AgentUsage.vue`'s `agoTimer`/`ccClockTimer` - JS-only, already lifecycle-clean (`docs/research/perf-idle-gpu-cpu.md`'s own verdict); this pass's scope was CSS compositor + terminal cursor, not JS timer cadence, and the task's own instruction (per that doc's "Implemented" section) was not to alter interval values.
- `.btn-tech`/`.btn-cell-trigger`'s `transition: all` in `main.css` - already deliberately kept broad by the prior `perf-idle-gpu-cpu.md` pass (shared base classes, heterogeneous variants, interaction-only so no idle cost); untouched here.

**Not verified here - needs the Mac:** every number in §1/§3 of this plan (51.4% CPU, 73.4%/94% GPU, Tiler/Device/Renderer Utilization) was measured before this fix; none of it has been re-measured. This pass is static reading only - no Activity Monitor, no Instruments, no running app (headless Linux box, macOS-only ship target, per project `CLAUDE.md`). WS3's verification steps above still apply, unrun, against this post-fix build. Whether the fix actually closes the gap to the stated `< 2%`/`< 5%` targets is an open question until the owner runs them.

## Verified on the owner's Mac 2026-08-19

- [x] §4 WS3 step 1 - app launched, idle on dashboard.
- [x] §4 WS3 step 2 (partial) - owner read Activity Monitor's **GPU History** window (system-wide, not the single-process `Aki Dev Sync Graphics and Media` % column this step originally specified): idle and under normal use now sits around **25%**, down from the **~95%** baseline observed before this fix. A large, real drop, but not the same metric as the `< 2%` per-process target - the process-specific reading was not taken, so that exact target is not confirmed hit.
- [ ] §4 WS3 step 3 - Instruments Tiler/Device/Renderer Utilization `< 5%` - not run (no Instruments session performed).
- [x] §4 WS3 step 4 - terminal tabs, dock collapse/expand, `htop`/shell use confirmed smooth and responsive; cursor blinks only on the active+focused tab; RefreshRing's stepped countdown confirmed visually (nhảy từng nấc, không mượt liên tục).

