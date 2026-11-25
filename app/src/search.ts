import { normalize, type PackCard, cards } from "./catalog.ts";
import type { LibraryUi, PackUi } from "./contract.ts";
import type { CatalogUi } from "./contract-community.ts";
import { searchEmojis } from "./emojis.ts";

export const HISTORY_MAX = 8;
export const RESULT_STICKERS_MAX = 60;
export const PACK_STICKERS_MAX = 30;
const HISTORY_KEY = "searches-recent";

export interface StickerResult {
  readonly pack: number;
  readonly namePack: string;
  readonly id: number;
  readonly path: string;
  readonly emojis: readonly string[];
}

export interface SearchResult {
  readonly packs: readonly PackCard[];
  readonly stickers: readonly StickerResult[];
}

function emojisDe(word: string): ReadonlySet<string> {
  const emojis = new Set<string>([word]);
  if (word.length >= 3) for (const entry of searchEmojis(word)) emojis.add(entry.emoji);
  return emojis;
}

function matchesWithEmoji(emojisSticker: readonly string[], word: string): boolean {
  const set = emojisDe(word);
  return emojisSticker.some((emoji) => set.has(emoji));
}

function score(card: PackCard, word: string, packEmojis: readonly string[]): number | null {
  if (normalize(card.name).includes(word)) return 0;
  if (card.tags.some((label) => normalize(label).includes(word))) return 1;
  if (normalize(card.author).includes(word)) return 2;
  if (card.category !== null && normalize(card.category).includes(word)) return 3;
  if (matchesWithEmoji(packEmojis, word)) return 4;
  return null;
}

function emojisOfPack(pack: PackUi | undefined): string[] {
  return pack === undefined ? [] : pack.stickers.flatMap((sticker) => sticker.emojis);
}

export function search(query: string, library: LibraryUi | null, catalog: CatalogUi | null): SearchResult {
  const words = normalize(query).split(/\s+/).filter((word) => word !== "");
  if (words.length === 0) return { packs: [], stickers: [] };
  const locales = library?.packs ?? [];

  const found: { card: PackCard; points: number }[] = [];
  for (const card of cards(library, catalog)) {
    const idBackup = card.origin === "local" ? Number(card.id) : catalog?.packs.find((pack) => pack.id === card.id)?.downloaded;
    const emojis = emojisOfPack(locales.find((local) => local.id === idBackup));
    const points = words.map((word) => score(card, word, emojis));
    if (points.every((point) => point !== null)) {
      found.push({ card, points: Math.min(...points.map((point) => point ?? 9)) });
    }
  }
  found.sort((a, b) => a.points - b.points || a.card.name.localeCompare(b.card.name, "es"));

  const stickers: StickerResult[] = [];
  for (const pack of locales) {
    for (const sticker of pack.stickers) {
      if (words.every((word) => matchesWithEmoji(sticker.emojis, word))) {
        stickers.push({ pack: pack.id, namePack: pack.name, id: sticker.id, path: sticker.path, emojis: sticker.emojis });
      }
    }
  }
  return { packs: found.map((found) => found.card), stickers: stickers.slice(0, RESULT_STICKERS_MAX) };
}

export function withSearch(history: readonly string[], text: string): string[] {
  const clean = text.trim().replace(/\s+/g, " ");
  if (clean === "") return [...history];
  const key = clean.toLowerCase();
  return [clean, ...history.filter((previous) => previous.toLowerCase() !== key)].slice(0, HISTORY_MAX);
}

export function withoutSearch(history: readonly string[], text: string): string[] {
  return history.filter((previous) => previous !== text);
}

type Store = Pick<Storage, "getItem" | "setItem">;

export function readHistory(store: Store | null): string[] {
  try {
    const text = store?.getItem(HISTORY_KEY) ?? null;
    if (text === null) return [];
    const value: unknown = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value.filter((element): element is string => typeof element === "string" && element.trim() !== "").slice(0, HISTORY_MAX);
  } catch {
    return [];
  }
}

export function saveHistory(store: Store | null, history: readonly string[]): boolean {
  try {
    store?.setItem(HISTORY_KEY, JSON.stringify(history.slice(0, HISTORY_MAX)));
    return store !== null;
  } catch {
    return false;
  }
}

export interface FavoritesModel {
  readonly packs: readonly PackCard[];
  readonly stickers: readonly StickerResult[];
  readonly neededPacks: number;
  readonly note: string | null;
}

export function favoritesModel(library: LibraryUi, catalog: CatalogUi | null): FavoritesModel {
  const stickers = library.packs.flatMap((pack) =>
    pack.stickers
      .filter((sticker) => sticker.favorite)
      .map((sticker) => ({ pack: pack.id, namePack: pack.name, id: sticker.id, path: sticker.path, emojis: sticker.emojis })),
  );
  const neededPacks = Math.ceil(stickers.length / PACK_STICKERS_MAX);
  let note: string | null = null;
  if (stickers.length > PACK_STICKERS_MAX) {
    note = `Son ${stickers.length} stickers: se crearán ${neededPacks} paquetes de hasta ${PACK_STICKERS_MAX}.`;
  } else if (stickers.length > 0 && stickers.length < 3) {
    note = "Con menos de 3 stickers el paquete no se podrá añadir a WhatsApp todavía.";
  }
  return {
    packs: cards(library, catalog).filter((card) => card.favorite),
    stickers,
    neededPacks,
    note,
  };
}
