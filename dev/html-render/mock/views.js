// Renders the mock screens from window.MOCK. Markup copies the class structure of the real templates in src/components, so main.css and sfc.css style it exactly as the app; when a template changes, change its function here.
const M = window.MOCK
const PUB = '../../../public'
const view = new URLSearchParams(location.search).get('view') || 'main'
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
const each = (list, fn) => list.map(fn).join('')

// RefreshRing.vue, frozen part-way through its countdown.
const ring = (overlay, color, frac = 0.62) => `<svg class="refresh-ring ${overlay ? 'refresh-ring--overlay' : 'refresh-ring--inline'}" viewBox="0 0 36 36"><circle class="rr-track" cx="18" cy="18" r="15"/><circle class="rr-fill" cx="18" cy="18" r="15" style="stroke:${color};stroke-dashoffset:${94.25 * (1 - frac)}"/></svg>`
const pctClass = (pct, muted = false) => (muted ? 'color-muted' : pct === null ? 'color-na' : pct <= 70 ? 'color-safe' : pct <= 90 ? 'color-warning' : 'color-danger')

// AppHeader.vue
const header = ({ menu = false, native = true } = {}) => `
<div class="dashboard-top-header"><header class="top-header">
  <div class="logo-section">
    <span class="app-icon-menu${menu ? ' mock-menu-open' : ''}">
      <img src="${PUB}/titlebar-icon.png" class="app-icon icon-glow">
      <span class="menu-affordance"><i class="fa-solid fa-bars"></i></span>
      ${menu ? dropdown() : ''}
    </span>
    <div class="title-block"><h1>Aki Dev Sync</h1></div>
  </div>
  <span class="app-version"></span>
  <div class="header-actions">
    <button class="btn-tech btn-tech-secondary btn-intro"><i class="fa-solid fa-book-open"></i> <span class="btn-text u-narrow-hide">INTRO</span></button>
    <button class="btn-tech btn-tech-secondary btn-note"><i class="fa-solid fa-note-sticky has-note"></i><span class="cell-badge cell-badge-top task-badge-pin">1</span><span class="cell-badge cell-badge-bottom task-badge-normal">4</span></button>
    <button class="btn-tech btn-tech-secondary btn-shortcuts"><i class="fa-solid fa-keyboard"></i></button>
    <button class="btn-tech btn-tech-secondary btn-donate u-narrow-hide"><i class="fa-solid fa-heart"></i></button>
    <div class="btn-group-refresh"><button class="btn-tech btn-tech-secondary btn-refresh-main"><i class="fa-solid fa-rotate-right"></i></button><button class="btn-tech btn-tech-secondary btn-refresh-settings"><i class="fa-solid fa-sliders"></i></button></div>
    ${native ? `<div class="titlebar-button pin-btn"><i class="fa-solid fa-thumbtack"></i></div><div class="titlebar-button minimize-btn"><i class="fa-solid fa-window-minimize"></i></div><div class="titlebar-button close-btn"><i class="fa-solid fa-xmark fa-lg"></i></div>` : ''}
  </div>
</header></div>`

const presetBtn = (icon, label, active) => `<button type="button" class="icon-dropdown-preset-btn${active ? ' is-active' : ''}"><i class="${icon}"></i> ${label}</button>`
const menuItem = (icon, label, cls = '') => `<a class="icon-dropdown-item ${cls}"><i class="${icon}"></i> ${label}</a>`
const RAINBOW = ['#ff5f5f', '#ffaf5f', '#ffff5f', '#5fff87', '#5fd7ff', '#af87ff']
const dropdown = () => `
<div class="icon-dropdown">
  <div class="icon-dropdown-preset-row">${presetBtn('fa-solid fa-arrows-rotate', 'Check Update')}${presetBtn('fa-solid fa-code-merge', 'Release')}</div>
  <div class="icon-dropdown-separator"></div>
  ${menuItem('fa-solid fa-edit', 'Edit SSH Config (Local)')}
  ${menuItem('fa-solid fa-palette', 'Enable SSH Terminal Color', 'icon-dropdown-item-ssh-color')}
  <div class="icon-dropdown-ac-section"><span class="ac-title"><i class="fa-solid fa-water"></i> Glass Effect:</span><label class="remember-view"><input type="checkbox"> Off</label></div>
  <a class="icon-dropdown-item statusline-menu-item"><i class="fa-solid fa-terminal"></i>
    <span class="statusline-label">${[...'Statusline Customizer'].map((c, i) => `<span style="color:${c === ' ' ? 'inherit' : RAINBOW[i % RAINBOW.length]}">${c}</span>`).join('')}</span>
    <span class="statusline-menu-targets"><img src="${PUB}/claude-icon.png" class="statusline-target-icon"><img src="${PUB}/antigravity-app-icon.png" class="statusline-target-icon ag"></span></a>
  ${menuItem('fa-solid fa-sliders', 'Claude Code Profile (Local)')}
  ${menuItem('fa-solid fa-broom', 'Claude Code Cleanup (Local)')}
  ${menuItem('fa-solid fa-shield-halved', 'Pre-allow AGY Commands')}
  <div class="icon-dropdown-separator"></div>
  <div class="icon-dropdown-ac-section"><span class="ac-title"><i class="fa-solid fa-tower-broadcast"></i> Remote Control:</span><label class="remember-view remote-toggle on plain-http"><input type="checkbox" checked> On</label></div>
  <div class="remote-code-row"><span class="remote-code-label"><i class="fa-solid fa-key"></i> Pair code</span><span class="remote-code">482 913</span></div>
  ${each(M.remoteUrls, ([kind, url]) => `<a class="icon-dropdown-item remote-url-item"><i class="fa-solid ${kind === 'tailscale' ? 'fa-globe' : 'fa-wifi'}"></i><span class="remote-url-kind">${kind}</span><span class="remote-url">${url}</span><i class="fa-regular fa-copy remote-copy-ic"></i></a>`)}
  <div class="remote-hint">Open a URL on your phone → enter the code.</div>
  <a class="icon-dropdown-item remote-url-item"><i class="fa-solid fa-sliders"></i><span class="remote-url">Remote Settings…</span></a>
  <div class="icon-dropdown-separator"></div>
  <div class="icon-dropdown-ac-section"><span class="ac-title"><i class="fa-solid fa-book"></i> AkiDevRule:</span><div class="icon-dropdown-preset-row ac-row">${presetBtn('fa-brands fa-github', 'Repo')}${presetBtn('fa-solid fa-download', 'Install')}</div></div>
  <div class="icon-dropdown-separator"></div>
  <div class="icon-dropdown-ac-section"><span class="ac-title"><i class="fa-solid fa-layer-group"></i> Usage slots:</span><select class="ac-row tier-slot-select"><option>4 slots (2 rows)</option></select></div>
  <div class="icon-dropdown-separator"></div>
  <div class="icon-dropdown-ac-section"><span class="ac-title"><i class="fa-solid fa-window-maximize"></i> AppWindow:</span><label class="remember-view on"><input type="checkbox" checked> remember</label></div>
  <div class="view-preset-grid">
    <div class="icon-dropdown-preset-row">${presetBtn('fa-solid fa-compress', 'Narrow')}${presetBtn('fa-solid fa-expand', 'Wide', true)}${presetBtn('fa-solid fa-up-right-and-down-left-from-center', 'Ultra')}</div>
    <div class="icon-dropdown-preset-row">${presetBtn('fa-solid fa-border-top-left', 'Stick Top-Left', true)}${presetBtn('fa-solid fa-crosshairs', 'Center Primary')}</div>
  </div>
  <div class="icon-dropdown-separator"></div>
  ${menuItem('fa-solid fa-clock-rotate-left', 'View Changelog')}
  ${menuItem('fa-brands fa-github', 'GitHub Repository')}
  ${menuItem('fa-solid fa-heart', 'Donate to AkiDevSync', 'donate-link')}
</div>`

