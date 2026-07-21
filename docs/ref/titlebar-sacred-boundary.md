# Titlebar Sacred Boundary

## Context

This app runs with `"decorations": false` and `"transparent": true` in `tauri.conf.json`.
That removes the native OS titlebar entirely. A custom `<header>` element with `data-tauri-drag-region`
takes its place at the top of the window.

The header has a fixed height of **42px** and is always the topmost element in the visual hierarchy.

## The Rule

**The titlebar zone (top 42px) is a sacred boundary. No UI element may overlap, cover, or intrude into it.**

This is not a soft guideline. Violating it breaks drag-to-move, hides critical controls, and creates
a confusing UX where users cannot drag the window or access minimize/close.

## What "intrude" means

| Element type | Violation example | Correct behavior |
|---|---|---|
| Modal / dialog | `top: 0` or `position: fixed; inset: 0` | Start at `top: 42px` or use `margin-top: 42px` |
| Full-screen overlay | `position: fixed; top: 0; left: 0; right: 0; bottom: 0` | `top: 42px` - never cover the header |
| Dropdown / tooltip | Positioned anchor is near the top-left | Ensure calculated position never goes above y=42 |
| Sidebar / drawer | Slides in from top-left without offset | Start below the header; use `top: 42px; height: calc(100% - 42px)` |
| Notification / toast | Positioned at `top-right` with `top: 0` | Use `top: 50px` or `bottom-end` position |
| Scroll container | Content scrolls under the header | Add `padding-top: 42px` or use `margin-top: 42px` on the scroll root |

## Why SweetAlert2 is safe

The app uses SweetAlert2 (Swal) for dialogs. SweetAlert2 centers modals in the viewport by default.
On a typical window height (≥400px), the modal content always starts well below 42px. **No override needed.**

If you add a `customClass` or position override to any Swal call, verify it does not push the backdrop
or the box above y=42.

## CSS enforcement pattern

For any new overlay or fixed-position UI:

```css
/* Required for any fixed/absolute element that could reach the top */
.my-overlay {
  position: fixed;
  top: 42px;           /* respect the titlebar */
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 100;        /* below titlebar z-index (see below) */
}
```

The titlebar's effective z-index must always beat overlays:

```css
.dashboard-top-header {
  position: relative;
  z-index: 200;        /* above all content, below OS chrome */
}
```

## Z-index ladder

| Layer | z-index | Notes |
|---|---|---|
| Page content | 1-9 | Tables, cards, static elements |
| Floating UI (tooltips, dropdowns) | 10-99 | Must not extend above y=42 |
| Modals / overlays | 100-199 | Must start at top: 42px or higher |
| Titlebar | 200 | Hard ceiling for app UI |

## Checklist for new UI that uses `position: fixed` or `position: absolute`

- [ ] Does the element's topmost edge respect y ≥ 42px at all window sizes?
- [ ] If the element uses `inset: 0` or `top: 0`, has it been overridden to `top: 42px`?
- [ ] Does the element's z-index stay below 200?
- [ ] If it is a backdrop/scrim, does it start at `top: 42px` (not `top: 0`)?
- [ ] If it is a toast/notification, is it anchored to `bottom-end` or offset from top by ≥ 50px?
