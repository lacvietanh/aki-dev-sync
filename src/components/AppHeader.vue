<template>
  <div class="dashboard-top-header">
    <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
      <div class="logo-section" data-tauri-drag-region>
        <span class="app-icon-menu" @mousedown.stop title="Menu — links, updates & utilities">
          <img src="/titlebar-icon.png" class="app-icon icon-glow" />
          <span class="menu-affordance"><i class="fa-solid fa-bars"></i></span>
          <div class="icon-dropdown">
            <a
               href="#"
               @click.prevent="showChangelogModal = true"
               class="icon-dropdown-item icon-dropdown-version"
               title="Click to view Changelog">
              <i class="fa-solid fa-clock-rotate-left"></i>
              <span class="version-text-default">{{ appVersion }} {{ buildTime }}</span>
              <span class="version-text-hover">Read Changelog</span>
            </a>
            <div class="icon-dropdown-separator"></div>
            <a href="#" @click.prevent="openLink(REPO_URL)" class="icon-dropdown-item">
              <i class="fa-brands fa-github"></i> GitHub Repository
            </a>
            <a href="#" @click.prevent="openLink(RELEASE_URL)" class="icon-dropdown-item">
              <i class="fa-solid fa-download"></i> Latest Release
            </a>
            <a href="#" @click.prevent="triggerManualUpdateCheck" class="icon-dropdown-item">
              <i class="fa-solid fa-arrows-rotate" :class="{ 'fa-spin': isCheckingUpdates }"></i> Check for Updates
            </a>
            <div class="icon-dropdown-separator"></div>
            <a
               href="#"
               @click.prevent="!(anySyncing || isReloading) && openSshConfig()"
               class="icon-dropdown-item"
               :class="{ 'item-disabled': anySyncing || isReloading }"
               title="Edit SSH Config (Local) — edits this machine's ~/.ssh/config">
              <i class="fa-solid fa-edit"></i> Edit SSH Config (Local)
            </a>
            <a href="#" @click.prevent="enableSshTerminalColor" class="icon-dropdown-item icon-dropdown-item-ssh-color" title="Tints the Terminal background while an SSH session is active, so it's visually distinct from local — row shows the actual tint">
              <i class="fa-solid fa-palette"></i> Enable SSH Terminal Color
            </a>
            <a href="#" @click.prevent="showStatuslineModal = true" class="icon-dropdown-item" title="Build ~/.claude/statusline-command.sh visually, apply to local and/or any configured remote host">
              <i class="fa-solid fa-terminal"></i>
              <span class="statusline-label"><span
                      v-for="(c, i) in statuslineLabelChars"
                      :key="i"
                      :style="c.color ? { color: c.color } : null">{{ c.char }}</span></span>
            </a>
            <a href="#" @click.prevent="showProfileModal = true" class="icon-dropdown-item" title="Claude Code Profile (Local) — Native / Proxy settings for ~/.claude/settings.json on this machine">
              <i class="fa-solid fa-sliders"></i> Claude Code Profile (Local)
            </a>
            <div class="icon-dropdown-separator"></div>
            <a href="#" @click.prevent="openLink(AKICLAUDEDOC_REPO_URL)" class="icon-dropdown-item">
              <i class="fa-brands fa-github"></i> AkiClaudeDoc Repo
            </a>
            <a href="#" @click.prevent="installAkiClaudeDoc" class="icon-dropdown-item">
              <i class="fa-solid fa-download"></i> Install AkiClaudeDoc
            </a>
            <div class="icon-dropdown-separator"></div>
            <div class="icon-dropdown-preset-row">
              <button
                      type="button"
                      class="icon-dropdown-preset-btn"
                      @click="setNarrowWidthSafe"
                      title="Resize window width to 420px (narrow mode), keeping height and position">
                <i class="fa-solid fa-compress"></i> Narrow
              </button>
              <button
                      type="button"
                      class="icon-dropdown-preset-btn"
                      @click="setWideWidthSafe"
                      title="Resize window width to 768px (wide mode), keeping height and position">
                <i class="fa-solid fa-expand"></i> Wide
              </button>
            </div>
            <div class="icon-dropdown-preset-row">
              <button
                      type="button"
                      class="icon-dropdown-preset-btn"
                      @click="stickTopLeftSafe"
                      title="Snap window to the top-left-most connected monitor and resize height to fit the whole project list">
                <i class="fa-solid fa-border-top-left"></i> Stick Top-Left
              </button>
              <button
                      type="button"
                      class="icon-dropdown-preset-btn"
                      @click="centerPrimarySafe"
                      title="Center window on the primary monitor (position only, no resize)">
                <i class="fa-solid fa-crosshairs"></i> Center Primary
              </button>
            </div>
            <div class="icon-dropdown-separator u-wide-hide"></div>
            <a href="#" @click.prevent="openLink(DONATE_URL)" class="icon-dropdown-item u-wide-hide" style="color: #f87171;">
              <i class="fa-solid fa-heart"></i> Donate to AkiDevSync
            </a>
          </div>
        </span>
        <div class="title-block" data-tauri-drag-region>
          <h1 data-tauri-drag-region>Aki Dev Sync</h1>
          <span class="build-narrow u-wide-hide">{{ appVersion }} {{ buildTime }}</span>
          <span v-if="isDev" class="dev-tag">DEV</span>
        </div>
      </div>
      <span class="app-version clickable" @click="showChangelogModal = true" title="Click to view Changelog">
        <span v-if="newVersionAvailable" class="version-row">
          <span class="update-badge" @click.stop="showUpdateModal = true" :title="'New version ' + newVersionAvailable + ' available! Click for details.'">
            <i class="fa-solid fa-circle-arrow-up"></i> Update
          </span>
        </span>
        <span class="build-time u-narrow-hide">{{ appVersion }} {{ buildTime }}</span>
      </span>
      <div class="header-actions">
        <button class="btn-tech btn-tech-secondary btn-intro" @click="openIntroModal" title="Introduction">
          <i class="fa-solid fa-book-open"></i> <span class="btn-text u-narrow-hide">INTRO</span>
          <span class="badge-dot"></span>
        </button>
        <button class="btn-tech btn-tech-secondary btn-note" @click="openGlobalNote" title="Global Note">
          <i class="fa-solid fa-note-sticky" :style="noteContent ? 'color: #f59e0b;' : ''"></i>
        </button>
        <button class="btn-tech btn-tech-secondary btn-donate u-narrow-hide" @click="openLink(DONATE_URL)" title="Donate — support development">
          <i class="fa-solid fa-heart"></i>
        </button>
        <div class="btn-group-refresh">
          <button class="btn-tech btn-tech-secondary btn-refresh-main" @click="handleRefresh" :title="(anyRefreshing || isReloading) ? 'Refreshing all — git, remote diff, usage…' : 'Refresh all — git, remote diff, usage'" :disabled="anySyncing || anyRefreshing || isReloading">
            <i class="fa-solid fa-rotate-right" :class="{ 'fa-spin': anyRefreshing || isReloading }"></i>
          </button>
          <button class="btn-tech btn-tech-secondary btn-refresh-settings" @click="showRefreshSettings = true" title="Background Refresh Settings" :disabled="isReloading">
            <i class="fa-solid fa-sliders"></i>
          </button>
        </div>

        <RefreshSettingsModal :show="showRefreshSettings" @close="showRefreshSettings = false" />
        <ChangelogModal :show="showChangelogModal" @close="showChangelogModal = false" />
        <UpdateModal
                     :show="showUpdateModal"
                     :version="newVersionAvailable"
                     :notes="latestReleaseNotes"
                     :download-url="latestDownloadUrl"
                     :release-url="latestReleaseUrl"
                     @close="dismissUpdateModal" />
        <GlobalNoteModal />
        <ClaudeSettingModal :show="showStatuslineModal" @close="showStatuslineModal = false" />
        <ClaudeProfileModal :show="showProfileModal" @close="showProfileModal = false" />

        <!-- Custom Traffic Lights -->
        <div
             class="titlebar-button pin-btn"
             :class="{ active: isPinned }"
             @click="togglePin"
             :title="isPinned ? 'Unpin from all Spaces' : 'Pin window on top across all Spaces'">
          <i class="fa-solid fa-thumbtack"></i>
        </div>
        <div class="titlebar-button minimize-btn" @click="minimize" title="Minimize">
          <i class="fa-solid fa-window-minimize"></i>
        </div>
        <div class="titlebar-button close-btn" @click="closeWin" title="Close">
          <i class="fa-solid fa-xmark fa-lg"></i>
        </div>
      </div>
    </header>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useAppWindow } from '../composables/useAppWindow';
