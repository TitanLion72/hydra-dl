# ui-harness — masaüstü arayüzü QA koşumu

Bu dizin, `src/` altındaki masaüstü arayüzünü **Tauri olmadan** gerçek tarayıcıda
çalıştırıp doğrular. Vite'ta arayüz zaten tarayıcıda açılır; tek engel `src/api.ts`
içindeki Tauri `invoke` çağrılarıdır. Playwright ile `/src/api.ts` isteğini
`mock-api.mjs` kaynağıyla karşılarız, arayüz de kendi sahte API'siyle konuşur.

Kapsam:

1. Ana ekran: başlık, boş durum.
2. İnceleme paneli: `#probe-btn` → format satırları, radyo seçimi, özet, uyarılar.
3. Format değiştir + İndir → `api.add` çağrısında **doğru `url` ve `format`**.
4. `Esc` ile panel kapanışı.
5. Ayarlar çekmecesi: alanların mock ayarıyla uyumu, `#save-settings` → `saveSettings`.
6. İndirme listesi: 3 kayıt (indiriliyor / tamamlandı / hata) ve durum rozetleri.
7. Konsol + `pageerror` hataları sıfır olmalı.

Her adım `artifacts/` altına PNG bırakır (`01-empty`, `02-probe`, `03-settings`, `04-list`).

## Koşum

```powershell
cd tools\ui-harness
npm.cmd install          # sadece bu dizinde; playwright devDependency'si
npm.cmd run test
```

- Tarayıcı indirilmez: sistemdeki Chrome (`channel: "chrome"`) kullanılır, yoksa Edge denenir.
- Vite dev sunucusu **port 1420**'de koşumun kendisi başlatır ve çıkışta öldürür.
  Port doluysa yanlış sunucuya bağlanmamak için koşum hemen durur.
- Çıkış kodu 0 = PASS, 1 = FAIL. FAIL çıktısında her `FAIL` satırı ve Vite log kuyruğu yazdırılır.

## Notlar

- `mock-api.mjs` gerçek `src/api.ts` tipleriyle birebir uyumlu olmalı; alan eklenirse
  (`Download`, `Settings`, `ProbeResult` ...) buradaki sahte veriler de güncellenmeli.
- Çağrı kayıtları `window.__hydraCalls`, veriler `window.__hydraMock` üzerinde durur;
  `window.__hydraMock.pushList()` 3 kaydı `onDownloads` olayıyla arayüze iter.
- Bu takım arayüzü **düzeltmez**; bulunan gerçek hatalar raporlanır (dosya sahipliği `src/`).
