# Terminal mobile keyboard viewport — design (not yet implemented)

Read this before touching layout CSS for the dock/terminal: `docs/arch/terminal-stack.md`, `docs/feat/in-app-terminal.md`. This doc answers one pinned request: on a phone opening the companion page, tapping the compose input (or the terminal) and having the on-screen keyboard appear breaks the layout — described by the user as the keyboard "replacing" the terminal view. It is design-only; nothing here has been implemented.

## 0. Scope

Companion only. The host is a macOS Tauri window (WKWebView-backed, per `RULE-stack-tauri.md`) with a physical keyboard and no on-screen keyboard concept — every finding below is about a phone (iOS Safari, an iOS home-screen standalone launch, or Android Chrome) hitting the relay's web page, i.e. `index.html` rendered in a mobile browser engine, not about the Mac window.

## 1. Findings, severity-weighted (`METHOD-ux-psych.md` §C1)

All of §1 is **verified from code** (`coding.A3`: source over memory) — file, line, and the exact rule quoted, not inference.

### SEVERE — F1. The whole app shell's height is a static `100vh`, with nothing to shrink it

`src/assets/main.css:124`:
```css
.dashboard-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
}
```
`App.vue`'s root element is `.dashboard-layout`. `body` (`main.css:41-53`) additionally sets `overflow: hidden`. On iOS Safari, the CSS `vh` unit is computed against the **layout viewport**, which iOS does **not** shrink when the on-screen keyboard appears — only the **visual viewport** shrinks (verified via research, §2 below). Combined with `overflow: hidden` on `body`, there is no scroll container anywhere above the terminal that could compensate: the 100vh box stays exactly the height it was before the keyboard existed, and the real keyboard then physically covers whatever pixels of that box sit at the bottom of the actual screen.

### SEVERE — F2. The dock's own height is *also* expressed only in `vh`, at every layer

- `main.css:139`, pre-hydration fallback: `.dashboard-bottom { height: 40vh; }`
- `src/composables/useDockLayout.js:79-81`, the live value once mounted:
  ```js
  export const dockHeightCss = computed(() =>
    dockMaximized.value ? MAXIMIZED_CSS : `${dockHeightPct.value}vh`
  )
  ```
- `useDockLayout.js:28`: `const MAXIMIZED_CSS = 'calc(100vh - var(--titlebar-h))'`

`.pty-terminal` (`TerminalView.vue:393-401`) is `flex:1; height:100%`, so it simply inherits whatever height `.dashboard-bottom` resolves to. Every one of these three expressions reads the same unshrinking `vh` unit F1 describes — there is no second bug here, it is the same root cause repeated at every layer of the box model the terminal sits inside.

### SEVERE — F3. The compose input (the thing the user actually taps) is the bottom-most row of that entire chain

`TerminalView.vue`'s template: `.pty-terminal` → `.pty-terminal-mount` (xterm) → `.pty-key-row` (companion only) → `.pty-compose-row` (the plain `<input>`, lines 78-94). `.pty-compose-row` and `.pty-key-row` are both `flex-shrink: 0` (`main.css`... actually declared inline in `TerminalView.vue`'s `<style>`, lines 429-440 and 480-488) — fixed-height rows pinned to the bottom of a column whose total height (F1, F2) never adjusts for the keyboard. The element the user taps to bring up the keyboard is architecturally guaranteed to be the first thing the keyboard covers. This is the mechanism behind the user's "keyboard replaces the terminal view" description: nothing shrinks, so the keyboard doesn't push the compose row up — it slides over it.

### MATERIAL — F4. `index.html`'s viewport meta does not opt into any keyboard-resize behavior

`index.html:16`:
```html
<meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover" />
```
No `interactive-widget` value is set. Per spec (§2), the unset default is `resizes-visual` — the layout viewport (what `vh` measures) is left untouched even on the browsers that *do* support opting out of that default (Chrome 108+, Firefox 132+; not Safari — see §2). This is a second, independent reason `vh` never reflects the keyboard, on top of F1/F2's more fundamental iOS behavior.

### MATERIAL — F5. Nothing in the codebase has ever addressed this

```
grep -rn "visualViewport\|dvh\|svh\|lvh\|keyboard-inset\|interactive-widget" src/ index.html
```
returns zero matches. This is a verified *absence*, not a guess: no prior attempt, patch, or workaround for virtual-keyboard viewport handling exists anywhere in the tree. There is nothing to "fix a hack" — the ask is to introduce the first correct handling, not repair a patch.

## 2. Research grounding — verified facts vs assumptions