import { useProjects } from '../composables/useProjects';
import { useSsh } from '../composables/useSsh';
import { useIntro } from '../composables/useIntro';
import { openGlobalNote, noteContent } from '../composables/useGlobalNote';
import { STATUSLINE_COLORS } from '../utils/statuslineColors';
import RefreshSettingsModal from './modals/RefreshSettingsModal.vue';
import ChangelogModal from './modals/ChangelogModal.vue';
import UpdateModal from './modals/UpdateModal.vue';
import GlobalNoteModal from './modals/GlobalNoteModal.vue';
import ClaudeSettingModal from './modals/ClaudeSettingModal.vue';
import ClaudeProfileModal from './modals/ClaudeProfileModal.vue';

const REPO_URL = 'https://github.com/lacvietanh/aki-dev-sync';
const RELEASE_URL = 'https://github.com/lacvietanh/aki-dev-sync/releases/latest';
const AKICLAUDEDOC_REPO_URL = 'https://github.com/lacvietanh/AkiClaudeDoc';
const DONATE_URL = 'https://app.akinet.me/en/qr-bank/?bank=970422&acc=0869297957&tpl=print&amount=0&info=Donate+AkiDevSync&name=LacVietAnh&view=1';
const UPDATE_DISMISS_KEY = 'aki-devsync-update-dismissed';

