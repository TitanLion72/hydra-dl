// Hydra DL — arka plan service worker: sağ tık menüleri, panel açma, mesaj yönlendirme.
"use strict";

importScripts("compat.js", "shared.js", "netlog.js");

const PANEL_PAGE = "panel.html";
const MENU_DOWNLOAD = "hydra-download";
const MENU_INSPECT = "hydra-inspect";
const MENU_HUNTER = "hydra-hunter";

async function createMenus() {
  const settings = await getSettings();
  try {
    await HX.api.contextMenus.removeAll();
  } catch (_err) {
    // zaten yok
  }
  const create = (props) =>
    Promise.resolve(HX.api.contextMenus.create(props)).catch((err) =>
      console.warn("[hydra] menü oluşturulamadı", props.id, err?.message ?? err),
    );
  await create({
    id: MENU_DOWNLOAD,
    title: "Hydra DL ile indir",
    contexts: ["link", "video", "audio", "image"],
  });
  // "Bu sayfayı incele" paneli açar; panel kapalıysa menüde de görünmesin.
  if (settings.showPanel) {
    await create({
      id: MENU_INSPECT,
      title: "Bu sayfayı incele",
      contexts: ["page"],
    });
  }
  await create({
    id: MENU_HUNTER,
    title: "Avcı modunu aç/kapat",
    contexts: ["page", "link", "video", "audio", "image", "selection"],
  });
}

HX.api.runtime.onInstalled.addListener(() => void createMenus());
HX.api.runtime.onStartup.addListener(() => void createMenus());

function panelUrl(targetUrl, referer, title, tabId) {
  const params = new URLSearchParams({ url: targetUrl });
  if (referer) params.set("referer", referer);
  if (title) params.set("title", title);
  if (Number.isInteger(tabId)) params.set("tab", String(tabId));
  return HX.api.runtime.getURL(`${PANEL_PAGE}?${params.toString()}`);
}

const PANEL_SCHEMES = /^(https?|ftp|magnet|blob):/i;

async function openPanel(targetUrl, referer, title, tabId) {
  if (!PANEL_SCHEMES.test(targetUrl)) {
    notify("Hydra DL", "Bu adres indirilebilir bir bağlantı değil.");
    return { ok: false, error: "geçersiz adres" };
  }
  await HX.api.tabs.create({ url: panelUrl(targetUrl, referer, title, tabId) });
  return { ok: true, panel: true };
}

async function addDirect(payload) {
  try {
    const data = await hydraAdd(payload);
    notify("Hydra DL", `${data.filename || "İndirme"} kuyruğa eklendi.`);
    return { ok: true, data };
  } catch (err) {
    notify(
      "Hydra DL'ye ulaşılamadı",
      `${err.message}. Uygulama açık mı? Yakalama sunucusu çalışıyor mu?`,
    );
    return { ok: false, error: err.message, code: err.code || "network" };
  }
}

// showPanel açıksa IDM tarzı seçenek ekranı, kapalıysa doğrudan kuyruğa.
async function handleDownloadRequest(payload) {
  const settings = await getSettings();
  if (settings.showPanel) {
    return openPanel(payload.url, payload.referer, payload.title, payload.tabId);
  }
  return addDirect({
    url: payload.url,
    format: payload.format,
    filename: payload.filename,
    dir: payload.dir,
    referer: payload.referer,
    media: payload.media,
  });
}

async function broadcastHunter(enabled) {
  let tabs = [];
  try {
    tabs = await HX.api.tabs.query({});
  } catch (_err) {
    return;
  }
  await Promise.all(
    tabs.map(async (tab) => {
      if (!tab.id || !tab.url || !/^https?:/i.test(tab.url)) return;
      try {
        await HX.api.tabs.sendMessage(tab.id, { type: "hunter-set", enabled });
      } catch (_err) {
        // içerik betiği olmayan sekmeler: yut
      }
    }),
  );
}

async function toggleHunter(force) {
  const settings = await getSettings();
  const enabled = typeof force === "boolean" ? force : !settings.hunterEnabled;
  await saveSettings({ hunterEnabled: enabled });
  await broadcastHunter(enabled);
  return enabled;
}

HX.api.contextMenus.onClicked.addListener((info, tab) => {
  const pageUrl = info.pageUrl || tab?.url || "";
  const target = info.linkUrl || info.srcUrl || info.frameUrl || pageUrl;

  if (info.menuItemId === MENU_DOWNLOAD) {
    if (!target) return;
    void handleDownloadRequest({
      url: target,
      referer: pageUrl,
      title: tab?.title,
      tabId: tab?.id,
    });
    return;
  }
  if (info.menuItemId === MENU_INSPECT) {
    if (!pageUrl) return;
    const inspectPage = async () => {
      const settings = await getSettings();
      if (settings.showPanel) {
        await openPanel(pageUrl, pageUrl, tab?.title, tab?.id);
        return;
      }
      await addDirect({ url: pageUrl, referer: pageUrl });
    };
    void inspectPage();
    return;
  }
  if (info.menuItemId === MENU_HUNTER) {
    void toggleHunter();
  }
});