// AgentUsageSection.vue → AgentUsageSlot.vue → AgentUsage.vue
const agPoolRow = (p) => `
<div class="ag-pool-row">
  <div class="ag-pool-head"><span class="ag-pool-name">${p.label}</span></div>
  <div class="ag-pool-windows">${each(p.windows, (w) => `
    <div class="ag-win-col${w.muted ? ' is-muted' : ''}${w.disabled ? ' is-disabled' : ''}">
      <div class="ag-win-meta"><span class="ag-win-label">${w.label}</span><span class="ag-pct-val ${pctClass(w.pct, w.muted)}">${w.pct === null ? '—' : w.pct + '%'}</span><span class="ag-time-val">${w.val}</span></div>
      <div class="ag-track"><div class="ag-track-fill ${pctClass(w.pct, w.muted)}" style="width:${w.pct ?? 0}%"></div></div>
    </div>`)}</div>
</div>`
const slotTab = (active, inner) => `<button class="slot-tab${active ? ' is-active' : ''}">${inner}</button>`
const srcTab = (s, key, icon, label) => slotTab(s.agent === key, `<i class="fa-solid fa-power-off src-power is-on"></i><img src="${PUB}/${icon}" class="src-icon"> <span class="u-narrow-hide">${label}</span>`)
const usageSlot = (s) => {
  const cc = s.agent === 'cc'
  const icon = cc ? `<img src="${PUB}/claude-icon.png" class="agent-img-icon icon-glow">` : s.source === 'cli' ? '<i class="fa-solid fa-terminal agent-cli-icon clickable-icon"></i>' : `<img src="${PUB}/antigravity-icon.png" class="agent-img-icon ag-ide-icon clickable-icon">`
  const account = `<span class="ag-account-wrap"><span class="agent-account ag-account-trigger"><span class="email-prefix">${s.account}</span><span class="email-blurred-fixed">••••••••</span></span></span><button class="btn-eye-inline"><i class="fa-regular fa-eye-slash"></i></button>`
  const name = cc
    ? `<div class="agent-name-row"><span class="agent-name u-narrow-hide">${s.name}</span><span class="agent-plan-badge claude">${s.plan}</span>${account}</div>`
    : `<div class="agent-name"><span class="u-narrow-hide">${s.source.toUpperCase()}</span>${s.plan ? `<span class="agent-plan-badge ag">${s.plan}</span>` : ''}${account}</div>`
  const body = cc
    ? `<div class="cc-bars-block">${each(s.bars, (b) => `
        <div class="cc-usage-bar"><div class="cc-bar-header"><span class="cc-bar-label">${b.label}</span><span class="cc-bar-pct ${pctClass(b.pct)}">${b.pct}%</span></div>
          <div class="cc-progress-track"><div class="cc-progress-fill ${pctClass(b.pct)}" style="width:${b.pct}%"></div></div>
          <div class="cc-reset-line"><span class="time-label">Reset </span><span class="time-val">${b.val}</span><span class="time-abs"> (${b.abs})</span></div></div>`)}</div>`
    : `<div class="ag-bars-block">${each(s.pools, agPoolRow)}</div>`
  return `
<div class="usage-column">
  <div class="column-header">
    <div class="slot-tab-group">${slotTab(s.scope === 'local', '<i class="fa-solid fa-laptop-code"></i> <span class="u-narrow-hide">LOCAL</span>')}${slotTab(s.scope === 'remote', '<i class="fa-solid fa-cloud"></i> <span class="u-narrow-hide">REMOTE</span>')}</div>
    <div class="slot-tab-group">${srcTab(s, 'ag', 'antigravity-app-icon.png', 'AG')}${srcTab(s, 'cc', 'claude-icon.png', 'CC')}${s.scope === 'remote' ? `<select class="host-select-mini"><option>${s.host}</option></select>` : ''}</div>
  </div>
  <div class="agent-usage-card">
    <div class="agent-header${cc ? ' claudecode-custom-header' : ''}">
      <div class="agent-title-group"><div class="agent-icon-wrapper${cc ? '' : ' ' + s.source}">${icon}</div>${name}</div>
      <div class="agent-status-badges"><button class="btn-ui-action btn-reload">${ring(true, 'rgba(0, 210, 255, 0.6)', 0.4)}<i class="fa-solid fa-rotate-right"></i></button></div>
    </div>
    <div class="agent-body"><div class="usage-bars-container">${body}</div></div>
  </div>
</div>`
}
const usageSection = () => `<div class="agent-usage-section">${[0, 2].map((i) => `<div class="tier-row-container"><div class="usage-split-layout">${usageSlot(M.slots[i])}${usageSlot(M.slots[i + 1])}</div></div>`).join('')}</div>`

