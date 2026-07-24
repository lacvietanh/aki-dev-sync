// Minimal service worker — the smallest thing that makes the companion an installable PWA.
//
// This is a LAN tool that is useless without the live Mac, so the worker deliberately caches
// NOTHING: no offline shell, no stale-content risk after the app updates. Its only job is to exist
// with a `fetch` handler (an installability signal) and pass every request straight to the network.
//
// It only ever runs in a secure context (HTTPS / localhost). Over plain http on a LAN IP the browser
// hides `navigator.serviceWorker` entirely, so `main.js`'s guarded register is a no-op there — the
// standalone behaviour on that path comes from the manifest + apple-* meta tags, not this worker.
self.addEventListener('install', () => self.skipWaiting())
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()))
self.addEventListener('fetch', (event) => {
  event.respondWith(
    fetch(event.request).catch(() => new Response('', { status: 504, statusText: 'offline' }))
  )
})
