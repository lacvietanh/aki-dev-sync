# Business backbone — Aki Dev Sync

> updated 2026-09-19 · v1.30.0

Canonical source of truth for who Aki Dev Sync is for, why someone picks it, and how it sustains itself. Per `RULE-docs.md` A3 and `RULE-biz.md` A3, README, feature, and plan docs defer to this file; a market-facing claim that contradicts it is reconciled here or escalated, never silently overridden.

## Primary audience and job

A solo developer who edits and commits on a light local Mac (the git source of truth) and runs an AI coding agent (Claude Code or Antigravity) on a stronger remote box over SSH, and needs to keep both sides in sync and watch the agent's real quota without polluting git history.

One audience, deliberately narrow: the maintainer and developers who share this exact split local-to-remote workflow (README: "built for a specific way of working", "the Lạc Việt Anh Workflow"). Someone who does not split work across a local Mac and a remote AI box is a non-target, and every tradeoff resolves for the split-workflow user.

## USP and moat

Falsifiable functional differentiators, each checkable against the code and the README "Under the Hood" section:

- Shows the exact `rate_limits` Anthropic's own CLI already computed, read from the statusLine hook cache, instead of estimating usage from token counts. Antigravity quota is read from the IDE's local Language Server over Connect RPC, not from the cloud endpoints that return dead 0% data.
- Syncs the split environment over native rsync and ssh, carrying full `.git/` history to the remote, without creating throwaway commits just to move files.

No commercial moat. Those mechanisms are reimplementable, the app is macOS-only and single-maintainer, and distribution is a single public GitHub release (`.dmg`). The differentiation is functional, not defensible: there is no lock-in, network effect, or proprietary data advantage, and none is claimed (`RULE-biz.md` A2, honest "no moat" over a decorative one).

## Revenue path

None as a commercial product. Aki Dev Sync is a personal workflow tool released publicly; it sustains itself through the maintainer's own use plus optional donations (in-app Donate: PayPal, MoMo QR, VietQR bank transfer). There are no paid tiers, no subscription, no backend account, and no plan to add them. This explicit "none: personal tool, donation-supported" is the stable answer (`RULE-biz.md` B4), not a placeholder pending monetization.

## Product boundaries

Constraints that resolve tradeoffs; a proposed change that crosses one is reconsidered against this doc first:

- macOS-only. One bundle ships, a `.dmg`. A cross-platform GUI is a non-goal until a build for another platform actually ships.
- Local is the source of truth; the remote is a disposable AI workspace. Sync never treats the remote as authoritative for git history.
- Real quota only. The app never runs the agent CLI itself to force a reading (a headless probe was deleted after leaking orphaned sessions); it reads the passive cache.
- Native rsync and ssh engine, no hosted service. No cloud backend, no account, no service to operate.
- Extreme-narrow UI. No new row, banner, or label when color, outline, badge, or tooltip can carry the state (CLAUDE.md UI principle).
- Single-maintainer friendly (`RULE-coding.md` A2). Scope stays at what one person can maintain, without cutting UX.