// ProjectTable.vue (one row) with CountBadgeWrap.vue, TaskCell.vue, TerminalScopeButton.vue
const countBadge = (n, cls = '') => (n ? `<button type="button" class="cell-badge cell-badge-top sync-count-badge ${cls}">${n}</button>` : '')
const projectRow = (p) => {
  const pushClass = p.push && p.pull ? 'btn-sync-diverged' : p.push ? '' : 'btn-sync-clean'
  const pullClass = p.push && p.pull ? 'btn-sync-diverged' : p.pull ? '' : 'btn-sync-clean'
  return `
<div class="grid-row" data-project="${p.name}">
  <div class="grid-row-cell col-project-info"><div class="project-info-row">
    <div class="project-icon-col"><div class="project-drag-handle icon-glow"><i class="${p.icon} project-drag-icon-fallback" style="color:${p.color}"></i></div><i class="fa-solid fa-power-off src-power is-on"></i></div>
    <div class="project-text-col">
      <div class="project-name"><span class="project-name-label">${p.name}</span>${p.url ? '<a class="project-prod-link"><i class="fa-solid fa-globe"></i><i class="fa-solid fa-arrow-up-right-from-square project-prod-icon"></i></a>' : ''}</div>
      <div class="project-paths">
        <span class="path-local"><i class="fa-solid text-cyan mr-1 fa-laptop-code"></i> ${p.local}</span>
        <span class="path-remote"><i class="fa-solid text-amber mr-1 fa-cloud"></i><select class="host-select-mini host-select-mini--wide"><option>${p.host}</option></select><span>:${p.remote}</span></span>
      </div>
    </div>
  </div></div>
  <div class="grid-row-cell col-tasks"><div class="task-cell-wrapper"><button class="btn-cell-trigger${p.tasks.pin ? ' is-attention' : ''}"><i class="fa-solid fa-list-check"></i>${p.tasks.pin ? `<span class="cell-badge cell-badge-top task-badge-pin">${p.tasks.pin}</span>` : ''}${p.tasks.open ? `<span class="cell-badge cell-badge-bottom task-badge-normal">${p.tasks.open}</span>` : ''}</button></div></div>
  <div class="grid-row-cell col-git-status"><div class="git-cell"><div class="sync-btn-wrap">${p.git ? `<span class="cell-badge cell-badge-top sync-count-badge">${p.git}</span>` : ''}<button class="btn-cell-trigger btn-action-git${p.ahead ? ' git-ahead' : ''}"><i class="fa-brands fa-git-alt"></i></button></div></div></div>
  <div class="grid-row-cell col-terminal"><button class="btn-cell-trigger${p.term ? ' is-live' : ''}"><i class="fa-solid fa-terminal"></i>${p.term ? `<span class="cell-badge cell-badge-top term-badge-tabs">${p.term}</span>` : ''}</button></div>
  <div class="grid-row-cell col-action"><div class="actions-wrapper">
    <div class="open-popup-wrapper"><button class="btn-tech btn-tech-primary btn-action-open" style="anchor-name: --open-${p.name}"><span class="btn-text u-narrow-hide">OPEN</span> <i class="fa-solid fa-caret-up"></i></button>${openPopup(p)}</div>
    <button class="btn-tech btn-tech-secondary btn-refresh-project"><i class="fa-solid fa-arrows-rotate"></i></button>
  </div></div>
  <div class="grid-row-cell col-sync"><div class="actions-wrapper">
    <div class="sync-cluster">
      <fieldset class="remote-actions-fieldset"><div class="dry-group ${p.dry ? 'is-safe' : 'is-danger'}${p.push && p.pull ? ' is-diverged' : ''}">
        <div class="dry-group-left"><div class="sync-btn-wrap" style="anchor-name: --sync-${p.name}">${countBadge(p.push, 'sync-count-badge-clickable')}${p.mirror === 'push' ? '<i class="fa-solid fa-trash cell-badge cell-badge-bottom sync-delete-badge cell-badge-left"></i>' : ''}${p.conflicts ? `<button type="button" class="cell-badge cell-badge-top cell-badge-left sync-conflict-badge">⚠ ${p.conflicts}</button>` : ''}${p.deploy || p.hooks ? `<span class="cell-badge-cluster">${p.deploy ? '<i class="sync-deploy-badge">D</i>' : ''}${p.hooks ? '<i class="sync-hooks-badge">H</i>' : ''}</span>` : ''}
          <button class="btn-tech btn-tech-push ${pushClass}"><i class="fa-solid fa-cloud-arrow-up"></i> <span class="btn-text u-narrow-hide">PUSH</span></button></div>
          ${p.name === M.tasks.project ? syncPopup() : ''}</div>
        <div class="dry-toggle-center"><span class="dry-label">DRY</span><label class="switch switch-sm"><input type="checkbox"${p.dry ? ' checked' : ''}><span class="slider"></span></label></div>
        <div class="dry-group-right"><div class="sync-btn-wrap">${countBadge(p.pull, 'sync-count-badge-clickable')}${p.mirror === 'pull' ? '<i class="fa-solid fa-trash cell-badge cell-badge-bottom sync-delete-badge"></i>' : ''}
          <button class="btn-tech btn-tech-pull ${pullClass}"><i class="fa-solid fa-cloud-arrow-down"></i> <span class="btn-text u-narrow-hide">PULL</span></button></div></div>
      </div></fieldset>
      <div class="last-action"><div class="la-line"><span class="${p.last[0] === 'PULL' ? 'la-pull' : 'la-push'}">${p.last[0]}</span> ${p.last[1]}</div><div class="la-line la-host">${p.last[2]}</div></div>
    </div>
    <button class="btn-tech btn-tech-secondary btn-log-trigger"><i class="fa-solid fa-file-lines btn-log-icon-only"></i><span class="btn-text u-narrow-hide">LOG</span></button>
    <button class="btn-cell-trigger"><i class="fa-solid fa-gear"></i></button>
  </div></div>
</div>`
}

