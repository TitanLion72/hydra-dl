// Güncelleme kararı mantığı — saf, yan etkisiz, bağımlılıksız.
//
// Neden ayrı dosya: kararlar (şerit gösterilsin mi, kurulum başarısız mı,
// yüzde kaç) ağdan ve arayüzden bağımsız test edilebilsin diye. `tools/
// test-updater.mjs` bu dosyayı node'un tip sıyırmasıyla doğrudan yükler;
// bu yüzden burada **hiç import yok**.

/** Depolama soyutlaması: tarayıcıda `localStorage` uyar. */
export type Storage = {
  getItem(k: string): string | null;
  setItem(k: string, v: string): void;
  removeItem(k: string): void;
};

/** Kullanıcının "Sonra" dediği sürüm. */
const SKIPPED_KEY = "hydra.update.skipped";
/** Kurulum başlatıldı ama tamamlanmadı işareti. */
const PENDING_KEY = "hydra.update.pending";

/** Okuma; boş metin "işaret yok" sayılır. */
const read = (storage: Storage, key: string): string | null => {
  const value = storage.getItem(key);
  return value && value.length > 0 ? value : null;
};

export function readSkipped(storage: Storage): string | null {
  return read(storage, SKIPPED_KEY);
}

export function writeSkipped(storage: Storage, version: string): void {
  storage.setItem(SKIPPED_KEY, version);
}

export function readPending(storage: Storage): string | null {
  return read(storage, PENDING_KEY);
}

export function writePending(storage: Storage, version: string): void {
  storage.setItem(PENDING_KEY, version);
}

export function clearPending(storage: Storage): void {
  storage.removeItem(PENDING_KEY);
}

/** Şerit gösterilmeli mi? ("Sonra" denmiş sürüm için hayır.) */
export function shouldPrompt(version: string, skipped: string | null): boolean {
  if (!skipped) return true;
  return skipped !== version;
}

/** İndirme yüzdesi; toplam bilinmiyorsa null. Yuvarlanır, 0..100'e kırpılır. */
export function percent(downloaded: number, total: number | null): number | null {
  if (total === null || !Number.isFinite(total) || total <= 0) return null;
  if (!Number.isFinite(downloaded)) return null;
  const raw = Math.round((downloaded / total) * 100);
  if (raw < 0) return 0;
  if (raw > 100) return 100;
  return raw;
}

/**
 * İşaret varsa ve uygulama hâlâ işaretli sürümde değilse (yükseltme
 * gerçekleşmedi) kurulum başarısız sayılır. Sürüm işaretli sürüme ulaştıysa
 * `false` döner ve çağıran `clearPending` yapmalı.
 */
export function installLooksFailed(pending: string | null, currentVersion: string): boolean {
  if (!pending) return false;
  return pending !== currentVersion;
}
