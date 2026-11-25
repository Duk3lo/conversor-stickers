import type { AddResult } from "./contract-mobile.ts";
import type { Platform } from "./mobile.ts";

export interface StickerOptions {
  readonly share: boolean;
  readonly save: boolean;
  readonly whatsapp: boolean;
}

/**
 * What can be done with one sticker. Both need it to be in the library (a pack and a sticker
 * number); the system share sheet only exists on Android.
 */
export function stickerOptions(platform: Platform, pack: number | null, sticker: number | null): StickerOptions {
  const inLibrary = pack !== null && sticker !== null;
  return { share: inLibrary && platform === "android", save: inLibrary, whatsapp: inLibrary && platform === "android" };
}

/** A result from the editor can be shared when it is a WebP sticker that fits the limits. */
export function canShareResult(platform: Platform, destination: string, meetsLimit: boolean): boolean {
  return platform === "android" && destination === "sticker" && meetsLimit;
}

/** The quick WhatsApp button follows the same rule as sharing: Android, sticker destination, fits the limits. */
export function canSendToWhatsapp(platform: Platform, destination: string, meetsLimit: boolean): boolean {
  return canShareResult(platform, destination, meetsLimit);
}

export function sentToWhatsappNotice(result: AddResult, notice: string | null): string | null {
  if (result === "cancelled") return notice;
  return "Listo: búscalo en tu bandeja de stickers de WhatsApp, en “Mis stickers rápidos”.";
}

export function savedStickerNotice(name: string | null): string | null {
  return name === null ? null : `Sticker guardado como ${name}.`;
}

export const SHARE_AS_IMAGE_HINT = "WhatsApp lo recibe como foto. Para usarlo como sticker, añádelo a un paquete.";