const popupItem = (icon, label, cls = '') => `<div class="popup-item ${cls}">${icon.startsWith('img:') ? `<img src="${PUB}/${icon.slice(4)}" class="popup-icon${label.includes('Insiders') ? ' popup-icon-insiders' : ''}">` : `<i class="${icon} popup-item-icon"></i>`} ${label}</div>`
const openPopup = (p) => `
<div class="open-popup pin-left" popover id="open-${p.name}" style="position-anchor: --open-${p.name}">
  <div class="popup-header popup-header-wrap"><i class="${p.icon} mr-1" style="color:${p.color}"></i><span class="popup-title-text">${p.name}</span><button class="popup-copy-btn"><i class="fa-solid fa-file-lines"></i> REPORT</button>${p.url ? '<button class="popup-copy-btn"><i class="fa-solid fa-magnifying-glass"></i> GSC</button>' : ''}</div>
  <div class="popup-columns">
    <div class="popup-col-local"><div class="popup-section-label"><span>💻 LOCAL</span><button class="popup-copy-btn"><i class="fa-solid fa-copy"></i> COPY</button></div>
      ${popupItem('fa-solid fa-folder-open popup-icon-amber', 'Finder')}${popupItem('fa-solid fa-terminal popup-icon-cyan', 'In-App Terminal')}${popupItem('fa-solid fa-terminal', 'Terminal')}${popupItem('img:vscode-icon.png', 'VSCode')}${popupItem('img:vscode-icon.png', 'VSCode Insiders')}${popupItem('img:antigravity-icon.png', 'Antigravity IDE')}${popupItem('fa-solid fa-code', 'Cursor')}
      <div class="popup-run-row">${popupItem('fa-solid fa-terminal popup-icon-green', 'DEV', 'popup-run-btn')}${popupItem('fa-solid fa-hammer popup-icon-amber', 'BUILD', 'popup-run-btn')}${popupItem('fa-solid fa-rocket popup-icon-cyan', 'DEPLOY', 'popup-run-btn')}</div></div>
    <div class="popup-col-remote"><div class="popup-section-label"><span>☁️ REMOTE (SSH)</span><button class="popup-copy-btn"><i class="fa-solid fa-copy"></i> COPY</button></div>
      ${popupItem('fa-solid fa-terminal popup-icon-cyan', 'SSH Terminal (In-App)')}${popupItem('fa-solid fa-terminal', 'SSH Terminal')}${popupItem('img:vscode-icon.png', 'VSCode (Remote SSH)')}${popupItem('img:vscode-icon.png', 'VSCode Insiders (Remote)')}${popupItem('img:antigravity-icon.png', 'Antigravity (Remote)')}${popupItem('fa-solid fa-code', 'Cursor (Remote SSH)')}${popupItem('fa-solid fa-upload popup-icon-cyan', 'Upload (select files)')}</div>
  </div>
</div>`

