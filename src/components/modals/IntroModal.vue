<template>
  <BaseModal :show="showIntroModal" @close="closeIntroModal">
    <template #title>
      <i class="fa-solid fa-book-open mr-1 icon-guide"></i> User Guide - Aki Dev Sync
    </template>

    <div class="modal-body scrollable intro-content u-select-text">
      <div class="intro-header mb-3">
        <h3>🚀 Aki Dev Sync</h3>
        <p>
          Command center for syncing code between a <strong>Local</strong> and a <strong>Remote</strong> machine over SSH/rsync - no UI freeze, no junk commits polluting your Git history.
        </p>
      </div>

      <!-- Mental Model: Local <-> Remote -->
      <div class="model-flow mb-3">
        <div class="model-node local">
          <div class="model-role">LOCAL</div>
          <div class="model-title">Source of Truth</div>
          <div class="model-meta">Git · Antigravity</div>
        </div>
        <div class="model-arrows">
          <span class="arrow-push"><i class="fa-solid fa-arrow-right"></i> PUSH</span>
          <span class="arrow-pull">PULL <i class="fa-solid fa-arrow-left"></i></span>
        </div>
        <div class="model-node remote">
          <div class="model-role">REMOTE</div>
          <div class="model-title">AI Workspace</div>
          <div class="model-meta">Claude Code · Heavy jobs</div>
        </div>
      </div>

      <!-- Use Cases -->
      <div class="alert-box info mb-3">
        <h4 class="alert-title"><i class="fa-solid fa-earth-americas"></i> Who is this for?</h4>
        <ul class="custom-list">
          <li><strong>Weak machine ↔ powerful server:</strong> Keep the laptop light, push heavy builds / AI work to the server.</li>
          <li><strong>Source protection:</strong> Keep the core code on your own Remote, separate from the work machine.</li>
          <li><strong>Multi-device sync:</strong> Sync fast across PC, laptop and server without draft commits on GitHub.</li>
          <li><strong>AI Workspace:</strong> Push the whole project (including <code>.git/</code>) so the AI has full context.</li>
        </ul>
      </div>

      <!-- SYNC features -->
      <div class="subgroup-label mb-1">⚡ SYNC</div>
      <div class="features-grid mb-3">
        <div class="feature-card">
          <div class="feature-icon icon-push"><i class="fa-solid fa-arrow-up"></i></div>
          <div class="feature-text">
            <strong>PUSH</strong>
            <span>Push Local → Remote with everything not listed in <code>push_excludes</code>, plus a per-project "Force Delete" setting. A folder listed only in <code>pull_excludes</code> is <strong>push-only</strong> - sent up, never pulled back, never counted as a change (<code>.git/</code> by default: the AI gets full history, and the PUSH badge stays dark through git's own housekeeping). While a sync is running that project's button becomes <strong>STOP</strong> - it kills rsync and its ssh, and quitting the app kills them too.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-select"><i class="fa-solid fa-bolt"></i></div>
          <div class="feature-text">
            <strong>SELECT (Push Special)</strong>
            <span>Opens the native macOS file picker (multi-select). If a file already exists on Remote, shows a Local / Remote mtime comparison before you confirm the overwrite.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-pull"><i class="fa-solid fa-arrow-down"></i></div>
          <div class="feature-text">
            <strong>PULL</strong>
            <span>Pull back what was written on Remote so you can review and commit it on Local.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-dryrun"><i class="fa-solid fa-shield-halved"></i></div>
          <div class="feature-text">
            <strong>DRY RUN</strong>
            <span>Preview exactly which files would change - nothing is written until you turn this off.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-syncstatus"><i class="fa-solid fa-circle-dot"></i></div>
          <div class="feature-text">
            <strong>Sync Status</strong>
            <span>Checked automatically - the PUSH/PULL buttons light up when the two sides drift apart.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-hooks"><i class="fa-solid fa-code"></i></div>
          <div class="feature-text">
            <strong>Pre / Post Hooks</strong>
            <span>Scripts run before/after each sync (build, restart a service, notify...), on Local or Remote as you choose.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-delete"><i class="fa-solid fa-clone"></i></div>
          <div class="feature-text">
            <strong>Mirror / Delete</strong>
            <span>Turn on <code>--delete</code> for an exact mirror. Push never deletes by default; once enabled, pushing over newer Remote changes asks for confirmation first. Whichever direction is armed shows a small red trash glyph on a faint chip in the bottom corner of its button (bottom-left for PUSH, bottom-right for PULL) - hidden while that button reads STOP, so a running sync never looks like the deletion is what's happening.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-presets"><i class="fa-solid fa-layer-group"></i></div>
          <div class="feature-text">
            <strong>Exclude Presets</strong>
            <span>Separate rsync excludes for Push and Pull, with 1-click presets: Nuxt 4, Tauri v2, Aki Default.</span>
          </div>
        </div>
      </div>

      <!-- TOOLS & MONITOR features -->
      <div class="subgroup-label mb-1">🛠 TOOLS & MONITOR</div>
      <div class="features-grid">
        <div class="feature-card">
          <div class="feature-icon icon-tasks"><i class="fa-solid fa-list-check"></i></div>
          <div class="feature-text">
            <strong>Project Tasks & Notes</strong>
            <span>Task management: Pin 📌, Wish 🕒 and Done (which auto-unpins). Includes a Project Notes card that grows natively with its content via CSS (`field-sizing: content`) and trims stray whitespace. Shares its task engine and list UI with Global Note below.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-openpopup"><i class="fa-solid fa-grip"></i></div>
          <div class="feature-text">
            <strong>Open Popup & Stack Launcher</strong>
            <span>Quick-open Local and Remote IDEs - hover to open on the Mac, or <strong>tap</strong> on a paired phone (tap again, Esc, or a tap outside closes it; the popup's position is per-screen, so it no longer jumps to the Mac's coordinates on a phone). Only <strong>Upload (select files)</strong> is gated by the SYNC switch - SSH Terminal, VSCode/Antigravity Remote and COPY stay usable with sync off. <strong>DEV</strong> (green) and <strong>BUILD</strong> (amber) follow the detected stack, tooltip shows the exact command, and pressing one opens/focuses that project's own tab in the <strong>in-app terminal</strong> instead of an external window - a running dev server or an already-visible build is never re-triggered by another click, an exited one is respawned and re-run automatically; a project with no detected command still shows both, dimmed, pointing at Project Settings. Lockfile scan picks the right package manager. Every Terminal window still opened externally (local, SSH, the AkiClaudeDoc installer) snaps to 124 columns in the top-right of the main display.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-globalnote"><i class="fa-solid fa-note-sticky"></i></div>
          <div class="feature-text">
            <strong>Global Note</strong>
            <span>A global notepad on the titlebar - the icon turns amber when it has content, now paired with a full task list (pin/wish/done, same as per-project tasks). Not tied to any project, auto-saved after 500ms to <code>appDataDir</code> as <code>{content, tasks}</code> - an older content-only file still loads. Header badges show pinned/open counts.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-usage"><i class="fa-solid fa-chart-bar"></i></div>
          <div class="feature-text">
            <strong>Agent Usage</strong>
            <span>Real quota: <strong>Claude Code</strong> reads Anthropic's <code>rate_limits</code> - on this Mac or on any SSH host - showing plan tier, email and org name. One bar per quota bucket the account actually has: 5-Hour and 7-Day today, plus any per-model weekly (Opus, Sonnet, …) automatically, with no update needed when Anthropic adds one. <strong>Antigravity</strong> covers all 3 surfaces: <code>AG</code> (Desktop App), <code>IDE</code> (VS Code extension) and <code>CLI</code> (terminal), with a lightweight 1-pass Smart Deduplicate (~40ms). Both agents can be watched <strong>locally or on any number of SSH hosts at once</strong>: every slot has its own host picker, so one slot watches Claude Code on host A while another watches host B under a different account. Menu ☰ → <code>Usage slots:</code> picks <strong>2, 4, 6 or 8</strong> slots, filling two per row. Two slots pointed at the same source is <em>allowed</em> - they share one instance and one poll instead of doubling it. Each monitor has its own power icon; switch one off and its last reading stays on screen marked as cached, switch it back on and it resumes. Reset countdowns tick in real time. When a pool's 7-day quota hits 100%, <em>that pool's</em> 5-hour number is dimmed and dropped from the colour ladder - a spent pool makes its 5h figure meaningless, so your eye goes to the number that still counts.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-cloud"><i class="fa-solid fa-cloud"></i></div>
          <div class="feature-text">
            <strong>Sync Check & remote monitors - independent switches</strong>
            <span>The power icon in the SYNC column toggles PUSH/PULL/SELECT, the Open popup's <strong>Upload (select files)</strong> item, and sync diff checks (background + manual) - the popup's REMOTE column itself stays available, since SSH Terminal and the remote IDE entries only reach the server, they do not sync files. In Agent Usage's REMOTE tab, the <code>AG</code> and <code>CC</code> tabs each have their own power icon, toggling the Antigravity and Claude Code monitors independently. The host picker beside them belongs to <strong>that slot alone</strong>, so one slot watches Claude Code on host A while another watches host B under a different account - two machines side by side. Switching one off never switches off another. Every monitor is on by default; each active remote monitor costs one SSH round-trip per refresh cycle, and its power icon stops it completely. If a host stops answering, that monitor gives up rather than probing a dead machine forever: its power icon turns <strong>amber</strong>, the tooltip names the host, and clicking it retries.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-ssh"><i class="fa-solid fa-key"></i></div>
          <div class="feature-text">
            <strong>SSH Config</strong>
            <span>Manage <code>~/.ssh/config</code> with undo/redo - no terminal needed. The host picked here is the <strong>default</strong> for any Agent Usage slot that has not chosen its own; to change a slot's host, use the picker in that slot's header.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-background"><i class="fa-solid fa-rotate"></i></div>
          <div class="feature-text">
            <strong>Background Refresh</strong>
            <span>Checks Git status, sync diff and agent usage on a configurable cycle. A countdown ring shows live progress right on the GIT and ACTIONS column headers.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-refresh"><i class="fa-solid fa-arrows-rotate"></i></div>
          <div class="feature-text">
            <strong>Refresh - 1 unit of work</strong>
            <span>The per-project Refresh button and the global one in the titlebar both call the same <code>refreshProject()</code> (git status + sync diff + stack detection, in parallel) - no more full app reload. Spinner state lives on each check itself, so a background cycle spins the icon at the right moment too.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon icon-git"><i class="fa-brands fa-git-alt"></i></div>
          <div class="feature-text">
            <strong>Git & Changelog Visual Preview</strong>
            <span>The Git modal renders log/status in ANSI terminal colours, handles Vietnamese accents (quotepath=false), and supports stage & commit, fetch, push, plus a Visual Changelog Preview of the project.</span>
          </div>
        </div>
        <div class="feature-card">
          <div class="feature-icon icon-update"><i class="fa-solid fa-cloud-arrow-down"></i></div>
          <div class="feature-text">
            <strong>App Update Check</strong>
            <span>Checks for updates silently on launch, or manually from the Logo menu, showing a version badge and a toast when a new release is out.</span>
          </div>
        </div>
      </div>

      <!-- Engineering Highlights -->
      <div class="alert-box tech mt-3">
        <h4 class="alert-title"><i class="fa-solid fa-flask"></i> Engineering Highlights</h4>
        <ul class="custom-list">
          <li><strong>Git terminal colours & Unicode:</strong> Git output renders in real terminal colours, and Vietnamese filenames show accents correctly instead of octal escapes.</li>
          <li><strong>Stack Detector & Lockfile Analyzer:</strong> Detects Tauri vs Node and picks the right package manager automatically - no manual config.</li>
          <li><strong>Real quota, never simulated:</strong> Quota numbers come straight from Anthropic's server via the <code>statusLine</code> hook, including a patched-in reset time when the CLI itself would hide it at 100%.</li>
          <li><strong>Antigravity quota via native RPC:</strong> Skips Google's slow/empty cloud API - reads the IDE's local process directly, ~40ms.</li>
          <li><strong>App-icon menu & window presets:</strong> The ☰ titlebar menu holds update checks, SSH Config, Statusline Customizer, Claude Code Cleanup, Remote Control, a <code>Usage slots:</code> picker (2/4/6/8), and window presets (<code>⌘1</code> Narrow+Stick Top-Left, <code>⌘2</code> Wide+Center). Tick <strong>remember</strong> to restore the exact window size/position next launch.</li>
          <li><strong>Remote Control:</strong> Control the app from a phone browser on the same LAN or over Tailscale - pair once with a 6-digit code, then it reconnects silently. Confirmation dialogs (delete, remove project) mirror to both screens and are still verified on the Mac.</li>
          <li><strong>In-app terminal & groups:</strong> A real shell (genuine PTY) runs inside the app, scoped into one tab group per project plus a global group. A paired phone gets the same real terminal (full TUI support - <code>vim</code>, <code>htop</code>, Claude Code's own UI) plus a key row for Esc/Tab/Ctrl/Shift/arrows, since a phone has no physical keyboard - and a "Fit to my screen" button that sizes the shared terminal to the phone with one tap (the Mac can always reclaim it with one tap of its own).</li>
          <li><strong>Tasks &amp; notes live in the repo, not in the app:</strong> A project's tasks and notes live in <code>&lt;project&gt;/.akidevsync/notes.json</code>, travel with the repo, and now sync along with it - a mirroring PUSH/PULL still cannot erase them.</li>
          <li><strong>Vietnamese typing in the terminal:</strong> OpenKey/EVKey retype a corrected syllable as one event carrying the whole string; the terminal now reads typed text from the input surface itself instead of the key event, so nothing gets truncated.</li>
        </ul>
      </div>

      <!-- Origin Story - moved to bottom, preserved -->
      <div class="alert-box origin mt-3">
        <h4 class="alert-title"><i class="fa-solid fa-bullseye"></i> Origin Story</h4>
        <p class="mb-1">Built to serve the author's own (Lạc Việt Anh) need to streamline a daily coding workflow:</p>
        <ul class="custom-list">
          <li><strong>Local - Source of Truth:</strong> code kept safe with its Git history, edited in a personal <em>Antigravity Pro</em>.</li>
          <li><strong>Remote - AI Workspace:</strong> code pushed up for <em>Claude Code / MAX</em> (a separate account) to generate at scale from the Terminal.</li>
          <li><strong>Reverse Engineering Quota:</strong> Antigravity's quota measured by reverse-engineering the IDE - scan native processes, find the Connect RPC port with <code>lsof</code>, query the local proxy directly.</li>
        </ul>
      </div>
    </div>

    <div class="modal-footer modal-footer-right">
      <button class="btn-tech btn-tech-primary" @click="closeIntroModal">
        <i class="fa-solid fa-check"></i> GOT IT
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue';
import { useIntro } from '../../composables/useIntro';

const { showIntroModal, closeIntroModal } = useIntro();
</script>

<style scoped>
.intro-content {
  font-size: 14px;
  line-height: 1.5;
  color: #d1d5db;
}
.intro-header h3 {
  margin: 0 0 8px 0;
  color: #f3f4f6;
  font-size: 18px;
}
.intro-header p {
  margin: 0;
  color: var(--text-muted);
}
.alert-box {
  padding: 14px;
  border-radius: 6px;
  background: rgba(5, 7, 12, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.alert-box.info {
  background: rgba(59, 130, 246, 0.05);
  border-color: rgba(59, 130, 246, 0.2);
}
.alert-box.origin {
  background: rgba(110, 231, 183, 0.04);
  border-color: rgba(110, 231, 183, 0.15);
}
.alert-box.tech {
  background: rgba(167, 139, 250, 0.05);
  border-color: rgba(167, 139, 250, 0.2);
}
.alert-box.tech .alert-title {
  color: #a78bfa;
}
.alert-title {
  margin: 0 0 10px 0;
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.alert-box.info .alert-title {
  color: #60a5fa;
}
.alert-box.origin .alert-title {
  color: #6ee7b7;
  opacity: 0.8;
}
.model-flow {
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.model-node {
  flex: 1;
  padding: 10px 12px;
  border-radius: 6px;
  background: rgba(5, 7, 12, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.06);
}
.model-node.local {
  border-color: rgba(34, 211, 238, 0.25);
  background: rgba(34, 211, 238, 0.04);
}
.model-node.remote {
  border-color: rgba(245, 158, 11, 0.25);
  background: rgba(245, 158, 11, 0.04);
}
.model-role {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: rgba(255, 255, 255, 0.45);
}
.model-node.local .model-role { color: #22d3ee; }
.model-node.remote .model-role { color: #f59e0b; }
.model-title {
  font-size: 13px;
  font-weight: 700;
  color: #e5e7eb;
  margin: 2px 0;
}
.model-meta {
  font-size: 10px;
  color: var(--text-muted);
}
.model-arrows {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  flex-shrink: 0;
}
.arrow-push, .arrow-pull {
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.05em;
  white-space: nowrap;
}
.arrow-push { color: #6ee7b7; }
.arrow-pull { color: #60a5fa; }
.subgroup-label {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: rgba(255, 255, 255, 0.35);
  text-transform: uppercase;
}
.custom-list {
  margin: 0;
  padding-left: 20px;
}
.custom-list li {
  margin-bottom: 6px;
}
.custom-list li:last-child {
  margin-bottom: 0;
}
.features-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.feature-card {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  background: rgba(5, 7, 12, 0.6);
  padding: 10px 12px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.feature-icon {
  font-size: 16px;
  color: #a78bfa;
  margin-top: 2px;
  flex-shrink: 0;
  width: 16px;
  text-align: center;
}
.feature-text strong {
  display: block;
  color: #e5e7eb;
  font-size: 12px;
  font-weight: 700;
  margin-bottom: 3px;
}
.feature-text span {
  font-size: 11px;
  color: var(--text-muted);
}
.mb-1 { margin-bottom: 4px; }

/* Icon colors - using design system colors */
.icon-guide { color: #6ee7b7; }
.icon-push { color: #a78bfa; }
.icon-select { color: #f59e0b; }
.icon-pull { color: #3b82f6; }
.icon-dryrun { color: #22c55e; }
.icon-syncstatus { color: #a78bfa; }
.icon-hooks { color: #f97316; }
.icon-delete { color: #ef4444; }
.icon-presets { color: #14b8a6; }
.icon-tasks { color: #00d2ff; }
.icon-openpopup { color: #06b6d4; }
.icon-globalnote { color: #f59e0b; }
.icon-usage { color: #818cf8; }
.icon-cloud { color: #f59e0b; }
.icon-ssh { color: #94a3b8; }
.icon-background { color: #ec4899; }
.icon-refresh { color: #fbbf24; }
.icon-git { color: #f87171; }
.icon-update { color: #10b981; }

.modal-footer-right {
  justify-content: flex-end;
}

/* Long inline <code> spans (file paths, shell snippets) are the only unbreakable runs in this guide - let them break instead of widening the modal. */
.intro-content code {
  overflow-wrap: anywhere;
}

/* Narrow mode (SSoT 700px, main.css). Two feature columns leave ~135px of text each at 420px, which turns every card into a ragged column of 2-3 words. One column instead. */
@media (max-width: 700px) {
  .features-grid {
    grid-template-columns: 1fr;
  }

  .model-flow {
    flex-wrap: wrap;
  }

  .model-arrows {
    flex-direction: row;
    justify-content: center;
    gap: 12px;
    width: 100%;
    order: 3;
  }

  .alert-box {
    padding: 10px;
  }
}
</style>
