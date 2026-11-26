import { githubEdition } from "./edition.ts";
import type { PackUi } from "./contract.ts";
import { back, type Navigation, type Root, type Section } from "./navigation.ts";
import { exportableNoReason } from "./packs.ts";

export type Platform = "desktop" | "android";

export function destinationLabel(platform: Platform, root: Root): string {
  switch (root) {
    case "start":
      return "Inicio";
    case "community":
      return "Comunidad";
    case "create":
      return "Crear";
    case "favorites":
      return "Favoritos";
    case "mis-packs":
      return platform === "android" ? "Paquetes" : "Mis paquetes";
  }
}

export function tabLabel(section: Section): { name: string; detail: string } {
  switch (section) {
    case "stickers":
      return { name: "Sticker", detail: "WhatsApp" };
    case "gifs":
      return { name: "GIF", detail: "Discord" };
    case "download":
      return { name: "Descargar", detail: "Enlaces y pegar" };
    case "search-gif":
      return { name: "Buscar GIF", detail: "KLIPY y GIPHY" };
  }
}

export interface PlatformControls {
  readonly links: boolean;
  readonly ctrlV: boolean;
  readonly addToWhatsapp: boolean;
  readonly ffmpegCredits: boolean;
}

export function controls(platform: Platform): PlatformControls {
  const android = platform === "android";
  return { links: githubEdition, ctrlV: !android, addToWhatsapp: android, ffmpegCredits: android };
}

export const PASTE_HELP = {
  desktop: "Una imagen o un video copiado. También con Ctrl+V en cualquier pestaña.",
  android: "Copia una imagen o un video en otra app y pulsa Pegar; o pega un enlace abajo.",
} as const satisfies Record<Platform, string>;

export const CREATE_HELP = {
  desktop: "También puedes pegar una imagen o un video con Ctrl+V.",
  android: "También puedes pegar una imagen o un video desde la pestaña Descargar.",
} as const satisfies Record<Platform, string>;

export const EMPTY_QUEUE = {
  desktop: "Aquí aparecen los enlaces, lo que pegues y los archivos cuando eliges varios a la vez.",
  android: "Aquí aparecen los enlaces, lo que pegues y los archivos cuando eliges varios a la vez.",
} as const satisfies Record<Platform, string>;

export function whatsappState(
  platform: Platform,
  pack: PackUi,
): { visible: boolean; active: boolean; reason: string | null } {
  if (!controls(platform).addToWhatsapp) return { visible: false, active: false, reason: null };
  const reason = exportableNoReason(pack);
  return { visible: true, active: reason === null, reason };
}

export function previewMode(platform: Platform): "window-webm" | "file" {
  return platform === "android" ? "file" : "window-webm";
}

export function playableNoVideoReason(platform: Platform): string {
  return platform === "android"
    ? "el sistema no reconoce su códec"
    : "el sistema no pudo reproducirla; en Linux hace falta gst-plugins-good";
}

export type BackAction =
  | { readonly kind: "navigate"; readonly nav: Navigation }
  | { readonly kind: "exit" }
  | { readonly kind: "none" };

export function backAction(platform: Platform, nav: Navigation): BackAction {
  if (platform !== "android") return { kind: "none" };
  const previous = back(nav);
  return previous === null ? { kind: "exit" } : { kind: "navigate", nav: previous };
}
