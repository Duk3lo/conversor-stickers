import type { EntryUi, CreatedPack } from "./contract.ts";
import * as gifs from "./contract-gifs.ts";
import type { KlipyCategoryUi, GifsPageUi, KlipyStickersPageUi, GifProvider, GifProviderUi } from "./contract-gifs.ts";

export interface ApiGifs {
  gifProvider(): Promise<GifProviderUi>;
  configureGifProvider(provider: GifProvider, key: string | null): Promise<void>;
  searchGifs(text: string | null, page: number): Promise<GifsPageUi>;
  useGif(id: string): Promise<EntryUi>;
  stickersKlipy(text: string | null, category: string | null, page: number): Promise<KlipyStickersPageUi>;
  klipyCategories(): Promise<KlipyCategoryUi[]>;
  addKlipySticker(id: string, pack: number | null, newName: string | null): Promise<CreatedPack>;
}

export const apiGifsReal: ApiGifs = {
  gifProvider: gifs.gifProvider,
  configureGifProvider: gifs.configureGifProvider,
  searchGifs: gifs.searchGifs,
  useGif: gifs.useGif,
  stickersKlipy: gifs.stickersKlipy,
  klipyCategories: gifs.klipyCategories,
  addKlipySticker: gifs.addKlipySticker,
};
