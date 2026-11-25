import { type EntryUi, call, type CreatedPack } from "./contract.ts";

export type GifProvider = "klipy" | "giphy";

export interface GifProviderUi {
  provider: GifProvider | null;
  key_set: boolean;
  key_origin: KeyOrigin | null;
}

export type KeyOrigin = "bundled" | "user" | "proxy";

export interface GifResultUi {
  id: string;
  title: string;
  thumbnail: string;
  width: number;
  height: number;
}

export interface GifsPageUi {
  results: GifResultUi[];
  next: number | null;
  attribution: string;
}

export function gifProvider(): Promise<GifProviderUi> {
  return call("gif_provider");
}

export function configureGifProvider(provider: GifProvider, key: string | null): Promise<void> {
  return call("configure_gif_provider", { provider, key });
}

export function searchGifs(text: string | null, page: number): Promise<GifsPageUi> {
  return call("search_gifs", { text, page });
}

export function useGif(id: string): Promise<EntryUi> {
  return call("use_gif", { id });
}

export interface StickerKlipyUi {
  id: string;
  title: string;
  thumbnail: string;
  animated: boolean;
}

export interface KlipyStickersPageUi {
  results: StickerKlipyUi[];
  next: number | null;
}

export interface KlipyCategoryUi {
  id: string;
  name: string;
}

export function stickersKlipy(text: string | null, category: string | null, page: number): Promise<KlipyStickersPageUi> {
  return call("klipy_stickers", { text, category, page });
}

export function klipyCategories(): Promise<KlipyCategoryUi[]> {
  return call("klipy_categories");
}

export function addKlipySticker(id: string, pack: number | null, newName: string | null): Promise<CreatedPack> {
  return call("add_klipy_sticker", { id, pack, newName });
}