const SYNC_SIDES = [
  { key: 'push', icon: 'fa-cloud-arrow-up', label: 'PUSH', sub: 'Local → Remote' },
  { key: 'pull', icon: 'fa-cloud-arrow-down', label: 'PULL', sub: 'Remote → Local' },
]
const syncPopup = () => `
<div class="conflict-popup" popover id="sync-popup" style="position-anchor: --sync-${M.tasks.project}">
  <div class="conflict-popup-header">Sync Changes</div>
  <div class="conflict-popup-sides">${each(SYNC_SIDES, (s) => `
    <div class="conflict-popup-side is-${s.key}"><div class="conflict-popup-side-head"><i class="fa-solid ${s.icon}"></i> ${s.label} <b>${M.syncChanges[s.key].count}</b></div><div class="conflict-popup-side-sub">${s.sub}</div>
      <div class="conflict-popup-rows">${M.syncChanges[s.key].rows.length ? each(M.syncChanges[s.key].rows, ([dir, n]) => `<div class="conflict-popup-row"><span class="conflict-row-path">${dir}</span><span class="conflict-row-meta">${n}</span></div>`) : '<div class="conflict-row-empty">nothing</div>'}</div></div>`)}</div>
  ${M.syncChanges.conflicts.length ? `<div class="conflict-popup-conflicts"><div class="conflict-popup-conflicts-head">⚠ ${M.syncChanges.conflicts.length} conflict${M.syncChanges.conflicts.length === 1 ? '' : 's'} - left out of both</div><div class="conflict-popup-rows">${each(M.syncChanges.conflicts, ([path, meta]) => `<div class="conflict-popup-row"><span class="conflict-row-path">${path}</span><span class="conflict-row-meta">${meta}</span></div>`)}</div></div>` : ''}
  <div class="conflict-explain-row"><button class="btn-tech btn-tech-secondary conflict-explain-btn"><i class="fa-solid fa-wand-magic-sparkles"></i> Explain</button><button class="btn-tech btn-tech-secondary"><i class="fa-solid fa-sliders"></i></button></div>
</div>`

const projectTable = (count = M.projects.length) => `
<div class="projects-table-container"><div class="projects-grid">
  <div class="grid-header">
    <div class="grid-header-cell col-project-info col-project-info-header"><span><span class="u-narrow-hide">PROJECTS</span><span class="u-wide-hide">PJ</span> (${M.projectTotal})</span><button class="btn-tech btn-tech-primary btn-new-project-inline"><i class="fa-solid fa-plus"></i><span class="u-narrow-hide"> NEW</span></button></div>
    <div class="grid-header-cell col-tasks">TASKS</div>
    <div class="grid-header-cell col-git-status"><span class="th-with-ring"><span class="u-narrow-hide">GIT</span>${ring(false, 'rgba(16, 185, 129, 0.6)')}</span></div>
    <div class="grid-header-cell col-terminal"><button class="btn-cell-trigger is-live"><i class="fa-solid fa-terminal"></i></button></div>
    <div class="grid-header-cell col-action">ACTION</div>
    <div class="grid-header-cell col-sync"><span class="th-with-ring">SYNC ${ring(false, 'rgba(255, 140, 0, 0.6)', 0.3)}<i class="fa-solid fa-power-off src-power is-on"></i></span></div>
  </div>
  <div class="grid-body">${each(M.projects.slice(0, count), projectRow)}</div>
</div></div>`

// dock/TerminalStack.vue + TerminalTabStrip.vue + TerminalView.vue, xterm painted as plain spans in the app's terminal theme.
const TERM = { dim: '#6B7280', b: '#0088ff', o: '#ff8c00', g: '#10b981', r: '#ef4444', w: '#F3F4F6' }
const statusline = ({ agTag = false } = {}) => {
  const s = (t, c, bold) => `<span style="color:${c}${bold ? ';font-weight:700' : ''}">${esc(t)}</span>`
  const block = (html, bg) => `<span style="background:${bg}"> ${html} </span>`
  const tag = (cli, color) => `<span style="background:#EEEEEE;color:${color}">${cli}</span><span style="background:#d0d0d0;color:#7F7F7F"> team</span> `
  return tag('CC', '#FF8700') + (agTag ? tag('AG', '#0087FF') : '') +
    block(s('dev', '#FFFFFF') + s('@', '#7F7F7F') + s('devbox', '#FFFFFF'), '#000') +
    block(s('storefront', '#CD00CD'), '#262626') +
    block(s('opus', '#00CDCD') + ' ' + s('high', '#7F7F7F'), '#000') +
    block(s('ctx', '#FFFFFF') + s('88k', '#00FF00', true) + s('/1M', '#7F7F7F'), '#262626') +
    block(s('5h:', '#FFFFFF') + s('82%', '#FF8700', true) + s('1h12m', '#7F7F7F') + ' ' + s('7d:', '#FFFFFF') + s('37%', '#00FF00', true) + s('5d2h', '#7F7F7F'), '#000') +
    block(s('ss', '#FFFFFF') + s('9m', '#7F7F7F') + ' ' + s('+48', '#00FF00', true) + s('/', '#7F7F7F') + s('-4', '#FF0000', true) + ' ' + s('$1.20', '#5FFFD7', true), '#262626') +
    block(s('main', '#CD00CD'), '#000')
}
const termLine = (line) => (line === 'statusline' ? statusline() : each(line, ([k, t]) => (k === 'cursor' ? '<span class="mock-cursor"> </span>' : `<span style="color:${TERM[k] || TERM.w}">${esc(t)}</span>`)) || ' ')
const terminalStack = ({ phone = false } = {}) => `
<div class="dock-stack" style="flex:1">
  <div class="terminal-header">
    <div class="terminal-title">
      <span class="term-scope-id"><i class="fa-brands fa-vuejs" style="color:#42b883"></i><span class="term-scope-name">storefront</span></span>
      <div class="tab-group term-tab-group">
        <button class="tab term-tab is-active"><span class="tab-icon-btn icon-pin"><i class="fa-solid fa-thumbtack"></i></span><span class="tab-title">claude · devbox</span><span class="tab-icon-btn icon-close"><i class="fa-solid fa-xmark"></i></span></button>
        <button class="tab term-tab"><span class="tab-icon-btn icon-pin"><i class="fa-solid fa-thumbtack"></i></span><span class="tab-title">npm run dev</span><span class="tab-icon-btn icon-close"><i class="fa-solid fa-xmark"></i></span></button>
        <button class="tab term-tab term-tab-add"><i class="fa-solid fa-plus"></i></button>
      </div>
    </div>
    <div class="terminal-actions"><button class="btn-tech btn-tech-secondary btn-terminal-action"><i class="fa-solid fa-ellipsis-vertical"></i></button>${phone ? '<button class="btn-tech btn-tech-secondary btn-terminal-action"><i class="fa-solid fa-up-right-and-down-left-from-center"></i></button><button class="btn-tech btn-tech-secondary btn-terminal-action"><i class="fa-solid fa-chevron-down"></i></button>' : ''}</div>
  </div>
  <div class="terminal-mount-wrap"><div class="pty-terminal">
    <div class="pty-terminal-mount"><pre class="mock-xterm">${M.terminal.map(termLine).join('\n')}</pre></div>
    ${phone ? `<div class="pty-key-row">${each(['Esc', 'Tab', 'Shift', 'Ctrl'], (k) => `<button class="pty-key"><span class="pty-key-label">${k}</span></button>`)}${each(['fa-arrow-up', 'fa-arrow-down', 'fa-arrow-left', 'fa-arrow-right'], (i) => `<button class="pty-key"><i class="fa-solid ${i}"></i></button>`)}<button class="pty-key"><span class="pty-key-label">Enter</span></button></div>
      <div class="pty-compose-row"><textarea rows="1" class="pty-compose-input" placeholder="message…"></textarea><button class="pty-key pty-compose-send"><i class="fa-solid fa-paper-plane"></i></button></div>` : ''}
  </div></div>
</div>`
const logStack = () => `
<div class="dock-stack is-collapsed">
  <div class="terminal-header"><div class="terminal-title"><span><i class="fa-solid fa-book-journal-whills text-cyan mr-1"></i> GLOBAL EVENT LOG</span></div>
    <div class="terminal-actions"><button class="btn-tech btn-tech-secondary btn-terminal-action"><i class="fa-solid fa-copy"></i><span class="u-narrow-hide">COPY</span></button><button class="btn-tech btn-tech-secondary btn-terminal-action"><i class="fa-solid fa-chevron-up"></i><span class="u-narrow-hide">EXPAND</span></button></div></div>
  <div class="log-peek log-line">${M.log}</div>
</div>`

