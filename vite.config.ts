import { defineConfig } from "vite";

// Tauri'nin bekledigi sabit port; port doluysa sessizce baska porta kaymasin.
export default defineConfig({
  clearScreen: false,
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
});