HX.api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message || typeof message.type !== "string") return false;

  switch (message.type) {
    case "hydra-health":
      hydraHealth()
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err) => sendResponse({ ok: false, error: err.message, code: err.code }));
      return true;

    case "hydra-probe":
      if (typeof message.url !== "string") {
        sendResponse({ ok: false, error: "url gerekli" });
        return false;
      }
      hydraProbe(message.url, message.media)
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err) => sendResponse({ ok: false, error: err.message, code: err.code }));
      return true;

    case "hydra-add":
      if (typeof message.url !== "string") {
        sendResponse({ ok: false, error: "url gerekli" });
        return false;
      }
      hydraAdd({
        url: message.url,
        format: message.format,
        filename: message.filename,
        dir: message.dir,
        referer: message.referer || sender?.tab?.url,
        media: message.media,
      })
        .then((data) => {
          notify("Hydra DL", `${data.filename || "İndirme"} kuyruğa eklendi.`);
          sendResponse({ ok: true, data });
        })
        .catch((err) => sendResponse({ ok: false, error: err.message, code: err.code }));
      return true;

    case "hunter-toggle":
      toggleHunter(typeof message.enabled === "boolean" ? message.enabled : undefined)
        .then((enabled) => sendResponse({ ok: true, enabled }))
        .catch((err) => sendResponse({ ok: false, error: err.message }));
      return true;

    case "hunter-pick":
      if (typeof message.url !== "string") {
        sendResponse({ ok: false, error: "url gerekli" });
        return false;
      }
      handleDownloadRequest({
        url: message.url,
        referer: message.referer || sender?.tab?.url,
        title: message.pageTitle,
        tabId: sender?.tab?.id,
      })
        .then(sendResponse)
        .catch((err) => sendResponse({ ok: false, error: err.message }));
      return true;

    case "net-media": {
      const tabId = Number(message.tabId);
      if (!Number.isInteger(tabId)) {
        sendResponse({ ok: false, error: "tabId gerekli" });
        return false;
      }
      Promise.all([readNetEntries(tabId), getSettings()])
        .then(([entries, settings]) =>
          sendResponse({ ok: true, entries, enabled: settings.netCapture === true }),
        )
        .catch((err) => sendResponse({ ok: false, error: err.message }));
      return true;
    }

    case "net-clear": {
      const tabId = Number(message.tabId);
      if (!Number.isInteger(tabId)) {
        sendResponse({ ok: false, error: "tabId gerekli" });
        return false;
      }
      writeNetEntries(tabId, [])
        .then(() => {
          try {
            void HX.api.action.setBadgeText({ tabId, text: "" });
          } catch (_err) {
            // yut
          }
          sendResponse({ ok: true });
        })
        .catch((err) => sendResponse({ ok: false, error: err.message }));
      return true;
    }

    case "net-toggle":
      saveSettings({ netCapture: message.enabled === true })
        .then(async (settings) => {
          await syncNetCapture(settings.netCapture === true);
          sendResponse({ ok: true, enabled: settings.netCapture === true });
        })
        .catch((err) => sendResponse({ ok: false, error: err.message }));
      return true;

    default:
      return false;
  }
});

HX.api.storage.onChanged.addListener((changes, area) => {
  if (area !== "sync") return;
  if (changes.showPanel) void createMenus();
  if (changes.hunterEnabled) void broadcastHunter(changes.hunterEnabled.newValue === true);
  if (changes.netCapture) void syncNetCapture(changes.netCapture.newValue === true);
});

// ---------------------------------------------------------------------------
// Ağdan medya yakalama (IDM tarzı)
// ---------------------------------------------------------------------------
//
// Tarayıcının gerçekten istediği medya adreslerini görürüz; böylece medyayı
// sayfada göstermeyen sitelerde de (JS ile `m3u8`/`mpd` çekenler) indirme
// mümkün olur. **Varsayılan kapalı**; yalnız medya istekleri saklanır.
// Kayıtlar `storage.session` içinde sekme başına tutulur (worker uyusa da yaşar).

const NET_KEY_PREFIX = "net:";

function netKey(tabId) {
  return `${NET_KEY_PREFIX}${tabId}`;
}

async function readNetEntries(tabId) {
  try {
    const key = netKey(tabId);
    const stored = await HX.api.storage.session.get(key);
    const list = stored && stored[key];
    return Array.isArray(list) ? list : [];
  } catch (_err) {
    return [];
  }
}

async function writeNetEntries(tabId, entries) {
  try {
    await HX.api.storage.session.set({ [netKey(tabId)]: entries });
  } catch (_err) {
    // storage.session yoksa (eski tarayıcı) yakalama sessizce devre dışı kalır.
  }
}

/**
 * Sekme başına yazma sırası.
 *
 * Sayfa aynı anda birden çok medya isteği atar; her istek "oku → ekle → yaz"
 * yaparsa son yazan diğerlerini **ezer** (ilk testte m3u8/mpd kayboluyordu).
 * Bu yüzden sekmeye özel bir promise zinciri kullanıyoruz.
 */
