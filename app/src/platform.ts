import type { Platform } from "./mobile.ts";

export const platform: Platform = import.meta.env?.TAURI_ENV_PLATFORM === "android" ? "android" : "desktop";
