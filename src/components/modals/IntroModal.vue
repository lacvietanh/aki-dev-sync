<template>
  <BaseModal :show="showIntroModal" @close="closeIntroModal">
    <template #title>
      <i class="fa-solid fa-book-open mr-1" style="color: #6ee7b7;"></i> User Guide - Aki Dev Sync
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
          <div class="feature-icon"><i class="fa-solid fa-arrow-up"></i></div>
          <div class="feature-text">
            <strong>PUSH</strong>
            <span>Push Local → Remote with everything not listed in <code>push_excludes</code>, plus a per-project "Force Delete" setting. A folder listed only in <code>pull_excludes</code> is <strong>push-only</strong> - sent up, never pulled back, never counted as a change (<code>.git/</code> by default: the AI gets full history, and the PUSH badge stays dark through git's own housekeeping).</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-bolt"></i></div>
          <div class="feature-text">
            <strong>SELECT (Push Special)</strong>
            <span>Opens the native macOS file picker (multi-select). If a file already exists on Remote, shows a Local / Remote mtime comparison before you confirm the overwrite.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #3b82f6;"><i class="fa-solid fa-arrow-down"></i></div>
          <div class="feature-text">
            <strong>PULL</strong>
            <span>Pull back what was written on Remote so you can review and commit it on Local.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #22c55e;"><i class="fa-solid fa-shield-halved"></i></div>
          <div class="feature-text">
            <strong>DRY RUN</strong>
            <span>Preview exactly which files would change - nothing is written until you turn this off.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #a78bfa;"><i class="fa-solid fa-circle-dot"></i></div>
          <div class="feature-text">
            <strong>Sync Status</strong>
            <span>Checked automatically - the PUSH/PULL buttons light up when the two sides drift apart.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f97316;"><i class="fa-solid fa-code"></i></div>
          <div class="feature-text">
            <strong>Pre / Post Hooks</strong>
            <span>Scripts run before/after each sync (build, restart a service, notify...), on Local or Remote as you choose.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ef4444;"><i class="fa-solid fa-clone"></i></div>
          <div class="feature-text">
            <strong>Mirror / Delete</strong>
            <span>Turn on <code>--delete</code> for an exact mirror. Push never deletes by default; once enabled, pushing over newer Remote changes asks for confirmation first.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #14b8a6;"><i class="fa-solid fa-layer-group"></i></div>
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
          <div class="feature-icon" style="color: #00d2ff;"><i class="fa-solid fa-list-check"></i></div>
          <div class="feature-text">
            <strong>Project Tasks & Notes</strong>
            <span>Task management: Pin 📌, Wish 🕒 and Done (which auto-unpins). Includes a Project Notes card that grows natively with its content via CSS (`field-sizing: content`) and trims stray whitespace.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #06b6d4;"><i class="fa-solid fa-grip"></i></div>
          <div class="feature-text">
            <strong>Open Popup & Stack Launcher</strong>
            <span>Quick-open Local and Remote IDEs. <strong>DEV</strong> (green) and <strong>BUILD</strong> (amber) follow the detected stack, tooltip shows the exact command. Lockfile scan picks the right package manager. Every Terminal window opened snaps to 124 columns in the top-right of the main display.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-note-sticky"></i></div>
          <div class="feature-text">
            <strong>Global Note</strong>
            <span>A global notepad on the titlebar - the icon turns amber when it has content. Not tied to any project, auto-saved after 500ms to <code>appDataDir</code>.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #818cf8;"><i class="fa-solid fa-chart-bar"></i></div>
          <div class="feature-text">
            <strong>Agent Usage</strong>
            <span>Real quota: <strong>Claude Code</strong> reads Anthropic's <code>rate_limits</code> (5H + 7D) - on this Mac or on any SSH host - showing plan tier, email and org name. <strong>Antigravity</strong> covers all 3 surfaces: <code>AG</code> (Desktop App), <code>IDE</code> (VS Code extension) and <code>CLI</code> (terminal), with a lightweight 1-pass Smart Deduplicate (~40ms). The two auth domains stay separate (IDE SQLite vs the shared Gemini core in <code>~/.gemini/</code>), each with its own contextual Log Out. Both agents can be watched <strong>locally or on any number of SSH hosts at once</strong>: every slot has its own host picker, so one slot watches Claude Code on host A while another watches host B under a different account. Menu ☰ → <code>Usage row:</code> picks <strong>1 row</strong> (2 slots side by side) or <strong>2 rows</strong> (4 slots). Two slots pointed at the same source is <em>allowed</em> - they share one instance and one poll instead of doubling it. Each monitor has its own power icon; switch one off and its last reading stays on screen marked as cached, switch it back on and it resumes. Reset countdowns tick in real time. When a pool's 7-day quota hits 100%, <em>that pool's</em> 5-hour number is dimmed and dropped from the colour ladder - a spent pool makes its 5h figure meaningless, so your eye goes to the number that still counts.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-cloud"></i></div>
          <div class="feature-text">
            <strong>Sync Check & remote monitors - independent switches</strong>
            <span>The power icon in the SYNC column toggles PUSH/PULL/SELECT, the remote IDE entries in the Open popup, and sync diff checks (background + manual). In Agent Usage's REMOTE tab, the <code>AG</code> and <code>CC</code> tabs each have their own power icon, toggling the Antigravity and Claude Code monitors independently. The host picker beside them belongs to <strong>that slot alone</strong>, so one slot watches Claude Code on host A while another watches host B under a different account - two machines side by side. Switching one off never switches off another. Every monitor is on by default; each active remote monitor costs one SSH round-trip per refresh cycle, and its power icon stops it completely.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #94a3b8;"><i class="fa-solid fa-key"></i></div>
          <div class="feature-text">
            <strong>SSH Config</strong>
            <span>Manage <code>~/.ssh/config</code> with undo/redo - no terminal needed. The host picked here is the <strong>default</strong> for any Agent Usage slot that has not chosen its own; to change a slot's host, use the picker in that slot's header.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ec4899;"><i class="fa-solid fa-rotate"></i></div>
          <div class="feature-text">
            <strong>Background Refresh</strong>
            <span>Checks Git status, sync diff and agent usage on a configurable cycle. A countdown ring shows live progress right on the GIT and ACTIONS column headers.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #fbbf24;"><i class="fa-solid fa-arrows-rotate"></i></div>
          <div class="feature-text">
            <strong>Refresh - 1 unit of work</strong>
            <span>The per-project Refresh button and the global one in the titlebar both call the same <code>refreshProject()</code> (git status + sync diff + stack detection, in parallel) - no more full app reload. Spinner state lives on each check itself, so a background cycle spins the icon at the right moment too.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #f87171;"><i class="fa-brands fa-git-alt"></i></div>
          <div class="feature-text">
            <strong>Git & Changelog Visual Preview</strong>
            <span>The Git modal renders log/status in ANSI terminal colours, handles Vietnamese accents (quotepath=false), and supports stage & commit, fetch, push, plus a Visual Changelog Preview of the project.</span>
          </div>
        </div>
        <div class="feature-card">
          <div class="feature-icon" style="color: #10b981;"><i class="fa-solid fa-cloud-arrow-down"></i></div>
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
          <li><strong>Git terminal colours & Unicode:</strong> Force Git to emit raw ANSI colour (`color.status=always`), parse it with regex into coloured HTML spans (Red/Green/Yellow/Cyan/Bold), and disable quotepath (`core.quotepath=false`) so Vietnamese filenames render perfectly.</li>
          <li><strong>Stack Detector & Lockfile Analyzer:</strong> Detects Tauri vs Node and reads lockfiles (`pnpm`, `yarn`, `bun`, `npm`) to run the right dev/preview command with no manual config.</li>
          <li><strong>Native textarea autogrow (CSS-only):</strong> The new `field-sizing: content` property grows Tasks & Notes to fit their actual content - not one line of JS resize code, and no layout jank.</li>
          <li><strong>Inherited Changelog modal:</strong> A `projectId` parameter plus a custom title/content lets one shared Changelog component serve each project's own changelog too.</li>
          <li><strong>Real quota:</strong> Reads the `rate_limits` Anthropic's server returns through the `statusLine` hook - nothing patched together, no simulated requests.</li>
          <li><strong>Hybrid Patching:</strong> At 100% the Claude CLI hides `rate_limits`. The app estimates the reset time itself so the UI keeps showing something accurate.</li>
          <li><strong>Parallel quota (v1.3.0):</strong> Two Connect RPC endpoints queried at once for the 5H and Weekly quotas of both the Gemini and Claude/GPT pools, grouped into compact fieldsets.</li>
          <li><strong>Antigravity Native RPC:</strong> Skips Google's API (usually empty) - scans native processes and finds the port with `lsof` to query Connect RPC on the local proxy, ~40ms.</li>
          <li><strong>Antigravity Log Out that really logs out:</strong> Clearing Cookies/Local Storage is not enough - the login token is encrypted by Electron's `safeStorage`, whose AES key lives in exactly one macOS Keychain item (`"Antigravity IDE Safe Storage"`). Log Out quits the app and deletes that one item (no Keychain scan or dump), leaving the token permanently unreadable - settings, extensions and rules live in separate files and are untouched.</li>
          <li><strong>Claude Code - one source of truth:</strong> The app never runs <code>claude</code> itself to fetch usage. The old active path (force-sync, Haiku probe) is deleted outright: it once leaked 19 orphaned sessions (6GB RAM + 4GB swap) on a remote host, and a real measurement showed a headless turn returns only the reset time, never a percentage. The <code>statusLine</code> hook cache is now the only source.</li>
          <li><strong>Two CLIs, ONE statusline script:</strong> The same file is installed for both Claude Code (<code>~/.claude/statusline-command.sh</code>) and AGY CLI (<code>~/.gemini/antigravity-cli/statusline.sh</code>), and <strong>each CLI is pointed at it through the <code>statusLine</code> key in its own <code>settings.json</code></strong> - writing the file alone is half an install and the CLI runs nothing. It works out which CLI is running it from its own path (<code>$0</code>), not by guessing from the payload. Pick the targets in the modal title, apply to several hosts in parallel. Every line opens with a coloured <code>CC</code>/<code>AG</code> tag plus the account name, so you know which CLI and which account printed it. The <code>rate_limits</code> cache is now bound to the account that wrote it and drops expired entries - no more phantom quota from another session or account. Each field has its own truncate width, blocks are separated by alternating light/dark backgrounds (zebra) instead of <code>|</code>, and every option in the modal has an automated test proving it really reaches the file. Each host shows a <code>CC</code>/<code>AG</code> tag for the CLIs it actually has: filled means the statusline is running, hollow means the CLI is there but nothing is wired up yet.</li>
          <li><strong>App-icon menu & window presets:</strong> The ☰ titlebar menu collects GitHub/Release links, manual update check, SSH Config, Enable SSH Terminal Color, Statusline Customizer, Claude Code Profile (Local), Remote Control, the AkiClaudeDoc installer, a <code>Usage row:</code> picker for 1 row (2 slots) or 2 rows (4 slots), and 4 window presets under <code>AppWindow:</code> - Narrow (420px), Wide (768px), Stick Top-Left (auto-fits height to the project list), Center Primary. <code>⌘1</code> = Narrow + Stick Top-Left, <code>⌘2</code> = Wide + Center Primary (both work without opening the menu). Tick <strong>remember</strong> to re-apply the width and placement you chose on the next launch (width first, since Stick Top-Left fits its height to the current width).</li>
          <li><strong>Remote Control (v1.19.0, preview):</strong> Control the app from a phone browser on the same LAN or over Tailscale. Menu ☰ → <strong>Remote Control</strong> → <strong>On</strong> shows a 6-digit pair code and the <code>IP:PORT</code> rows to open on the phone (click to copy); pair once, and it reconnects silently after that. The Mac stays the single source of truth - the phone only mirrors its state and sends gestures back over one WebSocket. <strong>Off</strong> cuts every live phone and stops serving on that port; 10 wrong codes disable it automatically. Same address in dev and release (<code>:1421</code>). The <strong>HTTPS (PWA)</strong> row serves over HTTPS via Tailscale, which is what lets the phone <em>Install</em> it as a standalone app (needs HTTPS certs enabled once in the Tailscale admin console). Security note: a paired device can invoke any Tauri command - there is no allowlist, the pair code is the only gate.<br><strong>v1.20.0:</strong> confirmation dialogs are now mirrored state - trigger a PUSH <code>--delete</code> from the phone and the "type the project name" box appears on <em>both</em> screens, answerable from either (the typed name is still verified on the Mac, so the phone cannot skip the check). Same for Remove Project, the replacement SSH host picker, and the "preview failed, continue anyway?" prompt. Also fixes task/note edits made on the phone reverting a while later: an edit used to be written straight to disk without touching the Mac's live state, so the next reconnect (screen lock, tab switch, network drop) replayed the stale copy over it.</li>
          <li><strong>In-app terminal (v1.20.0):</strong> A <code>TERMINAL</code> tab beside the event log running a real shell on the Mac (a genuine PTY, not a command runner piping output) - so history recall with the arrow keys, <code>Ctrl+C</code> on a running command, interactive prompts and <code>vim</code> all work. <strong>One shared session:</strong> whatever you type on the Mac or on the phone lands in the same shell and both screens show identical output - which is the point, since <code>Terminal.app</code> renders nowhere but the Mac, so a phone could never drive it. The Mac stays the sole resize authority (a PTY has one size; letting the phone set it would let a narrow screen mangle output running on the Mac). A slim key row covers Esc / Tab / sticky Ctrl (tap Ctrl, then C) / the four arrows / Enter for what a phone keyboard cannot send, and recent output replays when you reopen the tab or the phone reconnects. The shell runs as a <em>login shell</em>, so it has the same <code>PATH</code> as every other Terminal window (nvm, rbenv, path_helper all load). The panel header carries <strong>CLEAR</strong> (wipes the Mac's stored scrollback, not just the screen in front of you, so the phone stays clear too and it does not come back on reconnect), <strong>RESTART</strong>, <strong>KILL</strong> and <strong>OPEN</strong> (hands the shell's <em>current</em> directory to <code>Terminal.app</code>). When the shell exits (<code>exit</code>, or a crash) the terminal prints <code>[process exited]</code> and the tab turns red instead of going quiet - typing anything starts a new one. Each project's OPEN popup also has <strong>In-App Terminal</strong>, dropping the shared shell straight into that project - the only one of the two terminal entries that works from a phone. Security: opening the terminal from a paired device adds <em>no</em> extra confirmation - that device could already run arbitrary commands via DEV/BUILD, so gating the terminal alone would be theatre and would defeat the point (you use the phone precisely when you are not at the Mac). <strong>Off</strong> still cuts every device instantly.</li>
          <li><strong>The `.git/` mtime trap:</strong> Directory-only mtime changes from git's internal housekeeping are filtered out of the dry-run result, so the PUSH button no longer lights up for nothing.</li>
          <li><strong>Bidirectional EC-3 disambiguation (Baseline Manifest):</strong> rsync cannot tell "remote created file X" from "Local deleted file X", or "Mac created file Y" from "remote deleted file Y". After each full sync the app writes a snapshot of the local file list to <code>appDataDir/baselines/</code>. On the next check: in pull_list + in baseline + gone from Local → Local deleted it → add to push_count; in push_list + in baseline → remote deleted it → drop from push_count. Fully solves the PUSH badge lighting up wrongly when most coding happens on the remote.</li>
          <li><strong>Narrow Mode (v1.14.0):</strong> The window stays usable down to 420px (<code>minWidth</code> in <code>tauri.conf.json</code>), driven by one shared breakpoint (700px) and two global utility classes (<code>.u-narrow-hide</code>/<code>.u-wide-hide</code>) - no component defines its own. A hidden text label survives in the <code>title</code> tooltip, and no button ever loses both its icon and its label.</li>
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

    <div class="modal-footer" style="justify-content: flex-end;">
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