Per `agent-behavior.B2`: what follows is separated into confirmed facts (with sources) and explicit assumptions.

**Verified facts**, from current (2024-2025) sources:

- **Layout viewport vs. visual viewport, iOS Safari.** "On Safari iOS, as the keyboard is shown, the Layout Viewport remains the same size but the Visual Viewport shrinks." `vh`/`svh`/`lvh` are computed against the layout viewport; only `window.visualViewport.height` reflects the keyboard. ([Bram.us — VirtualKeyboard API](https://www.bram.us/2021/09/13/prevent-items-from-being-hidden-underneath-the-virtual-keyboard-by-means-of-the-virtualkeyboard-api/), [iifx.dev — Fixing iOS Safari's Shifting UI with dvh](https://iifx.dev/en/articles/460170745/fixing-ios-safari-s-shifting-ui-with-dvh))
- **`dvh` does not solve the iOS keyboard case.** The dynamic-viewport units (`dvh`/`svh`/`lvh`) exist to handle the *collapsing toolbar*, not the keyboard: on iOS specifically, "the on-screen keyboard [is shown] over the bottom of the viewport… meaning a fixed element positioned with dvh units would be hidden by the on-screen keyboard on iOS." `dvh` is real, current, and worth adding as a static pre-JS approximation, but it is not sufficient by itself for this bug. (Same iifx.dev source; corroborated across the "Fix mobile keyboard overlap with dvh" write-ups found in search.)
- **`interactive-widget=resizes-content` (viewport meta).** Three values exist: `resizes-visual` (default — shrinks only the visual viewport), `resizes-content` (shrinks the layout viewport too, which is what would actually fix `vh`-based layouts), `overlays-content` (keyboard floats over content, measurements untouched). Supported: **Chrome/Android 108+, Firefox 132+. Not implemented in Safari/WebKit** as of the most recent dated source found (May 2025) — flagged explicitly as an open WebKit standards-position ask, not a done deal. ([HTMHell — Control the Viewport Resize Behavior](https://www.htmhell.dev/adventcalendar/2024/4/), [w3c/csswg-drafts #10464](https://github.com/w3c/csswg-drafts/issues/10464))
- **`env(keyboard-inset-height)` / the VirtualKeyboard API (`navigator.virtualKeyboard`).** Chromium 94+ only. "Neither Firefox nor Safari have signaled interest" — not usable as anything but a Chromium-only enhancement. ([Bram.us](https://www.bram.us/2021/09/13/prevent-items-from-being-hidden-underneath-the-virtual-keyboard-by-means-of-the-virtualkeyboard-api/))
- **The `visualViewport` resize/scroll listener pattern is the one technique that works across the whole matrix** (old and new iOS Safari, Android Chrome, and — because it is a WebKit-level API, not a Safari-specific one — the WKWebView-class engines used by iOS home-screen standalone launches): listen for `window.visualViewport`'s `resize` and `scroll` events and drive layout from the live `height`/`offsetTop` values, because it is the only value every current mobile engine actually updates when the keyboard opens. This is the documented cross-product fallback pattern in every source found (Bram.us, the Medium "Safari Mobile Resizing Bug" writeup, the Franciscomoretti "fix mobile keyboard overlap" post, the HTMHell article's own polyfill note).
- **A documented iOS bug on top of all this:** "When the keyboard is dismissed, `visualViewport.offsetTop` does not reset to 0," which can leave a fixed header/footer misaligned after the keyboard closes if a naive implementation only reacts to `resize`, not `scroll`/`offsetTop`. Worth designing around now rather than discovering it on-device later.

**Assumptions (not verified — mark before implementation)**:

- Whether the specific WebKit build behind an iOS "Add to Home Screen" **standalone** launch (which `index.html`'s `apple-mobile-web-app-capable` meta already enables) resizes `visualViewport` identically to a normal Safari tab. The sources found describe Safari tabs and native-app-embedded `WKWebView` (e.g. a Tauri/Capacitor shell); standalone home-screen web apps are a third configuration not directly covered by anything fetched here. Treat as **unverified — needs a runtime check on a real iPhone**, both as a Safari tab and as an added-to-home-screen launch.
- Whether the current iOS version(s) in the user's actual device pool have since shipped `interactive-widget` support (WebKit shows as an open ask in the most recent dated source, May 2025; this is 2026 and could have shipped since). Treat the "Safari doesn't support it" fact as **needs reconfirmation at implementation time**, not as permanently true — cheap to add regardless since unsupported values are spec'd to be ignored, not to error.
- Android Chrome's default keyboard-resize behavior (`resizes-visual` per spec) in *this app's* actual companion flow has not been observed on a real device in this investigation — only documented as the cross-browser default.

## 3. Recommended fix — smallest correct shape, precise enough to implement without further research

Do **not** rely on `dvh`/`interactive-widget`/`env(keyboard-inset-height)` alone (§2: each is either iOS-blind, Safari-unsupported, or Chromium-only). The one technique that actually covers this app's real matrix (iOS Safari tab, iOS standalone, Android Chrome) is the **`visualViewport` resize-listener pattern**, feeding a CSS custom property that the existing `vh`-based layout math is rewritten to consume. `dvh` and `interactive-widget=resizes-content` are added alongside it as cheap, standards-track defense-in-depth — never as the primary mechanism.

### 3.1 New composable: `src/composables/useVisualViewportHeight.js`

Single responsibility (`design.A3`, the "and" test: it does one thing — publish the current visible height in px): on mount, if `window.visualViewport` exists, listen to its `resize` **and** `scroll` events (the offsetTop-reset bug in §2 is exactly why `scroll` must also be watched, not just `resize`) and write `document.documentElement.style.setProperty('--vvh', `${px}px`)`. Feature-detect and no-op to `window.innerHeight` if `visualViewport` is undefined (old WebKit / desktop browsers without it) — the CSS fallback chain in §3.2 covers the rest.

This needs **no host/companion branch and no `isHost` import** — on the Mac, `visualViewport.height` simply always equals the window's real height (no on-screen keyboard ever changes it), so the same code is inert there. This matches the project's existing capability-pattern spirit (`usePtyTerminal.js`'s `ownsPtySize`/`showKeyRow`): a screen doesn't need to be *told* it's the host to behave correctly, the platform API itself already degrades to a no-op.

Mount it once, near the app root — `App.vue`'s existing `onMounted` (alongside `initRemote()`), or `main.js` next to `boot/roleStamp.js`'s pattern of "one small side-effecting module imported once at boot." Either is fine; do not mount it per-component (there is exactly one visual viewport per page).

### 3.2 `main.css` — consume the var, keep the pre-JS fallback

```css
.dashboard-layout {
  height: 100vh;              /* pre-JS / no-visualViewport fallback, unchanged */
  height: 100dvh;             /* static improvement: correct once the toolbar has settled */
  height: calc(var(--vvh, 100dvh)); /* live, keyboard-aware once the composable has run */
}
```
```css
.dashboard-bottom {
  height: 40vh;
  height: calc(var(--vvh, 100vh) * 0.4);  /* fallback height, keyboard-aware */
}
```
Three declarations of the same property are intentional progressive enhancement (not duplication in the `design.A1` sense — each is a fallback for a browser that can't do the next one), same pattern CSS authors use for `gap`/`color-mix`/etc. fallbacks elsewhere in the ecosystem; a linter that flags "duplicate property" here would be wrong to block it.

### 3.3 `src/composables/useDockLayout.js` — the two other `vh` math sites

- Line 79-81 (`dockHeightCss`): change the template literal from `` `${dockHeightPct.value}vh` `` to `` `calc(var(--vvh, 100vh) * ${dockHeightPct.value} / 100)` ``.
- Line 28 (`MAXIMIZED_CSS`): change `'calc(100vh - var(--titlebar-h))'` to `'calc(var(--vvh, 100vh) - var(--titlebar-h))'`.

Both are load-bearing, not cosmetic: F2 showed `dockHeightCss` and `MAXIMIZED_CSS` are two more places the same static-`vh` root cause repeats. Fixing only `.dashboard-layout` (§3.2) and leaving these untouched would still let `.dashboard-bottom` claim a `vh`-percentage of the *pre-keyboard* screen height inside a now-correctly-shrunk parent, overflowing it — `body`'s `overflow: hidden` would then clip whatever doesn't fit, most likely from the top of `.dashboard-top` (a *different* visible symptom, not a fix).

### 3.4 `index.html` — cheap defense-in-depth, not the fix itself

Add `interactive-widget=resizes-content` to the existing viewport meta:
```html
<meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover, interactive-widget=resizes-content" />
```
Per spec, an unrecognized value is ignored rather than erroring, so this is safe to ship even where unsupported (today's Safari). It buys the Android Chrome / Firefox slice of the matrix a second, browser-native mechanism working in parallel with §3.1-3.3 — belt-and-braces, not the load-bearing fix (that's §3.1-3.3, which must work even where this meta value is ignored).

### 3.5 What this fix does **not** touch

- No new DOM element, banner, or row (`CLAUDE.md`'s Extreme Narrow rule) — this is a CSS custom property plus a JS side-effect module, zero new visible chrome.
- No `isHost`/role branching (ENV-1, the capability pattern) — same code path on host and companion.
- `TerminalView.vue`'s template and its `doFit()`/`scheduleFit()` fit logic are untouched. The keyboard-driven resize is a **container** resize exactly like a window drag or a dock-splitter drag — the existing `ResizeObserver` on `.pty-terminal-mount` (`TerminalView.vue:357-358`) already re-fits xterm whenever its container's measured size changes, and §3.2's CSS change is precisely what makes that container's measured size change correctly when the keyboard opens. No new fit path is needed; the existing one starts receiving correct numbers.

## 4. Device/browser split, and what needs a real device

| Surface | Keyboard resize behavior | Fix path | Status |
|---|---|---|---|
| Mac host window (WKWebView, Tauri) | No on-screen keyboard exists | N/A | Out of scope (§0) |
| iOS Safari tab (companion) | Layout viewport static, visual viewport shrinks (§2, cited) | `visualViewport` listener (§3.1-3.3) | Static reasoning solid; **runtime check required** — this is a layout/viewport class of risk that `coding.B3` says cannot be settled by reading code alone |
| iOS "Add to Home Screen" standalone (companion) | Assumed same WebKit viewport model as a Safari tab | Same fix | **Unverified assumption** (§2) — needs its own on-device pass, do not assume the Safari-tab verification covers it |
| Android Chrome tab (companion) | Spec default `resizes-visual`; opts into `resizes-content` via §3.4's meta, `visualViewport` listener covers it regardless | Both §3.1-3.3 and §3.4 apply | Static reasoning solid; **runtime check required** for the same reason as iOS |
| Android Chrome "Add to Home Screen" (companion) | Not researched in this pass | Same fix, unverified whether it needs separate testing | Out of scope for this doc — flag if it comes up |

**What is verified without touching a phone:** every finding in §1 (all read directly from this repo's own code), and every fact in §2's "Verified facts" list (each has a cited, dated source). **What only a real device can settle**, per `coding.B3` (a viewport/keyboard interaction is exactly the runtime-only risk class that rule names): that `visualViewport.resize`/`scroll` actually fire in this app's real WKWebView/Safari/Chrome combination, at the frequency and timing needed for the terminal not to visibly jump; that the compose input stays fully visible and reachable through a full type→keyboard-open→send→keyboard-close cycle on each of the four rows in the table above; and that the offsetTop-reset bug (§2) does not leave the dock's header misaligned after the keyboard closes.

## 5. Acceptance criteria

1. On an iPhone (real device, Safari tab): open the companion, tap the compose input. The on-screen keyboard opens; the compose input and send button remain fully visible immediately above the keyboard, not hidden behind it.
2. Same device, same test, as an "Add to Home Screen" standalone launch.
3. Same test on an Android phone, Chrome tab.
4. Closing the keyboard (tapping elsewhere, or a system back-gesture) restores the dock to its exact pre-keyboard height and the header/tab strip stay aligned — no residual offset from the `visualViewport.offsetTop` bug named in §2.
5. The Mac host window's dock height, drag-resize, and MAXIMIZE behavior are pixel-identical to before this change (`--vvh` must be a no-op there) — verify by diffing `dockHeightPct`/`MAXIMIZED_CSS` rendered height before/after on the Mac.
6. `⌘+`/`⌘-`/`⌘0` font zoom and the terminal's own `ResizeObserver`-driven `doFit()` are unaffected on both surfaces — no double-fit, no oscillation, during a keyboard open/close cycle (watch for the same `ResizeObserver` loop class of bug `scheduleFit()`'s rAF-coalescing comment already guards against).
7. No new DOM row/banner is visible anywhere (Extreme Narrow) and `grep -n "isHost" src/composables/useVisualViewportHeight.js` returns nothing (ENV-1 compliance, capability-pattern-consistent by never needing a role check at all).

## 6. Not in this pass

- Android Chrome "Add to Home Screen" / TWA behavior — flagged in §4 as unresearched, pick up only if it surfaces as a real complaint.
- Any visual affordance indicating "keyboard is open" (a banner, a resize animation beyond the CSS's existing height transition) — Extreme Narrow forbids new chrome, and nothing in the findings calls for it: the fix is that the existing layout becomes correct, not that a new state needs to be communicated.
- Polyfilling `interactive-widget` for pre-108 Chrome or any WebKit version — the `visualViewport` path (§3.1) already covers those; the meta tag (§3.4) is additive only.
