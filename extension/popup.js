// Hydra DL — küçük açılır panel.
"use strict";

const H = window.HydraShared;

const el = {
  dot: document.getElementById("dot"),
  statusText: document.getElementById("status-text"),
  pageUrl: document.getElementById("page-url"),
  inspect: document.getElementById("inspect"),
  manualUrl: document.getElementById("manual-url"),
  addManual: document.getElementById("add-manual"),
  hunter: document.getElementById("hunter"),
  showPanel: document.getElementById("show-panel"),
  port: document.getElementById("port"),
  testPort: document.getElementById("test-port"),
  portResult: document.getElementById("port-result"),
  message: document.getElementById("message"),
  optionsLink: document.getElementById("options-link"),
  version: document.getElementById("version"),
};

let currentTab = null;
let settings = { ...H.DEFAULT_SETTINGS };

function send(message) {
  try {
    return Promise.resolve(HX.api.runtime.sendMessage(message)).catch(() => null);
  } catch (_err) {
    return Promise.resolve(null);
  }
}

function say(text, tone) {
  el.message.textContent = text;
  el.message.className = "message" + (tone ? ` ${tone}` : "");
  if (text) {
    setTimeout(() => {
      if (el.message.textContent === text) say("");
    }, 4200);
  }
}

function setStatus(online) {
  el.dot.className = online ? "dot on" : "dot off";
  el.statusText.textContent = online ? "hazır" : "uygulama kapalı";
}

async function refreshHealth() {
  const response = await send({ type: "hydra-health" });
  setStatus(Boolean(response && response.ok));
  return Boolean(response && response.ok);
}

function openPanel(url, referer, title) {
  const params = new URLSearchParams({ url });
  if (referer) params.set("referer", referer);
  if (title) params.set("title", title);
  return HX.api.tabs.create({
    url: HX.api.runtime.getURL(`panel.html?${params.toString()}`),
  });
}

async function loadTab() {
  try {
    const [tab] = await HX.api.tabs.query({ active: true, currentWindow: true });
    currentTab = tab || null;
    const url = tab?.url || "";
    el.pageUrl.textContent = url || "Bu sekmede adres yok.";
    el.inspect.disabled = !/^https?:/i.test(url);
  } catch (_err) {
    el.pageUrl.textContent = "Sekme okunamadı.";
    el.inspect.disabled = true;
  }
}

async function loadSettings() {
  settings = await H.getSettings();
  el.hunter.checked = settings.hunterEnabled;
  el.showPanel.checked = settings.showPanel;
  el.port.value = String(settings.port);
}

async function applyPort() {
  const port = Number(el.port.value);
  if (!Number.isInteger(port) || port < H.PORT_MIN || port > H.PORT_MAX) {
    el.portResult.textContent = "geçersiz port";
    el.portResult.style.color = "#f87171";
    return false;
  }
  settings = await H.saveSettings({ port });
  return true;
}

el.inspect.addEventListener("click", async () => {
  const url = currentTab?.url || "";
  if (!/^https?:/i.test(url)) {
    say("Bu sekmede indirilebilir bir adres yok.", "err");
    return;
  }
  await openPanel(url, url, currentTab?.title);
  window.close();
});

el.addManual.addEventListener("click", async () => {
  const url = el.manualUrl.value.trim();
  if (!url) return;
  if (!/^(https?|ftp|magnet):/i.test(url)) {
    say("Bu bir bağlantı adresi değil.", "err");
    return;
  }
  if (settings.showPanel) {
    await openPanel(url, currentTab?.url, currentTab?.title);
    window.close();
    return;
  }
  const response = await send({ type: "hydra-add", url, referer: currentTab?.url });
  if (response && response.ok) {
    say("Kuyruğa eklendi.", "ok");
    el.manualUrl.value = "";
  } else {
    say((response && response.error) || "Eklenemedi, uygulama açık mı?", "err");
    setStatus(false);
  }
});

el.manualUrl.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void el.addManual.click();
  }
});

el.hunter.addEventListener("change", async () => {
  settings = await H.saveSettings({ hunterEnabled: el.hunter.checked });
  say(el.hunter.checked ? "Avcı modu açık — sayfada Alt+Shift+D." : "Avcı modu kapalı.", "");
});

el.showPanel.addEventListener("change", async () => {
  settings = await H.saveSettings({ showPanel: el.showPanel.checked });
  say(
    el.showPanel.checked
      ? "İndirmelerde seçenek paneli açılacak."
      : "İndirmeler doğrudan kuyruğa eklenecek.",
    "",
  );
});

el.testPort.addEventListener("click", async () => {
  el.portResult.textContent = "kontrol…";
  el.portResult.style.color = "";
  const valid = await applyPort();
  if (!valid) return;
  const online = await refreshHealth();
  if (online) {
    el.portResult.textContent = "bağlı";
    el.portResult.style.color = "#4ade80";
    say("Bağlantı kuruldu.", "ok");
  } else {
    el.portResult.textContent = "ulaşılamadı";
    el.portResult.style.color = "#f87171";
    say("Hydra DL yanıt vermiyor. Uygulama açık mı?", "err");
  }
});

el.optionsLink.addEventListener("click", () => HX.api.runtime.openOptionsPage());

HX.api.storage.onChanged.addListener((changes, area) => {
  if (area !== "sync") return;
  if (changes.hunterEnabled) el.hunter.checked = changes.hunterEnabled.newValue === true;
  if (changes.showPanel) el.showPanel.checked = changes.showPanel.newValue !== false;
  if (changes.port) el.port.value = String(changes.port.newValue);
});

async function init() {
  el.version.textContent = `v${HX.api.runtime.getManifest().version}`;
  await loadSettings();
  await loadTab();
  const online = await refreshHealth();
  if (!online) say("Hydra DL kapalı görünüyor; uygulamayı açıp tekrar dene.", "");
}

void init();