// BaseModal.vue
const modal = (title, body, { containerClass = '', headerClass = '', style = '' } = {}) => `
<div class="modal-overlay"><div class="modal-container ${containerClass}" style="${style}">
  <div class="modal-header ${headerClass}"><h2>${title}</h2><button class="btn-close-modal"><i class="fa-solid fa-xmark"></i></button></div>
  ${body}
</div></div>`

// modals/ClaudeSettingModal.vue
const check = (label, on = true, cls = 'ctl-toggle') => `<label class="${cls}"><input type="checkbox"${on ? ' checked' : ''}><span class="ctl-label">${label}</span></label>`
const swatch = (hex) => `<span class="color-pick"><button type="button" class="swatch swatch-current" style="background:${hex}"></button></span>`
const ladderDots = () => `<span class="color-dynamic">${each(['#5FFFD7', '#00FF00', '#FFFF00', '#FF8700', '#FF0000'], (h) => `<span class="tier-dot" style="background:${h}"></span>`)}</span>`
const trunc = (n) => `<label class="ctl-trunc"><i class="fa-solid fa-scissors"></i><input type="number" value="${n}"></label>`
const parts = (...items) => `<span class="ctl-parts u-narrow-hide">${each(items, (p) => `<span class="ctl-part">${p}</span>`)}</span>`
const fieldRow = (lines, note) => `<div class="row-item"><i class="fa-solid fa-grip-vertical drag-handle"></i><div class="row-lines">${note ? `<div class="row-note">${note}</div>` : ''}${each(lines, (l) => `<div class="row-line"><span class="ctl-seg">${l}</span></div>`)}</div></div>`
const customizer = () => modal(
  `<div class="modal-title-wrap"><span class="modal-title-text"><i class="fa-solid fa-terminal"></i> Statusline Customizer</span><div class="header-target-selector"><span class="target-label">Apply to:</span><label class="target-check-item"><input type="checkbox" checked><span>agy</span></label><label class="target-check-item"><input type="checkbox" checked><span>claude</span></label></div></div>`,
  `<div class="modal-body setting-body">
    <div class="preview-box"><pre class="preview-line">${statusline({ agTag: true })}</pre></div>
    <div class="section-label fields-header"><span>Fields <span class="hint">(drag the grip to reorder)</span></span><button type="button" class="toggle-all-btn">Check all</button></div>
    <div class="row-item pinned-row"><i class="fa-solid fa-thumbtack pin-icon"></i>${check('CLI tag')}${check('Account')}${trunc(4)}</div>
    <div class="field-list">
      ${fieldRow([`${check('id', true, 'ctl-toggle ctl-master')}</span><span class="ctl-seg tight">${check('user')}${swatch('#FFFFFF')}${trunc(5)}<span class="ctl-part">@</span>${check('host')}${swatch('#FFFFFF')}${trunc(6)}`])}
      ${fieldRow([`${check('Working directory')}${swatch('#CD00CD')}${trunc(12)}`])}
      ${fieldRow([`${check('model')}${swatch('#00CDCD')}${check('effort')}`])}
      ${fieldRow([`${check('Context window')}${ladderDots()}${parts('token in+out', '/Max Token')}`])}
      ${fieldRow([`${check('5h')}${parts('%')}${ladderDots()}<span class="ctl-spacer"></span>${check('Reset ETA')}`, `${check('7d')}${parts('%')}${ladderDots()}<span class="ctl-spacer"></span>${check('Reset ETA')}`], 'AGY: pool follows the selected model')}
      ${fieldRow([`${check('ss')}${parts('duration', 'Line +/-', '$')}${ladderDots()}`])}
      ${fieldRow([`${check('Git branch')}${swatch('#CD00CD')}${trunc(10)}`])}
    </div>
    <div class="section-label">Dynamic color <span class="hint">(each band starts at the % below it)</span></div>
    <div class="ladder-bar">${each([['#5FFFD7', 0, 20], ['#00FF00', 20, 51], ['#FFFF00', 51, 75], ['#FF8700', 75, 90], ['#FF0000', 90, 100]], ([h, from, to], i) => `<div class="ladder-cell" style="flex-grow:${to - from}"><span class="ladder-seg" style="background:${h}"></span>${i ? `<input type="number" value="${from}">` : '<span class="ladder-zero">0</span>'}</div>`)}</div>
    <div class="section-label">Target Hosts <span class="hint">(tag per CLI found: filled = statusline live)</span></div>
    <div class="host-list">${each([['Local', 1, ['CC', 'AG']], ['devbox', 1, ['CC', 'AG']], ['gpu-box', 1, ['AG']], ['staging', 0, []]], ([h, on, tags]) => `<label class="host-chip${on ? ' active' : ''}"><input type="checkbox"${on ? ' checked' : ''}> ${h} ${each(tags, (t) => `<span class="cli-tag on">${t}</span>`)}</label>`)}</div>
  </div>
  <div class="modal-footer modal-footer-form"><button class="btn-modal-action btn-reset"><i class="fa-solid fa-arrow-rotate-left"></i> Reset</button><button class="btn-modal-action btn-apply"><i class="fa-solid fa-paper-plane"></i> Apply to 3 hosts</button></div>`,
  { style: 'width: 90vw; max-width: 1100px;' })

