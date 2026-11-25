import type { GifProvider, GifResultUi } from "./contract-gifs.ts";

export const QUERY_MAX = 100;
export const KEY_MAX = 200;
export const WRITING_DELAY_MS = 400;

export type ErrorReason = "without-key" | "key-invalid" | "limit" | "without-connection" | "other";

export type GifSearch =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly results: readonly GifResultUi[];
      readonly next: number | null;
      readonly attribution: string;
      readonly loadingPlus: boolean;
      readonly plusError: string | null;
    }
  | { readonly kind: "error"; readonly reason: ErrorReason; readonly message: string };

export function queryText(text: string): string | null {
  const clean = text.trim();
  return clean === "" ? null : [...clean].slice(0, QUERY_MAX).join("");
}

export function classifyError(message: string): ErrorReason {
  const text = message.toLowerCase();
  if (text.includes("falta la clave")) return "without-key";
  if (text.includes("no es válida")) return "key-invalid";
  if (text.includes("límite")) return "limit";
  if (text.includes("sin conexión")) return "without-connection";
  return "other";
}

export function validateKey(key: string): { ok: true; value: string } | { ok: false; reason: string } {
  const value = key.trim();
  if (value === "") return { ok: false, reason: "Escribe la clave del proveedor." };
  if (/\s/.test(value)) return { ok: false, reason: "La clave no lleva espacios." };
  if (value.length > KEY_MAX) return { ok: false, reason: `La clave es demasiado larga (máximo ${KEY_MAX} caracteres).` };
  return { ok: true, value };
}

export function providerName(provider: GifProvider): string {
  return provider === "giphy" ? "GIPHY" : "KLIPY";
}

export function keyLink(provider: GifProvider): string {
  return provider === "giphy" ? "https://developers.giphy.com" : "https://partner.klipy.com";
}

export function visibleAttribution(provider: GifProvider | null, received: string): string {
  if (received.trim() !== "") return received;
  if (provider === null) return "";
  return provider === "giphy" ? "Powered by GIPHY" : "KLIPY";
}

export function joinPage(current: readonly GifResultUi[], fresh: readonly GifResultUi[]): GifResultUi[] {
  const seen = new Set(current.map((result) => result.id));
  return [...current, ...fresh.filter((result) => !seen.has(result.id))];
}

export interface Timer {
  set(action: () => void, ms: number): number;
  remove(id: number): void;
}

export function createDelay<T>(
  action: (value: T) => void,
  ms: number,
  timer: Timer = {
    set: (make, waits) => window.setTimeout(make, waits),
    remove: (id) => window.clearTimeout(id),
  },
): { request(value: T): void; cancel(): void } {
  let pending: number | null = null;
  function cancel(): void {
    if (pending !== null) timer.remove(pending);
    pending = null;
  }
  return {
    request(value) {
      cancel();
      pending = timer.set(() => {
        pending = null;
        action(value);
      }, ms);
    },
    cancel,
  };
}
