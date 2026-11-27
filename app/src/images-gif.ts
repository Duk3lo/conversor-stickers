import type { ImagesGifRequest } from "./contract.ts";

/** Same limits as `conversor_core::images_gif`; Rust validates them again. */
export const MAX_GIF_IMAGES = 50;
export const MIN_IMAGE_SECONDS = 0.1;
export const MAX_IMAGE_SECONDS = 5;
export const DEFAULT_IMAGE_SECONDS = 0.5;

export function clampImageSeconds(seconds: number): number {
  if (!Number.isFinite(seconds)) return DEFAULT_IMAGE_SECONDS;
  const tenths = Math.round(seconds * 10) / 10;
  return Math.min(MAX_IMAGE_SECONDS, Math.max(MIN_IMAGE_SECONDS, tenths));
}

export function imageSecondsText(seconds: number): string {
  return `${String(seconds).replace(".", ",")} s`;
}

export function imagesChosenText(count: number): string {
  return count === 1 ? "1 imagen: GIF sin movimiento" : `${count} imágenes: GIF animado`;
}

export function imagesGifRequest(ids: readonly number[], imageSeconds: number, repeat: boolean): ImagesGifRequest {
  return { ids: [...ids], image_s: imageSeconds, repeat };
}
