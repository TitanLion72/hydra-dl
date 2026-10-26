// Hydra DL — ayarlar sayfası.
"use strict";

const H = window.HydraShared;

const el = {
  port: document.getElementById("port"),
  showPanel: document.getElementById("show-panel"),
  hunter: document.getElementById("hunter"),
  ask: document.getElementById("ask"),
  save: document.getElementById("save"),
  test: document.getElementById("test"),
  result: document.getElementById("result"),
  version: document.getElementById("version"),
  health: document.getElementById("health"),
};

async function load() {
  const settings = await H.getSettings();
  el.port.value = String(settings.port);
  el.showPanel.checked = settings.showPanel;
  el.hunter.checked = settings.hunterEnabled;
  el.ask.checked = settings.askOnDownload;
}

function readForm() {
  const port = Number(el.port.value);
  if (!Number.isInteger(port) || port < H.PORT_MIN || port > H.PORT_MAX) {
    el.result.textContent = `Port ${H.PORT_MIN}–${H.PORT_MAX} arasında olmalı.`;
    el.result.style.color = "#f87171";
    return null;
  }
  return {
    port,
    showPanel: el.showPanel.checked,
    hunterEnabled: el.hunter.checked,
    askOnDownload: el.ask.checked,
  };
}

async function testConnection() {
  el.result.textContent = "kontrol ediliyor…";
  el.result.style.color = "#8ea1ba";
  try {
    const data = await H.hydraHealth();
    el.result.textContent = `Bağlı — ${data.app || "hydra-dl"} v${data.version || "?"} · ${
      data.downloads ?? 0
    } indirme, ${data.active ?? 0} aktif`;
    el.result.style.color = "#4ade80";
    el.health.textContent = "sunucu çalışıyor";
    el.health.style.color = "#4ade80";
    return true;
  } catch (err) {
    el.result.textContent = `Ulaşılamadı: ${err.message}`;
    el.result.style.color = "#f87171";
    el.health.textContent = "sunucu kapalı";
    el.health.style.color = "#f87171";
    return false;
  }
}

el.save.addEventListener("click", async () => {
  const patch = readForm();
  if (!patch) return;
  await H.saveSettings(patch);
  el.result.textContent = "Ayarlar kaydedildi.";
  el.result.style.color = "#4ade80";
  await testConnection();
});

el.test.addEventListener("click", async () => {
  const patch = readForm();
  if (!patch) return;
  // Test etmeden önce portu kaydet ki eklentinin tamamı aynı portu kullansın.
  await H.saveSettings(patch);
  await testConnection();
});

el.port.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void el.save.click();
  }
});

async function init() {
  el.version.textContent = `Hydra DL eklentisi v${chrome.runtime.getManifest().version}`;
  await load();
  void testConnection();
}

void init();
