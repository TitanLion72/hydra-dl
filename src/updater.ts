// Güncelleme çalışma zamanı — `@tauri-apps/plugin-updater` sarmalayıcısı.
//
// Kararlar `updater-logic.ts`'te (saf, testli); burada yalnız eklenti, ağ ve
// arayüz bağlantısı var. Tauri dışında (düz tarayıcı, UI harness) eklenti
// yüklenemez ve her çağrı sessizce döner: tarayıcı asla patlamamalı.

import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { api } from "./api";
import {
  clearPending,
  installLooksFailed,
  percent,
  readPending,
  readSkipped,
  shouldPrompt,
  writePending,
  writeSkipped,
  type Storage,
} from "./updater-logic";

/** Açılıştan sonra ilk kontrol gecikmesi (ms). Harness aynı değeri bekler. */
export const STARTUP_CHECK_DELAY_MS = 3000;

/** Kısa, kullanıcıya gösterilecek mesajlar (i18n anahtarı olarak da geçer). */
const DOWNLOAD_FAILED = "Güncelleme indirilemedi — internet bağlantını kontrol et";

/** Arayüzün güncelleme şeridi için bağladığı geri çağrılar. */
export type UpdaterHooks = {
  onAvailable(version: string, notes: string): void;
  onProgress(percent: number | null): void;
  onInstalling(): void;
  onError(message: string): void;
  onFailedBefore(pendingVersion: string): void;
};

/**
 * `localStorage` sarmalayıcısı: gizli sekmede ya da kapalı depolamada
 * erişim hata atabilir; güncelleme yüzünden uygulama düşmesin.
 */
const storage: Storage = {
  getItem(key) {
    try {
      return window.localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  setItem(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch {
      // tercih kaydedilemedi; akış yine çalışır
    }
  },
  removeItem(key) {
    try {
      window.localStorage.removeItem(key);
    } catch {
      // sessiz
    }
  },
};

/** Son kontrolün bulduğu güncelleme; "Güncelle" düğmesi bunu kullanır. */
let available: Update | null = null;
/** Son kontrolün bağladığı arayüz; ilerleme/hata buraya gider. */
let hooks: UpdaterHooks | null = null;

/** `Update` kaynağını sessizce bırakır. */
async function closeQuietly(update: Update): Promise<void> {
  try {
    await update.close();
  } catch {
    // kaynak zaten bırakılmış olabilir
  }
}

/**
 * Çalışan uygulamanın sürümü; Tauri dışında `null`.
 *
 * `getVersion` yerine uygulamanın kendi `appInfo` komutu: aynı sürümü verir,
 * ayrıca tarayıcıda (harness) hata atmadan taklit edilebilir.
 */
async function currentVersion(): Promise<string | null> {
  try {
    const info = await api.appInfo();
    return info.version || null;
  } catch {
    return null;
  }
}

/**
 * Sessiz güncelleme kontrolü (açılışta).
 *
 * Yeni sürüm yoksa ya da kullanıcı bu sürüm için "Sonra" dediyse hiçbir şey
 * yapmaz. Ağ hatasında da sessiz kalır: açılış kontrolü kullanıcıyı
 * rahatsız etmemeli.
 */
export async function checkForUpdate(consumer: UpdaterHooks): Promise<void> {
  hooks = consumer;

  let update: Update | null;
  try {
    update = await check();
  } catch {
    return;
  }
  if (!update) {
    available = null;
    return;
  }
  if (!shouldPrompt(update.version, readSkipped(storage))) {
    await closeQuietly(update);
    available = null;
    return;
  }

  available = update;
  consumer.onAvailable(update.version, update.body ?? "");
}

/**
 * Şeritteki "Güncelle": indirir ve kurar.
 *
 * Kurulumdan **önce** `pending` işareti yazılır; uygulama yeniden açıldığında
 * `reportPendingFailure` işarete bakıp yükseltmenin gerçekleşip
 * gerçekleşmediğini anlar.
 */
export async function installUpdate(): Promise<void> {
  if (!available) {
    // Önceki oturumdan kalan "tamamlanmadı" işareti olabilir: önce yeniden bak.
    if (!hooks) return;
    await checkForUpdate(hooks);
  }
  const update = available;
  if (!update) return;

  const consumer = hooks;
  writePending(storage, update.version);

  let downloaded = 0;
  let total: number | null = null;
  const onEvent = (event: DownloadEvent) => {
    if (event.event === "Started") {
      total = typeof event.data.contentLength === "number" ? event.data.contentLength : null;
      consumer?.onProgress(percent(downloaded, total));
      return;
    }
    if (event.event === "Progress") {
      // Toplam boyut yalnız `Started` olayında gelir; yoksa yüzde bilinmez.
      downloaded += event.data.chunkLength;
      consumer?.onProgress(percent(downloaded, total));
      return;
    }
    // Finished: indirme bitti, kurulum başlıyor (uygulama yeniden açılır).
    consumer?.onProgress(percent(downloaded, total));
    consumer?.onInstalling();
  };

  try {
    consumer?.onProgress(null);
    await update.downloadAndInstall(onEvent);
  } catch {
    // İndirme/kurulum tamamlanmadı: kullanıcıya hemen söyle, işareti temizle
    // ki bir sonraki açılışta aynı hata ikinci kez gösterilmesin.
    clearPending(storage);
    consumer?.onError(DOWNLOAD_FAILED);
  } finally {
    await closeQuietly(update);
  }
}

/** "Sonra": bu sürüm için bir daha sorulmasın. */
export function skipVersion(version: string): void {
  writeSkipped(storage, version);
}

/**
 * Açılışta yarım kalan kurulumu bildirir.
 *
 * İşaret varsa ve çalışan sürüm hâlâ işaretli sürüm değilse yükseltme
 * gerçekleşmemiştir → şerit "Güncelleme tamamlanmadı" der. Sürüm işaretle
 * eşleşiyorsa yükseltme başarılıdır → işaret silinir.
 */
export function reportPendingFailure(consumer: UpdaterHooks): void {
  const pending = readPending(storage);
  if (!pending) return;

  hooks = consumer;
  void (async () => {
    const version = await currentVersion();
    // Sürüm okunamıyorsa (tarayıcı) karar veremeyiz; işareti bırak.
    if (version === null) return;
    if (installLooksFailed(pending, version)) consumer.onFailedBefore(pending);
    else clearPending(storage);
  })().catch(() => {
    // sessiz: açılış akışını asla düşürme
  });
}
