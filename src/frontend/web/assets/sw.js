// VellumFE service worker: app-shell only, network-first.
// Only the static shell is ever cached — never /ws (not interceptable
// anyway), never /health, never anything cross-origin. Network-first so a
// rebuilt binary always wins when the PC is reachable; the cache exists
// so the installed app still paints its shell while reconnecting.

const CACHE = "vellum-shell-v1";
const SHELL = ["/", "/app.js", "/app.css", "/manifest.webmanifest", "/icon.svg"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "GET" || url.origin !== location.origin) return;
  if (!SHELL.includes(url.pathname)) return;
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        const copy = response.clone();
        caches.open(CACHE).then((cache) => cache.put(event.request, copy));
        return response;
      })
      .catch(() => caches.match(event.request))
  );
});
