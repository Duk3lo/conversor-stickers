import type { MediaUi } from "./contract.ts";
import type { Platform } from "./mobile.ts";

/** State of «Sticker con sonido (experimental)»: the clip saved as a video for WhatsApp to read. */
export type ClipVideoState =
  | { readonly kind: "idle" }
  | { readonly kind: "saving" }
  | { readonly kind: "saved"; readonly name: string }
  | { readonly kind: "failed"; readonly message: string };

/** The button saves the clip as a video, which only makes sense for a video, on Android. */
export function canSaveClipVideo(platform: Platform, destination: string, media: MediaUi): boolean {
  return platform === "android" && destination === "sticker" && media.kind === "video";
}

/** «Abrir WhatsApp» shows once the clip is saved, and only on Android, where WhatsApp can be opened. */
export function openWhatsappVisible(platform: Platform, state: ClipVideoState): boolean {
  return platform === "android" && state.kind === "saved";
}

export function clipVideoText(state: ClipVideoState): string {
  switch (state.kind) {
    case "saving":
      return "Guardando vídeo…";
    case "saved":
      return "Vídeo guardado. En WhatsApp: Stickers → Crear sticker → elige este vídeo. Si tu WhatsApp ya tiene stickers con sonido, lo incluirá.";
    case "failed":
      return state.message;
    default:
      return "";
  }
}
