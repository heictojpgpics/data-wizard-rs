import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(dirname, "./src"),
    },
  },
  // Tauri expects a fixed port; if that's not available, it will fail.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Don't watch the Rust side; that triggers spurious reloads.
      ignored: ["**/src-tauri/**", "**/target/**", "**/crates/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  // Single-entry app: constrain the dep-scanner to the real entry so
  // stray HTML files are never crawled.
  optimizeDeps: {
    entries: ["index.html"],
  },
  build: {
    // Tauri uses Chromium on Windows WebView2; 2021+ support is fine.
    target: "chrome105",
    outDir: "dist",
    sourcemap: false,
    chunkSizeWarningLimit: 1024,
  },
});
