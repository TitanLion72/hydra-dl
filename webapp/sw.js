/* Hydra DL — app shell service worker.
 * Kural: API istekleri (health, downloads, events, probe, add, settings, torrents)
 * ASLA önbelleğe alınmaz; yalnızca arayüz kabuğu önbellekte tutulur.
 */

const CACHE = "hydra-shell-v1";
const SHELL = [
  "./",
  "./index.html",
  "./app.js",
  "./styles.css",
  "./manifest.webmanifest",
  "./icon.svg",
];

const API_ROOTS = [
  "/health",
  "/downloads",
  "/events",
  "/probe",
  "/add",
  "/settings",
  "/torrents",
];

function isApiRequest(url, request) {
  if (request.headers.get("accept") === "text/event-stream") return true;
  if (url.pathname.endsWith("/events")) return true;
  return API_ROOTS.some(
    (root) => url.pathname === root || url.pathname.endsWith(root) || url.pathname.includes(`${root}/`),
  );
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      await Promise.allSettled(SHELL.map((path) => cache.add(new Request(path, { cache: "reload" }))));
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key)));
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (isApiRequest(url, request)) return; // ağa bırak, önbelleğe dokunma

  if (request.mode === "navigate") {
    event.respondWith(
      (async () => {
        try {
          const fresh = await fetch(request);
          const cache = await caches.open(CACHE);
          cache.put("./index.html", fresh.clone());
          return fresh;
        } catch {
          const cache = await caches.open(CACHE);
          const cached = (await cache.match("./index.html")) || (await cache.match("./"));
          return (
            cached ||
            new Response("<h1>Çevrimdışı</h1>", {
              status: 503,
              headers: { "content-type": "text/html; charset=utf-8" },
            })
          );
        }
      })(),
    );
    return;
  }

  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      const cached = await cache.match(request);
      const network = fetch(request)
        .then((response) => {
          if (response && response.ok) cache.put(request, response.clone());
          return response;
        })
        .catch(() => null);
      if (cached) return cached;
      const response = await network;
      return response || new Response("", { status: 504 });
    })(),
  );
});