const appVersion = __APP_VERSION__;
const buildTime = __BUILD_TIME__;
const showRefreshSettings = ref(false);
const showChangelogModal = ref(false);
const showUpdateModal = ref(false);
const showStatuslineModal = ref(false);
const showProfileModal = ref(false);
const isDev = import.meta.env.DEV;
const newVersionAvailable = ref(null);
const isCheckingUpdates = ref(false);

// "Statusline Customizer" row: paint each letter with the actual palette the customizer
// supports (src/utils/statuslineColors.js — the same array ClaudeSettingModal.vue's per-field
// color picker uses), so the row demonstrates the feature instead of describing it in words.
// Spaces are kept in the sequence (so word spacing is unchanged) but don't consume a color.
const STATUSLINE_LABEL = 'Statusline Customizer';
const statuslineLabelChars = (() => {
  let colorIdx = 0;
  return STATUSLINE_LABEL.split('').map((char) => {
    if (char === ' ') return { char, color: null };
    const color = STATUSLINE_COLORS[colorIdx % STATUSLINE_COLORS.length].hex;
    colorIdx++;
    return { char, color };
  });
})();
const latestReleaseNotes = ref('');
const latestDownloadUrl = ref('');
const latestReleaseUrl = ref('');

const {
  startDragging,
  minimize,
  closeWin,
  isPinned,
  togglePin,
  restorePin,
  setNarrowWidth,
  setWideWidth,
  stickTopLeft,
  centerPrimary,
} = useAppWindow();
const { openSshConfig } = useSsh();
const { refreshAllProjects, anySyncing, anyRefreshing, isReloading, Toast } = useProjects();
const { openIntroModal } = useIntro();

const cleanVer = (v) => v.replace(/^v/, '').trim();
const hasUpdate = (current, latest) => {
  const cParts = cleanVer(current).split('.').map(Number);
  const lParts = cleanVer(latest).split('.').map(Number);
  for (let i = 0; i < Math.max(cParts.length, lParts.length); i++) {
    const c = cParts[i] || 0;
    const l = lParts[i] || 0;
    if (l > c) return true;
    if (l < c) return false;
  }
  return false;
};

function applyLatestRelease(latest) {
  newVersionAvailable.value = latest.tag_name;
  latestReleaseNotes.value = latest.body || '';
  const dmgAsset = (latest.assets || []).find(a => a.name && a.name.endsWith('.dmg'));
  latestDownloadUrl.value = dmgAsset ? dmgAsset.browser_download_url : latest.html_url || RELEASE_URL;
  latestReleaseUrl.value = latest.html_url || RELEASE_URL;
}

