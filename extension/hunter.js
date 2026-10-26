// Hydra DL — avcı modu içerik betiği.
// Sayfadaki öğeyi fareyle seçip indirme panelini açar. Tüm arayüz Shadow DOM içindedir.
(() => {
  "use strict";

  if (window.__hydraHunter) return;
  window.__hydraHunter = true;

  const HOST_ID = "hydra-hunter-host";
  const TARGET_SELECTOR = "video, audio, source, img, a[href], iframe, embed, object";
  // Üst çubuk yalnız üst frame'de görünür; iframe'lerde yalnız seçim arayüzü çalışır.
  const IS_TOP_FRAME = (() => {
    try {
      return window.top === window;
    } catch (_err) {
      return false;
    }
  })();
  const ALLOWED_SCHEME = /^(https?|ftp|magnet):/i;

  const SHADOW_CSS = `
    :host { all: initial; }
    * { box-sizing: border-box; }
    .bar {
      position: fixed;
      top: 18px;
      left: 50%;
      transform: translateX(-50%);
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 9px 16px;
      border-radius: 999px;
      background: rgba(7, 12, 21, 0.94);
      border: 1px solid rgba(34, 211, 238, 0.45);
      box-shadow: 0 12px 34px rgba(0, 0, 0, 0.55);
      color: #e7eefb;
      font: 500 13px/1.35 "Segoe UI", system-ui, sans-serif;
      pointer-events: none;
      backdrop-filter: blur(10px);
    }
    .bar .dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: linear-gradient(135deg, #22d3ee, #8b5cf6);
      box-shadow: 0 0 10px rgba(34, 211, 238, 0.85);
    }
    .bar .hint { color: #94a3b8; font-weight: 400; }
    .frame {
      position: fixed;
      border: 2px solid #22d3ee;
      border-radius: 4px;
      background: rgba(34, 211, 238, 0.12);
      box-shadow: 0 0 0 1px rgba(7, 12, 21, 0.75), 0 0 22px rgba(34, 211, 238, 0.35);
      pointer-events: none;
      display: none;
    }
    .tag {
      position: fixed;
      max-width: 420px;
      padding: 3px 8px;
      border-radius: 5px;
      background: linear-gradient(135deg, #22d3ee, #8b5cf6);
      color: #04121a;
      font: 700 11px/1.45 "Segoe UI", system-ui, sans-serif;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      pointer-events: none;
      display: none;
    }
    .toast {
      position: fixed;
      left: 50%;
      bottom: 28px;
      transform: translateX(-50%);
      max-width: min(520px, 88vw);
      padding: 11px 15px;
      border-radius: 10px;
      background: #101a2a;
      border: 1px solid rgba(248, 113, 113, 0.55);
      color: #fecaca;
      font: 500 12.5px/1.5 "Segoe UI", system-ui, sans-serif;
      box-shadow: 0 14px 36px rgba(0, 0, 0, 0.5);
      pointer-events: none;
      display: none;
    }
    .toast.warn {
      border-color: rgba(34, 211, 238, 0.5);
      color: #cffafe;
    }
  `;

  const state = {
    active: false,
    host: null,
    shadow: null,
    bar: null,
    frame: null,
    tag: null,
    toast: null,
    toastTimer: 0,
    hoverEl: null,
    rafId: 0,
  };

  function sendMessage(message) {
    return new Promise((resolve) => {
      try {
        chrome.runtime.sendMessage(message, (response) => {
          void chrome.runtime.lastError;
          resolve(response || null);
        });
      } catch (_err) {
        resolve(null);
      }
    });
  }

  function applyShadowStyles(shadow) {
    try {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(SHADOW_CSS);
      shadow.adoptedStyleSheets = [sheet];
      return;
    } catch (_err) {
      // eski tarayıcı: <style> ekle
    }
    const style = document.createElement("style");
    style.textContent = SHADOW_CSS;
    shadow.appendChild(style);
  }

  function ensureUi() {
    if (state.host && state.host.isConnected) return;
    const host = document.createElement("div");
    host.id = HOST_ID;
    const shadow = host.attachShadow({ mode: "open" });
    applyShadowStyles(shadow);

    const bar = document.createElement("div");
    bar.className = "bar";
    const dot = document.createElement("span");
    dot.className = "dot";
    const text = document.createElement("span");
    text.textContent = "Avcı modu — indirilecek öğeye tıkla";
    const hint = document.createElement("span");
    hint.className = "hint";
    hint.textContent = "· Esc: çık";
    bar.append(dot, text, hint);

    const frame = document.createElement("div");
    frame.className = "frame";
    const tag = document.createElement("div");
    tag.className = "tag";
    const toast = document.createElement("div");
    toast.className = "toast";

    bar.style.display = state.active ? "flex" : "none";
    // iframe'lerde üst çubuk gösterilmez (sayfa başına bir tane).
    shadow.append(...(IS_TOP_FRAME ? [bar] : []), frame, tag, toast);
    (document.body || document.documentElement).appendChild(host);

    state.host = host;
    state.shadow = shadow;
    state.bar = IS_TOP_FRAME ? bar : null;
    state.frame = frame;
    state.tag = tag;
    state.toast = toast;
  }

  function showToast(message, tone) {
    ensureUi();
    state.toast.textContent = message;
    state.toast.className = tone === "warn" ? "toast warn" : "toast";
    state.toast.style.display = "block";
    clearTimeout(state.toastTimer);
    state.toastTimer = setTimeout(() => {
      if (state.toast) state.toast.style.display = "none";
      if (!state.active) deactivate();
    }, 4200);
  }

  function tagNameFor(element) {
    const tag = element.tagName.toLowerCase();
    if (tag === "video") return "video";
    if (tag === "audio" || tag === "source") {
      return element.closest("audio") ? "ses" : "video";
    }
    if (tag === "img") return "resim";
    if (tag === "a") return "bağlantı";
    return "medya";
  }

  function resolveUrl(element) {
    const tag = element.tagName.toLowerCase();
    const attr = (name) => element.getAttribute(name) || "";

    if (tag === "video" || tag === "audio") {
      const source = element.querySelector("source[src]");
      return (
        element.currentSrc ||
        (source && (source.src || source.getAttribute("src"))) ||
        element.src ||
        attr("data-src") ||
        attr("poster") ||
        ""
      );
    }
    if (tag === "source") return element.src || attr("src") || attr("data-src") || "";
    if (tag === "img") {
      return (
        element.currentSrc ||
        element.src ||
        attr("data-src") ||
        attr("data-original") ||
        attr("data-lazy-src") ||
        ""
      );
    }
    if (tag === "a") return element.href || attr("href") || "";
    if (tag === "iframe") return element.src || attr("data-src") || "";
    if (tag === "embed") return element.src || attr("src") || "";
    if (tag === "object") return attr("data") || "";
    return "";
  }

  function pickElement(x, y) {
    const element = document.elementFromPoint(x, y);
    if (!element || !element.closest) return null;
    if (element.closest(`#${HOST_ID}`)) return null;
    return element.closest(TARGET_SELECTOR);
  }

  function paintFrame(element) {
    if (!element || !state.frame) {
      if (state.frame) state.frame.style.display = "none";
      if (state.tag) state.tag.style.display = "none";
      return;
    }
    const rect = element.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) {
      state.frame.style.display = "none";
      state.tag.style.display = "none";
      return;
    }
    const frame = state.frame;
    frame.style.display = "block";
    frame.style.left = `${Math.max(0, rect.left)}px`;
    frame.style.top = `${Math.max(0, rect.top)}px`;
    frame.style.width = `${Math.max(0, rect.width)}px`;
    frame.style.height = `${Math.max(0, rect.height)}px`;

    const url = resolveUrl(element);
    const label = `${tagNameFor(element)} · ${url ? url.slice(0, 90) : "adres yok"}`;
    const tag = state.tag;
    tag.textContent = label;
    tag.style.display = "block";
    const tagTop = rect.top > 26 ? rect.top - 24 : rect.top;
    tag.style.left = `${Math.max(4, Math.min(rect.left, window.innerWidth - 240))}px`;
    tag.style.top = `${Math.max(4, tagTop)}px`;
  }

  function onMouseMove(event) {
    if (!state.active) return;
    if (state.rafId) return;
    const { clientX, clientY } = event;
    state.rafId = requestAnimationFrame(() => {
      state.rafId = 0;
      if (!state.active) return;
      const element = pickElement(clientX, clientY);
      state.hoverEl = element;
      paintFrame(element);
    });
  }

  function onScroll() {
    if (!state.active) return;
    paintFrame(state.hoverEl);
  }

  function swallow(event) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
  }

  function pageReferer() {
    if (IS_TOP_FRAME) return location.href;
    // iframe içinden seçim: kaynak sayfa üst belgenin adresidir.
    return document.referrer || location.href;
  }

  async function handlePick(element) {
    const url = resolveUrl(element);
    if (!url) {
      showToast("Bu öğede indirilebilir bir adres bulunamadı.", "warn");
      return;
    }
    if (/^data:/i.test(url)) {
      showToast("data: adresleri doğrudan indirilemez, bu öğe atlandı.", "warn");
      return;
    }
    // blob: adresi sayfaya aittir, sunucu doğrudan indiremez. Bu durumda
    // sayfa adresini gönderiyoruz: Hydra sayfayı tarayıp gerçek kaynağı (ya da
    // yt-dlp ile en iyi kaliteyi) bulur. Kullanıcıya ne olduğunu söyle.
    const isBlob = /^blob:/i.test(url);
    if (!isBlob && !ALLOWED_SCHEME.test(url)) {
      showToast("Bu bağlantı türü desteklenmiyor (yalnız http/https/ftp/magnet).", "warn");
      return;
    }
    const pageUrl = pageReferer() || location.href;
    const targetUrl = isBlob ? pageUrl : url;

    const payload = {
      type: "hunter-pick",
      url: targetUrl,
      referer: pageUrl,
      pageTitle: document.title,
      fromBlob: isBlob,
    };

    deactivate();
    const response = await sendMessage(payload);
    await sendMessage({ type: "hunter-toggle", enabled: false });
    if (response && response.ok === false) {
      showToast(`Hydra DL: ${response.error || "eklenemedi"}`, "warn");
      return;
    }
    // Uyarı deactivate() ile silinmesin: blob seçiminden sonra sayfada tekrar göster.
    if (isBlob) {
      showToast("blob: medya doğrudan alınamıyor; sayfa adresi gönderildi.", "warn");
    }
  }

  function onClickCapture(event) {
    if (!state.active) return;
    const element = pickElement(event.clientX, event.clientY);
    if (!element) return;
    swallow(event);
    void handlePick(element);
  }

  function onMouseDownCapture(event) {
    if (!state.active) return;
    const element = pickElement(event.clientX, event.clientY);
    if (!element) return;
    swallow(event);
  }

  function onKeyDown(event) {
    if (event.key === "Escape" && state.active) {
      swallow(event);
      deactivate();
      void sendMessage({ type: "hunter-toggle", enabled: false });
      return;
    }
    if (event.altKey && event.shiftKey && (event.code === "KeyD" || event.key === "D")) {
      swallow(event);
      void toggle();
    }
  }

  function attach() {
    window.addEventListener("mousemove", onMouseMove, true);
    window.addEventListener("mousedown", onMouseDownCapture, true);
    window.addEventListener("click", onClickCapture, true);
    window.addEventListener("scroll", onScroll, true);
  }

  function detach() {
    window.removeEventListener("mousemove", onMouseMove, true);
    window.removeEventListener("mousedown", onMouseDownCapture, true);
    window.removeEventListener("click", onClickCapture, true);
    window.removeEventListener("scroll", onScroll, true);
  }

  // Kısayol ve Esc dinleyicisi kalıcıdır: avcı modu kapalıyken de Alt+Shift+D çalışır.
  window.addEventListener("keydown", onKeyDown, true);

  async function activate() {
    if (state.active) return true;
    const health = await sendMessage({ type: "hydra-health" });
    if (!health || health.ok !== true) {
      // Uygulama kapalı: sessizce bekle, ayar açık kalsın.
      return false;
    }
    ensureUi();
    state.active = true;
    state.hoverEl = null;
    attach();
    if (state.bar) state.bar.style.display = "flex";
    return true;
  }

  function deactivate() {
    if (!state.active && !state.host) return;
    state.active = false;
    detach();
    if (state.rafId) {
      cancelAnimationFrame(state.rafId);
      state.rafId = 0;
    }
    clearTimeout(state.toastTimer);
    if (state.host && state.host.parentNode) state.host.parentNode.removeChild(state.host);
    state.host = null;
    state.shadow = null;
    state.bar = null;
    state.frame = null;
    state.tag = null;
    state.toast = null;
    state.hoverEl = null;
  }

  async function toggle() {
    if (state.active) {
      deactivate();
      await sendMessage({ type: "hunter-toggle", enabled: false });
      return;
    }
    const response = await sendMessage({ type: "hunter-toggle", enabled: true });
    if (!response || !response.ok) {
      showToast("Hydra DL'ye ulaşılamadı, avcı modu açılamadı.", "warn");
      return;
    }
    const started = await activate();
    if (!started) showToast("Hydra DL açık değil; uygulamayı başlatıp tekrar deneyin.", "warn");
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || message.type !== "hunter-set") return false;
    if (message.enabled === true) {
      void activate().then((started) => sendResponse({ ok: true, active: started }));
      return true;
    }
    deactivate();
    sendResponse({ ok: true, active: false });
    return false;
  });

  (async () => {
    try {
      const stored = await chrome.storage.sync.get({ hunterEnabled: false });
      if (stored.hunterEnabled === true) await activate();
    } catch (_err) {
      // sessiz
    }
  })();
})();