// modals/ProjectConfigModal.vue (the sync-rule half)
const EXCLUDES = '.DS_Store\nnode_modules/\n.nuxt/\n.output/\n.env*\n*.log'
const config = () => modal(
  `<i class="fa-solid fa-gear mr-1"></i> Configuration: storefront`,
  `<div class="modal-body scrollable">
    <div class="full-width config-group commands-group mb-1 mt-1">
      <h4 class="group-title commands-title"><i class="fa-solid fa-terminal mr-1"></i> RUN COMMANDS <span class="owner-badge owner-project">in the project</span></h4>
      <div class="commands-row">
        <div class="form-group"><label class="text-green-dim">DEV <span class="default-hint">npm run dev</span></label><input type="text" class="code-input" value="npm run dev"></div>
        <div class="form-group"><label class="text-amber-dim">BUILD <span class="default-hint">npm run build</span></label><input type="text" class="code-input" value="npm run build"></div>
        <div class="form-group"><label class="text-cyan-dim">DEPLOY <span class="default-hint">npm run deploy</span></label><input type="text" class="code-input" value="npm run deploy"></div>
      </div>
      <div class="deploy-target-row">
        <div class="form-group"><label class="text-cyan-dim">Deploy Runs On</label><select><option>Remote (SSH)</option></select></div>
        <div class="form-group"><label class="text-cyan-dim">Deploy Host</label><select><option>devbox</option></select></div>
        <div class="form-group checkbox-inline"><input type="checkbox" id="d" checked><label for="d">Offer Deploy after a successful PUSH to the deploy host</label></div>
      </div>
    </div>
    <div class="full-width config-group mb-1 mt-1 dashed-group"><h4 class="group-title text-muted dashed-group-title"><i class="fa-solid fa-layer-group mr-1"></i> EXCLUDE PRESETS</h4>
      <div class="row-gap-8"><button class="btn-secondary btn-compact">Nuxt 4</button><button class="btn-secondary btn-compact">Tauri v2 (Rust)</button><button class="btn-secondary btn-compact">Aki Default</button></div></div>
    <div class="excludes-split full-width mt-1">
      <div class="config-group push-group"><h4 class="group-title text-push"><i class="fa-solid fa-arrow-up mr-1"></i> PUSH (Local → Remote)</h4><div class="form-group mb-1"><label class="text-push">Excludes (1 per line)</label><textarea class="large-textarea border-push" rows="5">${EXCLUDES}</textarea></div>
        <div class="form-group"><div class="scripts-toggle"><label class="text-push" style="font-size:11px;font-weight:800"><i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts <i class="fa-solid fa-chevron-down chevron-gap"></i></label></div></div></div>
      <div class="config-group pull-group"><h4 class="group-title text-pull"><i class="fa-solid fa-arrow-down mr-1"></i> PULL (Remote → Local)</h4><div class="form-group mb-1"><label class="text-pull">Excludes (1 per line)</label><textarea class="large-textarea border-pull" rows="5">${EXCLUDES}</textarea></div>
        <div class="form-group"><div class="scripts-toggle"><label class="text-muted" style="font-size:11px;font-weight:600"><i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts <i class="fa-solid fa-chevron-down chevron-gap"></i></label></div></div></div>
    </div>
  </div>`)