onMounted(async () => {
  restorePin();
  try {
    const raw = await invoke('check_for_updates');
    const latest = JSON.parse(raw);
    if (latest && latest.tag_name && hasUpdate(appVersion, latest.tag_name)) {
      applyLatestRelease(latest);
      const dismissedVersion = localStorage.getItem(UPDATE_DISMISS_KEY);
      if (dismissedVersion !== latest.tag_name) {
        showUpdateModal.value = true;
      }
    }
  } catch (e) {
    console.error('Failed to check for updates:', e);
  }
});

function dismissUpdateModal() {
  showUpdateModal.value = false;
  if (newVersionAvailable.value) {
    localStorage.setItem(UPDATE_DISMISS_KEY, newVersionAvailable.value);
  }
}

async function triggerManualUpdateCheck() {
  if (isCheckingUpdates.value) return;
  isCheckingUpdates.value = true;
  try {
    const raw = await invoke('check_for_updates');
    const latest = JSON.parse(raw);
    if (latest && latest.tag_name) {
      if (hasUpdate(appVersion, latest.tag_name)) {
        applyLatestRelease(latest);
        showUpdateModal.value = true;
      } else {
        newVersionAvailable.value = null;
        Toast.fire({
          icon: 'success',
          title: `You are on the latest version!`,
          text: `Current: v${appVersion}`
        });
      }
    } else {
      Toast.fire({
        icon: 'error',
        title: 'Failed to verify updates',
        text: 'Invalid server response.'
      });
    }
  } catch (e) {
    console.error('Failed manual update check:', e);
    Toast.fire({
      icon: 'error',
      title: 'Failed to check updates',
      text: String(e)
    });
  } finally {
    isCheckingUpdates.value = false;
  }
}

function openLink(url) {
  invoke('macos_open', { args: [url] }).catch(console.error);
}

async function enableSshTerminalColor() {
  try {
    await invoke('install_ssh_terminal_color');
    Toast.fire({
      icon: 'success',
      title: 'SSH terminal color enabled',
      text: 'Open a new terminal window for it to take effect.'
    });
  } catch (e) {
    Toast.fire({ icon: 'error', title: 'Failed to enable SSH terminal color', text: String(e) });
  }
}

async function installAkiClaudeDoc() {
  try {
    await invoke('install_akiclaudedoc');
  } catch (e) {
    Toast.fire({ icon: 'error', title: 'AkiClaudeDoc not found on this machine', text: String(e) });
  }
}

// Refreshes every project's derived status (git, remote diff, stack) plus the usage monitors —
// exactly what the tooltip promises, and exactly the same unit of work a single project's own
// Refresh button runs. It deliberately does NOT call loadData(): that re-reads projects.json, SSH
// hosts and IDE availability from disk, which is an app-load concern, not a refresh. Routing this
// button through loadData was why "everything dims" came from loadData's global isReloading flag
// instead of from the projects themselves — making the global and per-project buttons two
// unrelated mechanisms that only looked like one feature.
function handleRefresh() {
  refreshAllProjects();
}

function setNarrowWidthSafe() {
  setNarrowWidth().catch((e) => console.error('Failed to set narrow width:', e));
}

function setWideWidthSafe() {
  setWideWidth().catch((e) => console.error('Failed to set wide width:', e));
}

function stickTopLeftSafe() {
  stickTopLeft().catch((e) => console.error('Failed to stick window top-left:', e));
}

function centerPrimarySafe() {
  centerPrimary().catch((e) => console.error('Failed to center window:', e));
}
</script>

<style scoped>
.app-icon-menu {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  padding: 2px;
  border-radius: 6px;
  transition: background 0.15s;
  vertical-align: middle;
  margin-right: 6px;
}

.app-icon-menu:hover {
  background: rgba(0, 210, 255, 0.08);
}

.menu-affordance {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  font-size: 12px;
  color: #a5f3fc;
  background: rgba(0, 210, 255, 0.08);
  border: 1px solid rgba(0, 210, 255, 0.25);
  border-radius: 6px;
  transition: color 0.15s, background 0.15s, box-shadow 0.15s;
}

