import type { SaveVideoEvent } from "./contract.ts";
import { sizeText } from "./queue.ts";
import type { Platform } from "./mobile.ts";

/** The video being saved. Only one at a time: the backend shares the downloader with the queue. */
export interface Saving {
  readonly id: number;
  readonly event: SaveVideoEvent | null;
}

export function startSaving(id: number): Saving {
  return { id, event: null };
}

export function savingText(saving: Saving): string {
  const event = saving.event;
  if (event === null) return "Preparando…";
  if (event.kind === "writing") return "Guardando…";
  if (event.total === null || event.total <= 0) return `Descargando con sonido ${sizeText(event.bytes)}`;
  return `Descargando con sonido ${Math.min(100, Math.round((event.bytes / event.total) * 100))} %`;
}

export function savingPercentage(saving: Saving): number | null {
  const event = saving.event;
  if (event === null) return null;
  if (event.kind === "writing") return 100;
  if (event.total === null || event.total <= 0) return null;
  return Math.min(100, Math.round((event.bytes / event.total) * 100));
}

export function savedNotice(platform: Platform, name: string): string {
  return platform === "android" ? "Guardado en Vídeos" : `Guardado como ${name}`;
}