// modals/GitModal.vue
const GIT_COLOR = { g: 'var(--accent-green, #10b981)', r: 'var(--accent-red, #ef4444)' }
const git = () => modal(
  `<div class="modal-title-row"><i class="fa-brands fa-vuejs modal-title-glyph" style="color:#42b883"></i><span>Git Version Control: storefront</span></div>`,
  `<div class="modal-body scrollable">
    <div class="form-group full-width mb-2"><label>Remote Git URL</label><a class="git-url-link"><i class="fa-brands fa-git-alt mr-1"></i>${M.git.url}</a></div>
    <div class="git-actions-panel mb-3"><div class="form-group full-width"><label>Commit Message</label><div class="commit-input-wrapper"><input type="text" class="large-input" value="fix(cart): round the total once, after every discount code"><button class="btn-tech btn-tech-primary"><i class="fa-solid fa-check"></i> COMMIT</button></div></div></div>
    <div class="form-group full-width mt-3"><label class="git-status-header"><span><i class="fa-solid fa-clock-rotate-left mr-1"></i> Git Status History</span><button class="btn-refresh-status"><i class="fa-solid fa-arrows-rotate"></i></button></label>
      <pre class="git-status-log">${each(M.git.log, ([k, t]) => (k ? `<span style="color: ${GIT_COLOR[k]};">${esc(t)}</span>` : esc(t)))}</pre></div>
  </div>
  <div class="modal-footer"><button class="btn-tech btn-tech-secondary">CLOSE</button><button class="btn-tech btn-tech-secondary"><i class="fa-solid fa-scroll"></i> <span class="u-narrow-hide">CHANGELOG</span></button><button class="btn-tech btn-tech-secondary"><i class="fa-solid fa-download"></i> <span class="u-narrow-hide">FETCH</span></button><button class="btn-tech btn-tech-secondary"><i class="fa-solid fa-cloud-arrow-down"></i> <span class="u-narrow-hide">PULL</span></button><button class="btn-tech btn-tech-secondary"><i class="fa-solid fa-upload"></i> <span class="u-narrow-hide">PUSH</span></button></div>`)

// modals/ProjectTasksModal.vue + tasks/NotesField.vue + tasks/TaskListPanel.vue
const tasks = () => {
  const items = M.tasks.items
  const n = (k) => items.filter((t) => (k === 'done' ? t.done : k === 'pin' ? t.pin && !t.done : !t.pin && !t.done)).length
  return modal(
    `<div class="tasks-modal-header-title"><i class="fa-brands fa-vuejs mr-1" style="color:#42b883"></i><span>Tasks: ${M.tasks.project}</span></div>`,
    `<div class="modal-body scrollable">
      <div class="project-notes-section mb-3"><div class="notes-header"><span class="notes-title"><i class="fa-regular fa-note-sticky mr-1"></i> Project Notes</span></div><textarea class="project-notes-textarea" rows="2">${M.tasks.notes}</textarea></div>
      <div class="task-list-panel">
        <div class="tasks-summary-bar mb-3"><div class="tasks-summary"><span class="stat s-doing">${n('pin')} pinned</span><span class="stat s-todo">${n('todo')} active</span><span class="stat s-done">${n('done')} done</span></div><label class="toggle-hide-done"><input type="checkbox"><span>Hide Completed</span></label></div>
        <div class="task-add-row mb-3"><input type="text" class="task-add-input" placeholder="Add a new task..."><button class="btn-tech btn-tech-primary task-add-btn"><i class="fa-solid fa-plus"></i> Add</button></div>
        <div class="task-list-wrapper"><div class="task-list">${each(items, (t) => `
          <div class="task-item-row${t.done ? ' is-done' : ''}${t.detail ? ' has-detail' : ''}">
            <div class="task-states-left"><button class="task-state-icon-btn pin-btn${t.pin ? ' is-active' : ''}"><i class="fa-solid fa-thumbtack"></i></button><button class="task-state-icon-btn wish-btn${t.wish ? ' is-active' : ''}"><i class="fa-regular fa-clock"></i></button></div>
            <div class="task-info"><input type="text" class="task-title-input" value="${esc(t.title)}"${t.done ? ' disabled' : ''}><textarea class="task-detail-textarea" rows="1" placeholder="Add detail description..."${t.done ? ' disabled' : ''}>${t.detail || ''}</textarea></div>
            <span class="task-time">${t.time}</span>
            <button class="task-check-btn${t.done ? ' is-completed' : ''}"><i class="${t.done ? 'fa-solid fa-circle-check' : 'fa-regular fa-circle'}"></i></button>
            <button class="task-copy-btn"><i class="fa-regular fa-copy"></i></button>
            <button class="task-del-btn"><i class="fa-solid fa-trash-can"></i></button>
          </div>`)}</div></div>
      </div>
    </div>
    <div class="modal-footer"><button class="btn-secondary">Close</button></div>`,
    { containerClass: 'tasks-modal', headerClass: 'header-tasks' })
}

// App.vue: the window around everything.
const app = ({ dock, rows, phone, menu } = {}) => `
<div class="dashboard-layout">
  ${header({ menu })}
  <div class="dashboard-main${dock === 'right' ? ' is-right-dock' : ''}">
    <div class="dashboard-left">${usageSection()}${projectTable(rows)}${dock === 'right' ? logStack() : ''}</div>
    ${dock ? `<div class="dashboard-bottom"${dock === 'bottom' ? ' style="height:40%"' : ''}>${terminalStack({ phone })}${dock === 'bottom' ? logStack() : ''}</div>` : ''}
  </div>
</div>`

const VIEWS = {
  main: () => app({ rows: 7 }),
  menu: () => app({ rows: 4, menu: true }),
  wide: () => app({ dock: 'right', rows: 7 }),
  phone: () => app({ dock: 'bottom', rows: 7, phone: true }),
  sync: () => app({ rows: 7 }),
  open: () => app({ rows: 7 }),
  customizer,
  config,
  git,
  tasks,
}
document.body.classList.add(`mock-${view}`)
document.body.innerHTML = VIEWS[view]()
if (view === 'sync') document.getElementById('sync-popup').showPopover()
if (view === 'open') document.getElementById('open-api-server').showPopover()
