// `@tauri-apps/plugin-updater` henüz kurulmadı (Task 2 kuruyor).
//
// Burada yalnız `src/updater.ts`'in kullandığı yüzey tanımlı: `check()` ve
// `Update`. Paket `package.json`'a eklendiğinde **bu dosya silinmeli** —
// gerçek tiplerle çakışabilir. `tsconfig` gevşetilmiyor; tek amaç modülün
// şeklini tipli tutmak.

declare module "@tauri-apps/plugin-updater" {
  export type DownloadEvent =
    | { event: "Started"; data: { contentLength?: number } }
    | { event: "Progress"; data: { chunkLength: number; contentLength?: number } }
    | { event: "Finished" };

  export interface Update {
    /** Yeni sürüm (ör. "0.8.1"). */
    readonly version: string;
    /** Sürüm notları (markdown olabilir). */
    readonly body?: string;
    /** İndirip kurar; olay akışıyla ilerleme bildirir. */
    downloadAndInstall(onEvent?: (event: DownloadEvent) => void): Promise<void>;
    /** Kaynağı bırakır. */
    close(): Promise<void>;
  }

  export function check(options?: unknown): Promise<Update | null>;
}
