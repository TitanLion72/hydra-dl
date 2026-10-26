// Hydra DL — arka plan service worker: sağ tık menüleri, panel açma, mesaj yönlendirme.
"use strict";

importScripts("compat.js", "shared.js");

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

function panelUrl(targetUrl, referer, title) {
  const params = new URLSearchParams({ url: targetUrl });
  if (referer) params.set("referer", referer);
  if (title) params.set("title", title);
  return HX.api.runtime.getURL(`${PANEL_PAGE}?${params.toString()}`);
}

const PANEL_SCHEMES = /^(https?|ftp|magnet|blob):/i;

async function openPanel(targetUrl, referer, title) {
  if (!PANEL_SCHEMES.test(targetUrl)) {
    notify("Hydra DL", "Bu adres indirilebilir bir bağlantı değil.");
    return { ok: false, error: "geçersiz adres" };
  }
  await HX.api.tabs.create({ url: panelUrl(targetUrl, referer, title) });
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
    return openPanel(payload.url, payload.referer, payload.title);
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
    void handleDownloadRequest({ url: target, referer: pageUrl, title: tab?.title });
    return;
  }
  if (info.menuItemId === MENU_INSPECT) {
    if (!pageUrl) return;
    const inspectPage = async () => {
      const settings = await getSettings();
      if (settings.showPanel) {
        await openPanel(pageUrl, pageUrl, tab?.title);
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
      })
        .then(sendResponse)
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
});
