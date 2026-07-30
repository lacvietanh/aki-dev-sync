# Reference: CLI Multi-Account Configuration Directory Switching (`CLAUDE_CONFIG_DIR` & `GEMINI_DIR`)

**Date:** 2026-07-30  
**Topic:** Custom Configuration Directory Aliases for Claude Code (`claude`) and Google Antigravity (`agy`)  
**Location:** `docs/ref/multiple-account-config-dir.md`  

---

## 1. Overview

When managing multiple accounts or isolated profiles for AI coding CLIs (Claude Code and Antigravity / Gemini CLI), each CLI supports environment variables to override the default configuration directory path. This enables distinct credentials, history, and settings per account or environment without file swapping.

---

## 2. CLI Environment Variables Comparison

| CLI Agent | Default Config Path | Environment Variable | Example Usage |
| :--- | :--- | :--- | :--- |
| **Claude Code** (`claude`) | `~/.claude` | `CLAUDE_CONFIG_DIR` | `CLAUDE_CONFIG_DIR=~/.claude-2 claude` |
| **Google Antigravity** (`agy`) | `~/.gemini` | `GEMINI_DIR` | `GEMINI_DIR=~/.gemini-2 agy` |

---

## 3. Shell Aliases Configuration (`~/.zshrc`)

Add the following aliases to your shell configuration (`~/.zshrc` or `~/.bashrc`) to quickly launch CLI instances for specific account profiles:

```zsh
# Claude Code Multi-Account Profiles
alias cc1="CLAUDE_CONFIG_DIR=~/.claude claude"
alias cc2="CLAUDE_CONFIG_DIR=~/.claude-2 claude"

# Antigravity (AGY) Multi-Account Profiles
alias ag1="GEMINI_DIR=~/.gemini agy"
alias ag2="GEMINI_DIR=~/.gemini-2 agy"
```

---

## 4. Key Isolation Characteristics

1. **Config & Token Isolation**: Overriding the config directory creates an independent storage tree containing authentication tokens, history logs, plugins, and custom settings.
2. **Concurrent Sessions**: Terminal windows using different config directories can run side-by-side without credential collisions or token refresh overwrites.
3. **In-RAM Persistence**: Processes load environment settings at invocation; modifying disk configuration while a process is running does not alter the active in-RAM session state.
