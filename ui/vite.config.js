import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
export default defineConfig({
    plugins: [react()],
    resolve: {
        alias: {
            "@": path.resolve(__dirname, "./src"),
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
    build: {
        target: "es2022",
        outDir: "dist",
        sourcemap: false,
        chunkSizeWarningLimit: 1024,
    },
});