const netQueues = new Map();

function enqueueNet(tabId, task) {
  const previous = netQueues.get(tabId) || Promise.resolve();
  const next = previous.then(task).catch(() => {});
  netQueues.set(tabId, next);
  // Kuyruk boşalınca haritayı temizle (bellek sızmasın).
  void next.then(() => {
    if (netQueues.get(tabId) === next) netQueues.delete(tabId);
  });
  return next;
}

async function recordMediaRequest(details, contentType) {
  const tabId = details.tabId;
  if (!Number.isInteger(tabId) || tabId < 0) return;
  const entry = self.HydraNetlog.classifyMediaRequest(
    details.url,
    contentType || "",
    details.type,
  );
  if (!entry) return;
  await enqueueNet(tabId, async () => {
    const entries = await readNetEntries(tabId);
    const next = self.HydraNetlog.pushNetEntry(entries, {
      url: details.url,
      kind: entry.kind,
      ext: entry.ext,
      size: Number(details.contentLength) || 0,
      at: Date.now(),
    });
    await writeNetEntries(tabId, next);
    // Sekme rozetinde yakalanan sayısını göster (kullanıcı fark etsin).
    try {
      await HX.api.action.setBadgeText({ tabId, text: String(next.length) });
    } catch (_err) {
      // rozet desteklenmiyorsa yut
    }
  });
}

/**
 * Yanıt başlıklarından yakalama: uzantısız CDN adreslerinde (`/stream/abc`)
 * medya türü yalnız `content-type` ile anlaşılır. Yalnız bu tür yanıtlara bakılır.
 */
function onMediaResponse(details) {
  const headers = Array.isArray(details.responseHeaders) ? details.responseHeaders : [];
  let contentType = "";
  let contentLength = 0;
  for (const header of headers) {
    const name = String(header.name || "").toLowerCase();
    if (name === "content-type") contentType = String(header.value || "");
    else if (name === "content-length") contentLength = Number(header.value) || 0;
  }
  if (!contentType) return;
  void recordMediaRequest({ ...details, contentLength }, contentType);
}

let netCaptureActive = false;

function registerNetCapture() {
  if (netCaptureActive) return;
  const runtime = HX.api.webRequest || globalThis.chrome?.webRequest;
  if (!runtime || !runtime.onBeforeRequest) return;
  const filter = {
    urls: ["http://*/*", "https://*/*"],
    types: ["media", "xmlhttprequest", "other", "object"],
  };
  runtime.onBeforeRequest.addListener(recordMediaRequest, filter);
  // Uzantısız CDN adresleri: yalnız başlıklara bak (gövde okunmaz).
  if (runtime.onHeadersReceived) {
    runtime.onHeadersReceived.addListener(onMediaResponse, filter, ["responseHeaders"]);
  }
  netCaptureActive = true;
}

function unregisterNetCapture() {
  if (!netCaptureActive) return;
  const runtime = HX.api.webRequest || globalThis.chrome?.webRequest;
  if (!runtime) {
    netCaptureActive = false;
    return;
  }
  // Kaldırma her tarayıcıda sessizce başarısız olabilir; ayrı ayrı deniyoruz.
  try {
    runtime.onBeforeRequest?.removeListener?.(recordMediaRequest);
  } catch (_err) {
    // yut
  }
  try {
    runtime.onHeadersReceived?.removeListener?.(onMediaResponse);
  } catch (_err) {
    // yut
  }
  netCaptureActive = false;
}

/** Ayar değişince dinleyiciyi aç/kapat ve eski kayıtları temizle. */
async function syncNetCapture(enabled) {
  if (enabled) {
    registerNetCapture();
    return;
  }
  unregisterNetCapture();
  try {
    const all = await HX.api.storage.session.get(null);
    const keys = Object.keys(all || {}).filter((key) => key.startsWith(NET_KEY_PREFIX));
    if (keys.length > 0) await HX.api.storage.session.remove(keys);
  } catch (_err) {
    // yut
  }
  try {
    const tabs = await HX.api.tabs.query({});
    await Promise.all(
      tabs.map((tab) =>
        tab.id ? HX.api.action.setBadgeText({ tabId: tab.id, text: "" }).catch(() => {}) : null,
      ),
    );
  } catch (_err) {
    // yut
  }
}

/** Açılışta ayarı uygula (worker her uyandığında da çağrılır). */
void (async () => {
  const settings = await getSettings();
  if (settings.netCapture === true) registerNetCapture();
})();

// Sekme yeniden yüklenince o sekmenin kayıtlarını sıfırla.
HX.api.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (!changeInfo || changeInfo.status !== "loading") return;
  void (async () => {
    await writeNetEntries(tabId, []);
    try {
      await HX.api.action.setBadgeText({ tabId, text: "" });
    } catch (_err) {
      // yut
    }
  })();
});

HX.api.tabs.onRemoved.addListener((tabId) => {
  void writeNetEntries(tabId, []);
});
