import type { ExtensionUi } from "./contract.ts";
import type { ClipVideoState } from "./clip-video.ts";
import type { Platform } from "./mobile.ts";
import { githubEdition } from "./edition.ts";
import type { Store } from "./store.ts";

const KEY = "conversor.extensions";
const MIN_DIGITS = 8;
const MAX_DIGITS = 15;
const MAX_SOUND_S = 600;
// WhatsApp plays an animated sticker's image for at most 10 s, then freezes it, while the audio goes on.
const STICKER_IMAGE_MAX_S = 10;

/** An installed extension with what the user decided: the notice acceptance and the chat number. */
export interface InstalledExtension {
  readonly extension: ExtensionUi;
  readonly accepted: boolean;
  readonly number: string | null;
}

export type NumberInput =
  | { readonly kind: "none" }
  | { readonly kind: "number"; readonly digits: string }
  | { readonly kind: "invalid"; readonly message: string };

/** The chat number: digits only, with the country code and no `+`. Rust checks it again on sending. */
export function parseNumber(text: string): NumberInput {
  const trimmed = text.trim();
  if (trimmed === "") return { kind: "none" };
  if (!new RegExp(`^[0-9]{${MIN_DIGITS},${MAX_DIGITS}}$`).test(trimmed)) {
    return { kind: "invalid", message: `Escribe solo dígitos, de ${MIN_DIGITS} a ${MAX_DIGITS}, con prefijo de país y sin “+”.` };
  }
  return { kind: "number", digits: trimmed };
}

function isExtension(value: unknown): value is ExtensionUi {
  if (typeof value !== "object" || value === null) return false;
  const e = value as Record<string, unknown>;
  return (
    typeof e["id"] === "string" &&
    typeof e["name"] === "string" &&
    typeof e["description"] === "string" &&
    typeof e["notice"] === "string" &&
    typeof e["url"] === "string" &&
    typeof e["sound"] === "boolean" &&
    (e["max_sound_s"] === null || typeof e["max_sound_s"] === "number")
  );
}

function isInstalled(value: unknown): value is InstalledExtension {
  if (typeof value !== "object" || value === null) return false;
  const e = value as Record<string, unknown>;
  return isExtension(e["extension"]) && typeof e["accepted"] === "boolean" && (e["number"] === null || typeof e["number"] === "string");
}

/** Shipped with the `github` edition (the same manifest Rust validates in its tests); still needs the notice and a number. */
export const builtinExtension: ExtensionUi = {
  id: "bot-stickers-sonido",
  name: "Bot de stickers con sonido",
  description: "Recibe tus vídeos por WhatsApp y te devuelve un sticker animado con sonido.",
  notice:
    "Cliente no oficial de WhatsApp (Baileys): puede provocar el bloqueo de la cuenta. Úsalo con un número secundario, solo para uso personal y bajo tu responsabilidad. Sin garantía.",
  url: "https://github.com/Duk3lo/bot-stickers-sonido",
  sound: true,
  max_sound_s: 10,
};

/** Until the user saves a list (even an empty one, after «Quitar»), the built-in extension is there, unaccepted. */
export function loadExtensions(
  store: Store | null,
  preinstalled: readonly ExtensionUi[] = githubEdition ? [builtinExtension] : [],
): InstalledExtension[] {
  try {
    const stored = store?.getItem(KEY) ?? null;
    if (stored === null) return preinstalled.map((extension) => ({ extension, accepted: false, number: null }));
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isInstalled).map((installed) => {
      const number = installed.number === null ? null : parseNumber(installed.number);
      return { ...installed, number: number?.kind === "number" ? number.digits : null };
    });
  } catch {
    return [];
  }
}

export function saveExtensions(store: Store | null, list: readonly InstalledExtension[]): void {
  store?.setItem(KEY, JSON.stringify(list));
}

/** Installing again with the same id replaces the manifest and asks to accept the notice anew. */
export function installExtension(list: readonly InstalledExtension[], extension: ExtensionUi): InstalledExtension[] {
  const previous = list.find((installed) => installed.extension.id === extension.id);
  const next: InstalledExtension = { extension, accepted: false, number: previous?.number ?? null };
  return previous === undefined ? [...list, next] : list.map((installed) => (installed === previous ? next : installed));
}

export function removeExtension(list: readonly InstalledExtension[], id: string): InstalledExtension[] {
  return list.filter((installed) => installed.extension.id !== id);
}

export function changeExtension(
  list: readonly InstalledExtension[],
  id: string,
  changes: Partial<Pick<InstalledExtension, "accepted" | "number">>,
): InstalledExtension[] {
  return list.map((installed) => (installed.extension.id === id ? { ...installed, ...changes } : installed));
}

/** The extensions «Enviar a …» is offered for: accepted, with a number, on Android and with a saved clip. */
export function sendTargets(list: readonly InstalledExtension[], platform: Platform, state: ClipVideoState): InstalledExtension[] {
  if (platform !== "android" || state.kind !== "saved") return [];
  return list.filter((installed) => installed.accepted && installed.number !== null);
}

/** The caption the chat reads: `sonido=5` when the extension asks for the sound; `null` otherwise. */
export function extensionCaption(extension: ExtensionUi, soundS: number): string | null {
  const seconds = Math.min(Math.floor(soundS), MAX_SOUND_S);
  return extension.sound && seconds >= 1 ? `sonido=${seconds}` : null;
}

/** A notice, not an error: the range is longer than the sound the extension says it takes. */
export function soundDurationNotice(extension: ExtensionUi, soundS: number): string | null {
  const seconds = Math.min(Math.floor(soundS), MAX_SOUND_S);
  if (!extension.sound || extension.max_sound_s === null || seconds <= extension.max_sound_s) return null;
  return `Se enviarán ${seconds} s de sonido a ${extension.name}, que declara un máximo de ${extension.max_sound_s} s.`;
}

/** A notice: the sound outlasts the sticker image, which WhatsApp freezes after `STICKER_IMAGE_MAX_S`. */
export function stillImageNotice(extension: ExtensionUi, soundS: number): string | null {
  const seconds = Math.min(Math.floor(soundS), MAX_SOUND_S);
  if (!extension.sound || seconds <= STICKER_IMAGE_MAX_S) return null;
  return `La imagen del sticker dura ${STICKER_IMAGE_MAX_S} s; el resto del sonido (${seconds - STICKER_IMAGE_MAX_S} s) sonará con la imagen quieta.`;
}