.app-icon-menu:hover .menu-affordance {
  color: #fff;
  background: rgba(0, 210, 255, 0.2);
  box-shadow: inset 0 0 8px rgba(0, 210, 255, 0.3);
}

.icon-dropdown {
  display: none;
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 1000;
  background: #1a1d23;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 7px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04);
  min-width: 180px;
  padding: 4px;
  white-space: nowrap;
  /* reset h1 inherited styles */
  text-shadow: none;
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
  font-size: 13px;
  /* SSoT echo: exact OSC 11 background src-tauri/src/system.rs patches into the terminal
     (SSH_COLOR_SNIPPET, `printf '\033]11;#1a0f0f\007'`) — mirrored here once so the "Enable SSH
     Terminal Color" row can show the real tint instead of describing it. */
  --ssh-terminal-bg: #1a0f0f;
}

.icon-dropdown::before {
  content: '';
  position: absolute;
  top: -6px;
  left: 0;
  right: 0;
  height: 6px;
}

.app-icon-menu:hover .icon-dropdown {
  display: block;
}

.icon-dropdown-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  font-size: 12px;
  color: #94a3b8;
  text-decoration: none;
  border-radius: 5px;
  transition: background 0.12s, color 0.12s;
}

.icon-dropdown-item:hover {
  background: rgba(255, 255, 255, 0.07);
  color: #e2e8f0;
}

.icon-dropdown-item i {
  width: 14px;
  text-align: center;
  color: #64748b;
}

.icon-dropdown-item:hover i {
  color: #94a3b8;
}

.icon-dropdown-item.item-disabled {
  opacity: 0.4;
  pointer-events: none;
  cursor: not-allowed;
}

/* Statusline Customizer row: each letter keeps its palette color regardless of hover — the
   inline per-letter color (set in the template from statuslineLabelChars) intentionally beats
   the generic .icon-dropdown-item:hover text-color rule above, since the whole point is a
   persistent, always-visible preview rather than a hover-only reveal. */
.statusline-label {
  white-space: nowrap;
}

/* Enable SSH Terminal Color row: background + text mirror the real tint (see --ssh-terminal-bg
   above) so the row demonstrates its own effect. Foreground is a light warm red, legible against
   the very dark red-tinted background. */
.icon-dropdown-item-ssh-color {
  background: var(--ssh-terminal-bg);
  color: #fca5a5;
}

.icon-dropdown-item-ssh-color i {
  color: #f87171;
}

.icon-dropdown-item-ssh-color:hover {
  background: #2a1414;
  color: #fecaca;
}

.icon-dropdown-item-ssh-color:hover i {
  color: #fca5a5;
}

.icon-dropdown-separator {
  height: 1px;
  background: rgba(255, 255, 255, 0.08);
  margin: 4px 6px;
}

.icon-dropdown-preset-row {
  display: flex;
  gap: 4px;
  padding: 0 2px;
}

.icon-dropdown-preset-row+.icon-dropdown-preset-row {
  margin-top: 4px;
}

.icon-dropdown-preset-btn {
  flex: 1 1 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 8px;
  font-size: 11px;
  font-family: inherit;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 5px;
  cursor: pointer;
  transition: background 0.12s, color 0.12s, border-color 0.12s;
  white-space: nowrap;
}

.icon-dropdown-preset-btn:hover {
  background: rgba(0, 210, 255, 0.1);
  color: #e2e8f0;
  border-color: rgba(0, 210, 255, 0.3);
}

.icon-dropdown-preset-btn i {
  width: 12px;
  text-align: center;
  color: #64748b;
}

.icon-dropdown-preset-btn:hover i {
  color: #a5f3fc;
}

.btn-intro {
  position: relative;
  margin-right: 4px;
}

.btn-note {
  margin-left: 10px;
}

.btn-donate {
  margin-left: 6px;
  color: #f87171;
}

.btn-donate:hover:not(:disabled) {
  color: #fb7185;
  border-color: rgba(251, 113, 133, 0.5);
  background: rgba(251, 113, 133, 0.12);
}

.badge-dot {
  position: absolute;
  top: -2px;
  right: -2px;
  width: 10px;
  height: 10px;
  background-color: #ef4444;
  border-radius: 50%;
  border: 2px solid #131317;
  animation: pulse-red 2s infinite;
}

