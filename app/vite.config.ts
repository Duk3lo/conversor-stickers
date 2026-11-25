import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  // Vite hides TAURI_ENV_PLATFORM unless its prefix is exposed.
  envPrefix: ["VITE_", "TAURI_ENV_"],
  server: {
    // Fixed port: devUrl in src-tauri/tauri.conf.json points to it.
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "safari16",
    outDir: "dist",
    emptyOutDir: true,
  },
});
