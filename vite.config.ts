import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const root = path.dirname(fileURLToPath(import.meta.url));

/**
 * `@tauri-apps/plugin-updater` paketi Task 2'de kurulacak. Kurulu değilken
 * derleme patlamasın diye boş bir yer tutucuya yönlendiriyoruz; paket
 * kurulduğunda bu alias kendiliğinden devre dışı kalır ve gerçek eklenti
 * paketlenir. Geliştirme sunucusunda alias yok: `src/updater.ts` eklentiyi
 * `/@id/...` üzerinden ister (harness sahte modülle karşılar).
 */
const updaterInstalled = existsSync(
  path.join(root, "node_modules", "@tauri-apps", "plugin-updater", "package.json"),
);

// Tauri'nin bekledigi sabit port; port doluysa sessizce baska porta kaymasin.
export default defineConfig(({ command }) => ({
  clearScreen: false,
  resolve:
    !updaterInstalled
      ? { alias: { "@tauri-apps/plugin-updater": path.join(root, "src", "plugin-updater-missing.ts") } }
      : undefined,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/extension/**"],
    },
  },
  build: {
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
}));

