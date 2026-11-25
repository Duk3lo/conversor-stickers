import { defineConfig, loadEnv } from "vite";

// CONVERSOR_EDITION (github by default) is the single switch of the edition; the same variable
// drives the Cargo feature and Gradle in `src-tauri/android-release.sh`.
export default defineConfig(({ mode }) => {
  const play = loadEnv(mode, ".", "CONVERSOR_EDITION")["CONVERSOR_EDITION"] === "play";
  return {
    define: { "import.meta.env.VITE_EDITION": JSON.stringify(play ? "play" : "github") },
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
  };
});
