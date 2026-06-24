# GEMINI.md  -  aki-sync-gui Project Rules for Gemini CLI / Antigravity

> **CRITICAL**: This file is a bootstrap loader only.
> The actual source of truth is **`CLAUDE.md`** in this directory.
> You MUST read `CLAUDE.md` before doing any work in this project.

> [!CAUTION]
> **NO SILENT DEVIATIONS**: Stick strictly to instructions. Ask and confirm before open built-in browser, changing target files, code, or logic
> **WHEN QUESTIONS ARE ASKED**: Answer only. Do not modify code, add comments, or write logic stubs without explicit confirmation.

## Mandatory first step

Read `CLAUDE.md` at project root. It contains:
- Pointer to shared Aki rules at `~/.aki/claudedoc/`
- All project-specific constraints (stack, config, design system, SEO, docs)

## Aki-RULE shared files

Global rules live at `~/.aki/claudedoc/`. CLAUDE.md contains project-specific constraints and points to shared Aki rules at `~/.aki/claudedoc/`.

## Why this file exists

Gemini CLI and Antigravity auto-load `GEMINI.md` from the project root.
This file ensures those agents are directed to `CLAUDE.md` as the single source of truth,
instead of operating without project context.
