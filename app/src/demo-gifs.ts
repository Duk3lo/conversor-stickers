import type { ApiGifs } from "./api-gifs.ts";
import type { LibraryUi, EntryUi } from "./contract.ts";
import type { KlipyCategoryUi, GifProvider, GifProviderUi, GifResultUi, StickerKlipyUi } from "./contract-gifs.ts";

export type GifsMode = "con-clave" | "without-key" | "without-connection" | "limit" | "key-invalid" | "sin-resultados";

const Titles = ["Gato saltando", "Perro feliz", "Baile", "Aplausos", "Sorpresa", "Risa", "Fuegos artificiales", "Pulgar arriba"];
const BY_PAGE = 24;
const TOTAL = 40;
const KLIPY_CATEGORIES: readonly KlipyCategoryUi[] = [
  { id: "animales", name: "Animales" },
  { id: "reacciones", name: "Reacciones" },
  { id: "fiesta", name: "Fiesta" },
  { id: "amor", name: "Amor" },
];
const STICKERS_BY_ROW = 10;
const STICKERS_TOTAL = 24;

export interface DemoGifs {
  readonly api: ApiGifs;
  mode(mode: GifsMode, provider?: GifProvider): void;
}

export function createGifsDemo(
  paths: readonly string[],
  currentLibrary: () => LibraryUi,
  delay = 300,
): DemoGifs {
  let mode: GifsMode = "con-clave";
  let provider: GifProvider | null = "klipy";
  let setKey = true;
  let ownKey = false;

  const wait = () => new Promise<void>((resolve) => window.setTimeout(resolve, delay));

  function result(text: string | null, index: number): GifResultUi {
    const title = Titles[index % Titles.length] ?? "GIF";
    return {
      id: `${text ?? "trending"}-${index}`,
      title: text === null ? title : `${title} (${text})`,
      thumbnail: paths[index % Math.max(paths.length, 1)] ?? "",
      width: 128,
      height: 128,
    };
  }

  const api: ApiGifs = {
    async gifProvider(): Promise<GifProviderUi> {
      await wait();
      return { provider, key_set: setKey, key_origin: setKey ? (ownKey ? "user" : "bundled") : null };
    },
    async configureGifProvider(fresh, key) {
      await wait();
      provider = fresh;
      setKey = key !== null;
      ownKey = setKey;
      if (setKey && mode === "without-key") mode = "con-clave";
    },
    async searchGifs(text, page) {
      await wait();
      if (!setKey) throw { kind: "other", message: `Falta la clave de ${provider === "giphy" ? "GIPHY" : "KLIPY"}` };
      if (mode === "without-connection") throw { kind: "other", message: "Sin conexión" };
      if (mode === "limit") throw { kind: "other", message: "Límite de búsquedas: espera un minuto" };
      if (mode === "key-invalid") throw { kind: "other", message: "La clave no es válida" };
      const attribution = provider === "giphy" ? "Powered by GIPHY" : "KLIPY";
      if (mode === "sin-resultados" || text === "nothing") return { results: [], next: null, attribution };
      const from = (page - 1) * BY_PAGE;
      const until = Math.min(from + BY_PAGE, TOTAL);
      const results = Array.from({ length: until - from }, (_, i) => result(text, from + i));
      return { results, next: until < TOTAL ? page + 1 : null, attribution };
    },
    async useGif(id) {
      await wait();
      const path = paths[0] ?? "";
      const entry: EntryUi = {
        id: 900,
        name: `${id}.gif`,
        origin: "link",
        state: { kind: "ready" },
        media: { kind: "image", path, width: 128, height: 128 },
      };
      return entry;
    },
    async stickersKlipy(text, category, page) {
      await wait();
      if (!setKey) throw { kind: "other", message: "Falta la clave de KLIPY" };
      if (mode === "without-connection") throw { kind: "other", message: "Sin conexión" };
      if (mode === "sin-resultados" || text === "nothing") return { results: [], next: null };
      const from = (page - 1) * STICKERS_BY_ROW;
      const until = Math.min(from + STICKERS_BY_ROW, STICKERS_TOTAL);
      const key = text ?? category ?? "trending";
      const results: StickerKlipyUi[] = Array.from({ length: until - from }, (_, i) => {
        const index = from + i;
        return {
          id: `${key}-${index}`,
          title: `${Titles[index % Titles.length] ?? "Sticker"}`,
          thumbnail: paths[(index + key.length) % Math.max(paths.length, 1)] ?? "",
          animated: index % 2 === 0,
        };
      });
      return { results, next: until < STICKERS_TOTAL ? page + 1 : null };
    },
    async klipyCategories() {
      await wait();
      if (!setKey) throw { kind: "other", message: "Falta la clave de KLIPY" };
      if (mode === "without-connection") throw { kind: "other", message: "Sin conexión" };
      return [...KLIPY_CATEGORIES];
    },
    async addKlipySticker(_id, pack, newName) {
      await wait();
      await wait();
      if (newName === "failure") throw { kind: "other", message: "No se pudo convertir el sticker" };
      const library = currentLibrary();
      return { library, pack: pack ?? library.packs[0]?.id ?? 0 };
    },
  };

  return {
    api,
    mode(fresh, chosen) {
      mode = fresh;
      provider = chosen ?? "klipy";
      setKey = fresh !== "without-key";
      ownKey = false;
    },
  };
}
