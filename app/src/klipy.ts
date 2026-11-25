import type { MediaOrigin, PackUi, StickerUi } from "./contract.ts";
import type { KlipyCategoryUi, KeyOrigin, StickerKlipyUi } from "./contract-gifs.ts";
import { validatePackName } from "./packs.ts";

export const KLIPY_ATTRIBUTION = "KLIPY";
export const CATEGORIES_IN_COVER = 6;
export const NEW_CHOICE = "new";

export type KlipyRowState =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly results: readonly StickerKlipyUi[];
      readonly next: number | null;
      readonly loadingPlus: boolean;
      readonly plusError: string | null;
    }
  | { readonly kind: "hidden" };

export interface KlipyRow {
  readonly key: string;
  readonly title: string;
  readonly category: string | null;
  readonly state: KlipyRowState;
}

export function initialRows(categories: readonly KlipyCategoryUi[]): KlipyRow[] {
  const loading: KlipyRowState = { kind: "loading" };
  return [
    { key: "trending", title: "Tendencias", category: null, state: loading },
    ...categories.slice(0, CATEGORIES_IN_COVER).map((category) => ({
      key: `category-${category.id}`,
      title: category.name,
      category: category.id,
      state: loading,
    })),
  ];
}

export function splitTrending(rows: readonly KlipyRow[]): { first: KlipyRow[]; rest: KlipyRow[] } {
  return { first: rows.filter((row) => row.key === "trending"), rest: rows.filter((row) => row.key !== "trending") };
}

export function visibleRow(row: KlipyRow): boolean {
  if (row.state.kind === "hidden") return false;
  return row.state.kind === "loading" || row.state.results.length > 0;
}

export function joinStickers(current: readonly StickerKlipyUi[], fresh: readonly StickerKlipyUi[]): StickerKlipyUi[] {
  const seen = new Set(current.map((sticker) => sticker.id));
  return [...current, ...fresh.filter((sticker) => !seen.has(sticker.id))];
}

export function originLabel(origin: MediaOrigin | undefined): string | null {
  if (origin === "klipy") return "KLIPY";
  if (origin === "giphy") return "GIPHY";
  return null;
}

export function foreignOriginReason(pack: PackUi): string | null {
  const origins = new Set(pack.stickers.map((sticker: StickerUi) => originLabel(sticker.media_origin)));
  const names = ["KLIPY", "GIPHY"].filter((name) => origins.has(name));
  return names.length === 0 ? null : `Contiene stickers de ${names.join(" y ")}.`;
}

export type PackDestination =
  | { readonly ok: true; readonly pack: number | null; readonly newName: string | null }
  | { readonly ok: false; readonly reason: string };

export function packDestination(choice: string, newName: string, packs: readonly PackUi[]): PackDestination {
  if (choice === NEW_CHOICE) {
    const name = validatePackName(newName);
    return name.valid ? { ok: true, pack: null, newName: name.value } : { ok: false, reason: name.reason };
  }
  const existing = packs.find((pack) => String(pack.id) === choice);
  if (existing === undefined) return { ok: false, reason: "Elige un paquete o crea uno nuevo." };
  return { ok: true, pack: existing.id, newName: null };
}

export function keyOriginText(origin: KeyOrigin | null): string | null {
  if (origin === "bundled") return "Clave integrada";
  if (origin === "user") return "Tu clave";
  if (origin === "proxy") return "Servicio de la app";
  return null;
}
