import type { Platform } from "./mobile.ts";

/** Folders the app saves into that the file manager can open; the name Rust and Kotlin validate. */
export type SavedFolder = "videos" | "sounds";

/** The folders only exist, and open, on Android: a desktop save asks the user where to put it. */
export function canOpenFolder(platform: Platform): boolean {
  return platform === "android";
}

/** The «Abrir carpeta» button shows next to a save that worked, and only where it can open. */
export function folderButtonVisible(platform: Platform, folder: SavedFolder | null): boolean {
  return canOpenFolder(platform) && folder !== null;
}
