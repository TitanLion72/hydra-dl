// Tarayıcı uyumluluğu.
//
// Chrome/Edge `chrome.*` API'sini sunar; Firefox ayrıca promise tabanlı
// `browser.*` sunar. Firefox, `chrome.*` uyumluluk katmanını da verdiği için
// "hangisi var" sorusu tek başına yetmez: Edge/Chromium de `browser` global'ini
// kısmen tanımlıyor. Bu yüzden Firefox'u **`browser.runtime.getBrowserInfo`**
// varlığıyla tespit ediyoruz (yalnız Firefox'ta bulunur).
//
// Kullanım: `HX.api.storage.sync.get(...)`. Bu dosya her bağlamda **ilk**
// yüklenmelidir (background: importScripts, content script: js[0], HTML: ilk
// <script>).
(function () {
  const chromeApi = globalThis.chrome;
  const browserApi = globalThis.browser;
  const native = browserApi ?? chromeApi;
  if (!native) {
    // Sayfa bağlamında API yoksa sessizce çık.
    return;
  }

  // `browser.runtime.getBrowserInfo` yalnız Firefox'ta vardır.
  const isFirefox = Boolean(browserApi && browserApi.runtime && browserApi.runtime.getBrowserInfo);

  // Promise döndüren API: Firefox'ta browser.*, diğerlerinde chrome.*
  // (Chrome MV3 çoğu metodu promise olarak da destekler).
  const api = isFirefox ? browserApi : chromeApi;

  globalThis.HX = { api, isFirefox, chrome: chromeApi, browser: browserApi };
})();
