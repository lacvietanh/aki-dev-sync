// Invented data for the mock screens: one story told everywhere. A developer codes on a Mac; Claude Code and Antigravity work on the SSH box `devbox` (and `gpu-box` for the ML project). Nothing here belongs to a real person or project.
window.MOCK = {
  hosts: ['devbox', 'gpu-box', 'staging'],

  // Two per row, in the order AgentUsageSection lays them out.
  slots: [
    { scope: 'local', agent: 'cc', name: 'Claude Code', plan: 'Max', account: 'me',
      bars: [{ label: '5-HOUR', pct: 46, val: '2h41m', abs: '16:00 Oct22' }, { label: '7-DAY', pct: 63, val: '3d9h', abs: '22:00 Oct25' }] },
    { scope: 'remote', host: 'devbox', agent: 'cc', name: 'Claude Code', plan: 'Pro', account: 'team', note: 'live',
      bars: [{ label: '5-HOUR', pct: 82, val: '1h12m', abs: '14:30 Oct22' }, { label: '7-DAY', pct: 37, val: '5d2h', abs: '15:00 Oct27' }] },
    { scope: 'local', agent: 'ag', source: 'ide', plan: 'Pro', account: 'me',
      pools: [
        { label: 'Argon 4', windows: [{ label: '7D', pct: 0, val: 'ready' }] },
        { label: 'Gemini', windows: [{ label: '5H', pct: 12, val: '3h05m' }, { label: '7D', pct: 34, val: '4d6h' }] },
        { label: 'Claude/OSS', windows: [{ label: '5H', pct: 0, val: 'ready' }, { label: '7D', pct: 9, val: '6d1h' }] },
      ] },
    { scope: 'remote', host: 'gpu-box', agent: 'ag', source: 'cli', account: 'lab',
      pools: [
        { label: 'Argon 4', windows: [{ label: '7D', pct: 0, val: 'ready', disabled: true }] },
        { label: 'Gemini', windows: [{ label: '5H', pct: 100, val: '47m' }, { label: '7D', pct: 71, val: '2d3h' }] },
        { label: 'Claude/OSS', windows: [{ label: '5H', pct: 23, val: '4h10m' }, { label: '7D', pct: 18, val: '5d20h' }] },
      ] },
  ],

  // The header count; the table shows only the first rows of a longer list.
  projectTotal: 29,

  // icon: a Font Awesome glyph standing in for the project's favicon.
  projects: [
    { name: 'storefront', icon: 'fa-brands fa-vuejs', color: '#42b883', url: true, local: '/Users/dev/code/storefront', host: 'devbox', remote: '~/code/storefront',
      tasks: { pin: 2, open: 3 }, git: 4, term: 2, pull: 12, conflicts: 1, last: ['PULL', '2m ago', 'devbox'] },
    { name: 'api-server', icon: 'fa-brands fa-node-js', color: '#5fa04e', url: true, local: '/Users/dev/code/api-server', host: 'devbox', remote: '~/code/api-server',
      tasks: { pin: 1, open: 1 }, git: 0, ahead: true, push: 3, deploy: true, last: ['PUSH', '1h ago', 'devbox'] },
    { name: 'ml-pipeline', icon: 'fa-brands fa-python', color: '#4b8bbe', local: '/Users/dev/code/ml-pipeline', host: 'gpu-box', remote: '~/work/ml-pipeline',
      tasks: { pin: 0, open: 4 }, git: 7, term: 1, push: 2, pull: 5, conflicts: 1, last: ['PULL', '25m ago', 'gpu-box'] },
    { name: 'desktop-app', icon: 'fa-brands fa-rust', color: '#f74c00', local: '/Users/dev/code/desktop-app', host: 'devbox', remote: '~/code/desktop-app',
      tasks: { pin: 1, open: 0 }, git: 0, hooks: true, last: ['PUSH', '3h ago', 'devbox'] },
    { name: 'landing-page', icon: 'fa-brands fa-html5', color: '#e34f26', url: true, local: '/Users/dev/code/landing-page', host: 'staging', remote: '/var/www/landing',
      tasks: { pin: 0, open: 2 }, git: 1, dry: true, mirror: 'push', last: ['PUSH', '1d ago', 'staging'] },
    { name: 'docs-site', icon: 'fa-brands fa-markdown', color: '#cbd5e1', url: true, local: '/Users/dev/code/docs-site', host: 'devbox', remote: '~/code/docs-site',
      tasks: { pin: 0, open: 0 }, git: 0, last: ['PULL', '2d ago', 'devbox'] },
    { name: 'mobile-app', icon: 'fa-brands fa-react', color: '#61dafb', local: '/Users/dev/code/mobile-app', host: 'devbox', remote: '~/code/mobile-app',
      tasks: { pin: 0, open: 1 }, git: 2, push: 6, last: ['PUSH', '4d ago', 'devbox'] },
  ],

  // Remote Control addresses in the app menu: what get_companion_url() lists, on the fixed relay port 1421.
  remoteUrls: [['lan', 'http://192.168.1.24:1421'], ['tailscale', 'http://100.101.7.42:1421']],

  // What the Sync Changes popover lists for storefront: the agent's work on devbox, waiting to come home, and one file edited on both sides.
  syncChanges: {
    push: { count: 0, rows: [] },
    pull: { count: 12, rows: [['src/components/cart/', 5], ['src/composables/', 3], ['server/api/checkout/', 2], ['tests/cart/', 2]] },
    conflicts: [['src/locales/vi.json', '1840B vs 1912B']],
  },

  tasks: {
    project: 'storefront',
    notes: 'Staging: https://staging.storefront.dev · deploy from devbox after PULL + review',
    items: [
      { title: 'Checkout: total rounds wrong with 3+ discount codes', detail: 'Claude Code on devbox is on it, PULL when done', pin: true, time: '12m' },
      { title: 'Cart drawer flickers on slow 3G', detail: 'repro with throttling, Safari only', pin: true, time: '2h' },
      { title: 'Move product images to the CDN bucket', time: '1d' },
      { title: 'Add e2e test for guest checkout', wish: true, time: '3d' },
      { title: 'Translate checkout errors (vi, ja)', time: '4d' },
      { title: 'Upgrade to Nuxt 4', done: true, time: '6d' },
    ],
  },

  git: {
    url: 'https://github.com/studio/storefront',
    log: [
      ['', 'On branch '], ['g', 'main'], ['', '\nYour branch is up to date with \'origin/main\'.\n\nChanges not staged for commit:\n'],
      ['r', '\tmodified:   src/components/cart/CartTotal.vue\n\tmodified:   src/composables/useDiscounts.ts\n\tmodified:   server/api/checkout/total.post.ts\n'],
      ['', '\nUntracked files:\n'], ['r', '\ttests/cart/discount-stack.spec.ts\n'],
    ],
  },

  // The in-app terminal: an SSH tab on devbox running Claude Code, ending on the statusline this app designs.
  terminal: [
    [['dim', 'dev@devbox'], ['', ':'], ['b', '~/code/storefront'], ['', '$ claude']],
    [],
    [['o', '> '], ['', 'checkout total rounds wrong when 3+ discount codes stack. fix it, add a test']],
    [],
    [['g', '⏺ '], ['', 'I\'ll trace how discounts are applied to the cart total.']],
    [],
    [['g', '⏺ '], ['w', 'Read'], ['', '(src/composables/useDiscounts.ts)']],
    [['dim', '  ⎿  Read 142 lines']],
    [['g', '⏺ '], ['', 'Each code rounds the subtotal before the next one applies, so']],
    [['', '  three codes compound the rounding error. Rounding once at the end fixes it.']],
    [],
    [['g', '⏺ '], ['w', 'Update'], ['', '(src/composables/useDiscounts.ts)']],
    [['dim', '  ⎿  Updated with '], ['g', '9 additions'], ['dim', ' and '], ['r', '4 removals']],
    [['g', '⏺ '], ['w', 'Write'], ['', '(tests/cart/discount-stack.spec.ts)']],
    [['dim', '  ⎿  Wrote 48 lines']],
    [['g', '⏺ '], ['w', 'Bash'], ['', '(npm run test -- cart)']],
    [['dim', '  ⎿  '], ['g', '✓ 14 passed'], ['dim', ' (1.8s)']],
    [],
    [['g', '⏺ '], ['', 'Fixed. Totals now round once, after every code is applied. 12 files changed.']],
    [],
    [['dim', '────────────────────────────────────────────────────────────────────────────────────────────']],
    [['o', '> '], ['cursor', ' ']],
    [['dim', '────────────────────────────────────────────────────────────────────────────────────────────']],
    'statusline',
  ],

  log: '[1:18:07 PM] [SYNC] storefront: 12 file(s) to PULL from devbox',
}
