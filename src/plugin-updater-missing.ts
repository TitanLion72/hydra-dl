// Yer tutucu: `@tauri-apps/plugin-updater` paketi henüz kurulmadı (Task 2).
//
// `vite.config.ts`, paket kurulu değilken bu dosyaya alias verir; böylece
// paket gelmeden de `npm run build` yeşil kalır. `check()` her zaman `null`
// döner: güncelleme şeridi hiç görünmez.
//
// **Paket kurulunca bu dosya silinebilir** — alias zaten devre dışı kalır.
// Tip importu yok (alias kendine dönmesin diye): yalnız çalışma anı yüzeyi.

export function check(): Promise<null> {
  return Promise.resolve(null);
}
