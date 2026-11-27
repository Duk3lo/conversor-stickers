// The smallest metadata: it formats national numbers like `max`; only the stricter `isValid` differs, unused here.
import { type CountryCode, getCountries, getCountryCallingCode, isSupportedCountry, parsePhoneNumberFromString } from "libphonenumber-js/min";

import type { ExtensionUi, FramingSpecUi } from "./contract.ts";
import type { ClipVideoState } from "./clip-video.ts";
import type { Platform } from "./mobile.ts";
import { githubEdition } from "./edition.ts";
import type { Store } from "./store.ts";

const KEY = "conversor.extensions";
const MIN_DIGITS = 8;
const MAX_DIGITS = 15;
const STORED_NUMBER = new RegExp(`^[0-9]{${MIN_DIGITS},${MAX_DIGITS}}$`);
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

/**
 * The chat number as the user types it, turned into digits with the country code and no `+` (Rust checks them
 * again on sending). A leading `+` is international; otherwise it is a national number of `country`.
 */
export function buildNumber(text: string, country: CountryCode | null): NumberInput {
  const trimmed = text.trim();
  if (trimmed === "") return { kind: "none" };
  if (!/^\+?[0-9\s()-]*$/.test(trimmed)) {
    return invalid("Usa solo dígitos, espacios, guiones y paréntesis; el “+” solo puede ir al principio.");
  }
  const international = trimmed.startsWith("+");
  if (!international && country === null) return invalid("Elige un país o escribe el número con “+” y el código de país.");
  // No Argentine area code starts with 15: it is a mobile prefix typed without its area code.
  if (!international && country === "AR" && /^0?15/.test(trimmed.replace(/[\s()-]/g, ""))) {
    return invalid("Falta el código de área: escribe el área antes del 15.");
  }
  const parsed = parsePhoneNumberFromString(trimmed.replace(/[\s()-]/g, ""), {
    ...(country === null ? {} : { defaultCountry: country }),
    extract: false,
  });
  // What follows a `+` is the user's explicit choice: forms the library calls impossible (Mexico's `+52 1`) are kept.
  if (parsed === undefined || (!international && !parsed.isPossible())) {
    return invalid(international ? "Revisa el código de país y el número." : "El número no encaja con el país elegido. Revísalo.");
  }
  const digits = parsed.number.slice(1);
  if (!STORED_NUMBER.test(digits)) {
    return invalid(`Con el código de país, el número debe tener de ${MIN_DIGITS} a ${MAX_DIGITS} dígitos.`);
  }
  return { kind: "number", digits };
}

function invalid(message: string): NumberInput {
  return { kind: "invalid", message };
}

export interface CountryOption {
  readonly code: CountryCode;
  readonly label: string;
}

export function countryOptions(): CountryOption[] {
  const names = new Intl.DisplayNames(["es"], { type: "region" });
  const collator = new Intl.Collator("es");
  return getCountries()
    .map((code) => ({ code, label: `${names.of(code) ?? code} (+${getCountryCallingCode(code)})` }))
    .sort((a, b) => collator.compare(a.label, b.label));
}

/** Only a region the language states (`es-AR`): a bare `es` says nothing about where the number is from. */
export function defaultCountry(language: string): CountryCode | null {
  try {
    const region = new Intl.Locale(language).region;
    return region !== undefined && isSupportedCountry(region) ? region : null;
  } catch {
    return null;
  }
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

/** The built-in extension is always there, even if an older version let the user remove it; it keeps its acceptance and number. */
export function loadExtensions(
  store: Store | null,
  preinstalled: readonly ExtensionUi[] = githubEdition ? [builtinExtension] : [],
): InstalledExtension[] {
  const withPreinstalled = (list: InstalledExtension[]): InstalledExtension[] => [
    ...preinstalled
      .filter((extension) => !list.some((installed) => installed.extension.id === extension.id))
      .map((extension) => ({ extension, accepted: false, number: null })),
    ...list,
  ];
  try {
    const stored = store?.getItem(KEY) ?? null;
    if (stored === null) return withPreinstalled([]);
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return withPreinstalled([]);
    return withPreinstalled(
      parsed.filter(isInstalled).map((installed) => {
        const number = installed.number !== null && STORED_NUMBER.test(installed.number) ? installed.number : null;
        return { ...installed, number };
      }),
    );
  } catch {
    return withPreinstalled([]);
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

/** The bot shipped with the app stays: it is the one the connection and the framing belong to. */
export function canRemoveExtension(id: string): boolean {
  return id !== builtinExtension.id;
}

export function removeExtension(list: readonly InstalledExtension[], id: string): InstalledExtension[] {
  if (!canRemoveExtension(id)) return [...list];
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

/** The bot shipped with the app is the only one known to read how the video is framed. */
function framingSettings(extension: ExtensionUi, framing: FramingSpecUi | null): string[] {
  // Keeping the whole video is the bot's default, so nothing has to be said.
  if (extension.id !== builtinExtension.id || framing === null || framing.mode !== "crop") return [];
  const value = (number: number): string => String(Math.round(number * 1000) / 1000);
  const settings = ["encuadre=recortar"];
  if (value(framing.zoom) !== "1") settings.push(`zoom=${value(framing.zoom)}`);
  if (value(framing.center_x) !== "0.5") settings.push(`x=${value(framing.center_x)}`);
  if (value(framing.center_y) !== "0.5") settings.push(`y=${value(framing.center_y)}`);
  return settings;
}

/** The caption the chat reads: `sonido=5` when the extension asks for the sound; `null` if it says nothing. */
export function extensionCaption(extension: ExtensionUi, soundS: number, framing: FramingSpecUi | null = null): string | null {
  const seconds = Math.min(Math.floor(soundS), MAX_SOUND_S);
  const sound = extension.sound && seconds >= 1 ? [`sonido=${seconds}`] : [];
  const settings = [...sound, ...framingSettings(extension, framing)];
  return settings.length === 0 ? null : settings.join(" ");
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