@keyframes pulse-red {
  0% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.7);
  }

  70% {
    transform: scale(1);
    box-shadow: 0 0 0 6px rgba(239, 68, 68, 0);
  }

  100% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0);
  }
}

.btn-group-refresh {
  display: flex;
  align-items: center;
  gap: 0;
}

.btn-refresh-main {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
  border-right: none;
  padding: 0 10px;
}

.btn-refresh-settings {
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  padding: 0 10px;
}

.btn-refresh-main:hover:not(:disabled)+.btn-refresh-settings,
.btn-refresh-settings:hover:not(:disabled) {
  border-left-color: rgba(255, 255, 255, 0.4);
}

.app-version {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: #64748b;
  margin-left: 8px;
  margin-right: auto;
  vertical-align: middle;
  font-weight: normal;
  letter-spacing: 1px;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.version-num {
  flex-shrink: 0;
}

.build-time {
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.app-version.clickable {
  cursor: pointer;
  transition: color 0.2s;
}

.app-version.clickable:hover {
  color: #3b82f6;
}

.title-block {
  display: flex;
  align-items: center;
}

.dev-tag {
  background-color: rgba(239, 68, 68, 0.15);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.4);
  font-size: 10px;
  font-weight: bold;
  padding: 1px 6px;
  border-radius: 4px;
  margin-left: 8px;
  vertical-align: middle;
  letter-spacing: 0.5px;
  display: inline-block;
}

.version-row {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.update-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: rgba(16, 185, 129, 0.15);
  color: var(--accent-green, #10b981);
  border: 1px solid rgba(16, 185, 129, 0.3);
  padding: 1px 5px;
  border-radius: 3px;
  font-size: 9px;
  font-weight: 700;
  cursor: pointer;
  letter-spacing: 0;
  transition: all 0.15s;
}

.update-badge:hover {
  background: rgba(16, 185, 129, 0.25);
  color: #fff;
}

.pin-btn.active {
  color: #ef4444;
  background-color: rgba(239, 68, 68, 0.15);
  box-shadow: inset 0 0 8px rgba(239, 68, 68, 0.4);
}

.pin-btn.active i {
  transform: rotate(45deg);
}

/* Hover swaps version/build for "Read Changelog". No width jump: the dropdown's width is set by
   its longest item ("Claude Code Profile (Local)"), which is wider than either of these labels. */
.icon-dropdown-version .version-text-hover {
  display: none;
}

.icon-dropdown-version:hover .version-text-default {
  display: none;
}

.icon-dropdown-version:hover .version-text-hover {
  display: inline;
}

@media (max-width: 700px) {
  .logo-section h1 {
    font-size: 8px;
    line-height: 1;
  }

  /* DEV badge now stacks under the title text only (not the whole logo-section, which also
     contains the menu icon) — .title-block wraps just the h1 + dev-tag, so the icon stays on
     the same row while the title/DEV pair goes vertical to save horizontal space. A small gap
     (instead of 0) keeps the badge from visually running into the title text right below it. */
  .title-block {
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
  }

  .dev-tag {
    font-size: 6px;
    padding: 0 3px;
    margin-left: 0;
    line-height: 1.3;
  }

  /* Build info shown at the DEV-tag position — same red, same tiny size, no click. DEV renders
     below it (last in column) so the order reads: build ⟶ DEV if in dev mode. */
  .build-narrow {
    font-size: 6px;
    color: #f87171;
    font-weight: 600;
    letter-spacing: 0.5px;
    line-height: 1.3;
  }

  /* Disable changelog click in narrow — accidental opens during window drag are common. The
     update badge keeps its own pointer-events so it stays tappable. */
  .app-version.clickable {
    pointer-events: none;
    cursor: default;
  }

  .app-version .update-badge {
    pointer-events: auto;
  }

  /* Gap between the INTRO button and the Global Note button, halved (10px -> 5px). */
  .btn-note {
    margin-left: 5px;
  }

  /* Folded in from the old 850px breakpoint (removed — 700px is the single narrow breakpoint;
     INTRO's label now hides via u-narrow-hide like everything else). */
  .header-actions .btn-tech {
    padding: 0 8px !important;
  }
}
</style>
