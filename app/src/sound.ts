import type { ClipRange } from "./crop.ts";
import type { MediaUi, Origin } from "./contract.ts";

/** The longest sound WhatsApp takes for a sticker. */
export const MAX_SOUND_S = 10;

/** State of «Guardar sonido para sticker» in the editor. */
export type SoundState =
  | { readonly kind: "unavailable" }
  | { readonly kind: "idle" }
  | { readonly kind: "saving" }
  | { readonly kind: "saved"; readonly name: string }
  | { readonly kind: "failed"; readonly message: string };

/**
 * A video from a link is downloaded without audio to make stickers, so it is not known whether the
 * original has any: the button is offered and the backend answers if it has none.
 */
export function canSaveSound(media: MediaUi, origin: Origin): boolean {
  return media.kind === "video" && (media.has_audio || origin === "link");
}

export function initialSound(media: MediaUi, origin: Origin): SoundState {
  return canSaveSound(media, origin) ? { kind: "idle" } : { kind: "unavailable" };
}

export function soundRequest(clipRange: ClipRange): { startS: number; seconds: number } {
  return { startS: clipRange.start, seconds: Math.min(clipRange.end - clipRange.start, MAX_SOUND_S) };
}

export function soundText(sound: SoundState): string {
  switch (sound.kind) {
    case "saving":
      return "Guardando sonido…";
    case "saved":
      return `Sonido guardado como ${sound.name}`;
    case "failed":
      return sound.message;
    default:
      return "";
  }
}
