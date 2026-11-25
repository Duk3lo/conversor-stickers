import type { CatalogApi } from "./catalog-api.ts";
import type { LibraryUi, CheckUi, OriginUi, PackUi, StickerUi, StickerKind } from "./contract.ts";
import type { CatalogUi, CommunityPackUi } from "./contract-community.ts";

const EMOJIS = ["🐱", "😂", "👍", "🎉", "😴", "🔥", "❤️", "🤔"];
const Now = 1_790_000_000;

export function demoPack(
  id: number,
  name: string,
  category: number | null,
  kind: StickerKind | null,
  paths: string[],
  changes: Partial<PackUi> = {},
): PackUi {
  const stickers: StickerUi[] = paths.map((path, index) => ({
    id: index + 1,
    path,
    kind: kind ?? "animated",
    size_bytes: 180_000,
    emojis: [EMOJIS[(index + id) % EMOJIS.length] ?? "🙂"],
    favorite: (id === 1 && index === 2) || (id === 2 && index === 1),
    ...(id === 2 && index === 1 ? { media_origin: "klipy" as const } : {}),
  }));
  const pack: PackUi = {
    id,
    name,
    author: "",
    category,
    kind,
    stickers,
    icon: paths[0] ?? null,
    exportable: false,
    favorite: false,
    modified: Now - id * 3600,
    published: null,
    origin: { kind: "own" },
    checks: [],
    ...changes,
  };
  return withChecks(pack);
}

export function withChecks(pack: PackUi): PackUi {
  const n = pack.stickers.length;
  const kinds = new Set(pack.stickers.map((sticker) => sticker.kind));
  const withdrawn = pack.origin.kind === "community" && pack.origin.withdrawn !== null;
  const checks: CheckUi[] = [
    { key: "count", ok: n >= 3 && n <= 30, text: n >= 3 && n <= 30 ? `${n} stickers (de 3 a 30)` : `Hay ${n} stickers: un paquete lleva de 3 a 30` },
    { key: "kind", ok: kinds.size <= 1, text: kinds.size <= 1 ? "Todos del mismo tipo" : "Mezcla stickers estáticos y animados" },
    {
      key: "emojis",
      ok: pack.stickers.every((sticker) => sticker.emojis.length > 0),
      text: pack.stickers.every((sticker) => sticker.emojis.length > 0) ? "Todos tienen emoji" : "Hay stickers sin emoji",
    },
    { key: "icon", ok: pack.icon !== null, text: pack.icon !== null ? "Tiene icono" : "Falta el icono de la bandeja" },
    {
      key: "texts",
      ok: pack.name.trim() !== "" && pack.author.trim() !== "",
      text: pack.name.trim() !== "" && pack.author.trim() !== "" ? "Nombre y autor" : "Falta el autor",
    },
    { key: "withdrawn", ok: !withdrawn, text: withdrawn ? "Este paquete se retiró del catálogo" : "No está retirado" },
  ];
  return { ...pack, checks, exportable: checks.every((check) => check.ok) };
}

const community = (id: string, version: number, license: string, author: string, withdrawn: string | null = null): OriginUi => ({
  kind: "community",
  id,
  version,
  license,
  original_author: author,
  withdrawn,
});

export function demoLibrary(paths: readonly string[]): LibraryUi {
  const take = (from: number, howMany: number): string[] => paths.slice(from, from + howMany);
  return {
    categories: [
      { id: 1, name: "Anime" },
      { id: 2, name: "Familia" },
      { id: 3, name: "Memes" },
      { id: 4, name: "Animales" },
    ],
    packs: [
      demoPack(1, "Gatos que saltan", 4, "animated", take(0, 12), { author: "Ana", favorite: true, published: 1, modified: Now }),
      demoPack(2, "Reacciones", 3, "animated", take(3, 7), { author: "Ana", modified: Now - 600 }),
      demoPack(3, "Fotos del verano", 2, "static", take(6, 2), { author: "Ana" }),
      demoPack(4, "Ojos grandes", 1, "animated", take(1, 5), { author: "Ana" }),
      demoPack(5, "Sin estrenar", null, null, [], { author: "Ana" }),
      demoPack(6, "Garabatos de gato", 4, "static", take(2, 8), { author: "Lucía", origin: community("gatos-garabato", 1, "CC-BY-4.0", "Lucía"), published: 1 }),
      demoPack(7, "Dibujos del parque", 2, "static", take(4, 6), {
        author: "Marcos",
        origin: community("parque-dibujado", 1, "CC0-1.0", "Marcos", "Retirado por una reclamación de derechos de autor"),
      }),
    ],
    notice: null,
  };
}

function remote(
  id: string,
  name: string,
  author: string,
  category: string,
  animated: boolean,
  added: string,
  stickers: number,
  icon: string | null,
  changes: Partial<CommunityPackUi> = {},
): CommunityPackUi {
  return {
    id,
    version: 1,
    name,
    author,
    license: "CC0-1.0",
    category,
    tags: [],
    animated,
    added,
    stickers,
    icon,
    downloaded: null,
    updatable: false,
    ...changes,
  };
}

export function demoCatalog(paths: readonly string[]): CatalogUi {
  const icon = (index: number): string | null => paths[index % Math.max(paths.length, 1)] ?? null;
  return {
    categories: [
      { id: "animales", name: "Animales" },
      { id: "comida", name: "Comida" },
      { id: "memes", name: "Memes" },
      { id: "emociones", name: "Emociones" },
    ],
    packs: [
      remote("gatos-garabato", "Garabatos de gato", "Lucía", "animales", false, "2026-11-20", 8, icon(2), {
        downloaded: 6,
        updatable: true,
        version: 2,
        license: "CC-BY-4.0",
        tags: ["gato", "garabato"],
      }),
      remote("perros-pixel", "Perros de píxel", "Hugo", "animales", true, "2026-11-18", 12, icon(5), { tags: ["perro", "pixel"] }),
      remote("tacos-felices", "Tacos felices", "Inés", "comida", false, "2026-11-15", 10, icon(7), { license: "CC-BY-SA-4.0", tags: ["comida", "taco"] }),
      remote("fruta-con-cara", "Fruta con cara", "Inés", "comida", true, "2026-10-02", 14, icon(9)),
      remote("memes-de-oficina", "Memes de oficina", "Pablo", "memes", false, "2026-11-21", 20, icon(1), { tags: ["trabajo"] }),
      remote("cara-de-lunes", "Cara de lunes", "Pablo", "emociones", true, "2026-09-12", 9, icon(4), { tags: ["sueño"] }),
      remote("parque-dibujado", "Dibujos del parque", "Marcos", "emociones", false, "2026-08-01", 6, icon(3), { downloaded: 7 }),
      remote("nubes-simpaticas", "Nubes simpáticas", "Eva", "animales", false, "2026-11-05", 5, null),
    ],
    updated: Now,
    offline: false,
    notice_accepted: false,
  };
}

const pause = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

export interface CatalogDemo {
  readonly api: CatalogApi;
  library(): LibraryUi;
  setLibrary(library: LibraryUi): void;
  onChange(listener: () => void): void;
  withoutRed(state: "con-red" | "caché" | "sin-caché"): void;
}

export function createCatalogDemo(paths: readonly string[], initial: LibraryUi, delay = 250): CatalogDemo {
  let library = initial;
  let catalog = demoCatalog(paths);
  let red: "con-red" | "caché" | "sin-caché" = "con-red";
  let cancelled = false;
  const listeners: (() => void)[] = [];

  const change = (fresh: LibraryUi): LibraryUi => {
    library = fresh;
    for (const listener of listeners) listener();
    return fresh;
  };
  const withPack = (id: number, changes: (pack: PackUi) => PackUi): LibraryUi =>
    change({
      ...library,
      packs: library.packs.map((pack) => (pack.id === id ? withChecks({ ...changes(pack), modified: Now }) : pack)),
    });
  const idNext = (): number => Math.max(0, ...library.packs.map((pack) => pack.id)) + 1;

  const api: CatalogApi = {
    async setEmojis(pack, sticker, emojis) {
      await pause(delay / 2);
      return withPack(pack, (current) => ({
        ...current,
        stickers: current.stickers.map((candidate) => (candidate.id === sticker ? { ...candidate, emojis } : candidate)),
      }));
    },
    async markFavorite(target, favorite) {
      await pause(delay / 2);
      if (target.kind === "pack") return withPack(target.id, (current) => ({ ...current, favorite }));
      return withPack(target.pack, (current) => ({
        ...current,
        stickers: current.stickers.map((candidate) => (candidate.id === target.id ? { ...candidate, favorite } : candidate)),
      }));
    },
    async reorderStickers(pack, order) {
      await pause(delay / 2);
      return withPack(pack, (current) => ({
        ...current,
        stickers: order.flatMap((id) => current.stickers.filter((sticker) => sticker.id === id)),
      }));
    },
    async createPackFromFavorites(name) {
      await pause(delay);
      const chosen = library.packs.flatMap((pack) => pack.stickers.filter((sticker) => sticker.favorite));
      const id = idNext();
      const fresh = demoPack(id, name, null, "animated", chosen.slice(0, 30).map((sticker) => sticker.path), { author: "Yo" });
      return { library: change({ ...library, packs: [...library.packs, fresh] }), pack: id };
    },
    async exportBackup() {
      await pause(delay);
      return "stickers-copia-2026-11-22.zip";
    },
    async restoreBackup() {
      await pause(delay);
      return null;
    },
    async editPack(id, edit) {
      await pause(delay / 2);
      return withPack(id, (current) => ({ ...current, ...edit }));
    },
    async deletePack(id) {
      await pause(delay / 2);
      const removed = library.packs.find((pack) => pack.id === id);
      if (removed?.origin.kind === "community") {
        const idCommunity = removed.origin.id;
        catalog = {
          ...catalog,
          packs: catalog.packs.map((pack) => (pack.id === idCommunity ? { ...pack, downloaded: null, updatable: false } : pack)),
        };
      }
      return change({ ...library, packs: library.packs.filter((pack) => pack.id !== id) });
    },
    async removeSticker(pack, sticker) {
      await pause(delay / 2);
      return withPack(pack, (current) => ({ ...current, stickers: current.stickers.filter((candidate) => candidate.id !== sticker) }));
    },
    async exportPack(id) {
      await pause(delay);
      return `${library.packs.find((pack) => pack.id === id)?.name ?? "pack"}.wastickers`;
    },
    async exportSticker(pack, sticker) {
      await pause(delay);
      return `${library.packs.find((candidate) => candidate.id === pack)?.name ?? "pack"}-${sticker}.webp`;
    },
    async shareSticker() {
      await pause(delay);
    },
    async sendStickerToWhatsapp() {
      await pause(delay);
      return { result: "added", notice: null };
    },
    async addToWhatsapp(pack) {
      await pause(delay);
      withPack(pack, (current) => ({ ...current, published: current.origin.kind === "community" ? current.origin.version : (current.published ?? 0) + 1 }));
      return { result: "added", notice: null };
    },
    async catalog() {
      await pause(delay);
      if (red === "sin-caché") throw { kind: "other", message: "Sin conexión y sin catálogo guardado" };
      return { ...catalog, offline: red === "caché" };
    },
    async communityDetail(id) {
      await pause(delay);
      const pack = catalog.packs.find((candidate) => candidate.id === id);
      if (pack === undefined) throw { kind: "other", message: "Este paquete ya no está en el catálogo" };
      const samples = Array.from({ length: Math.min(pack.stickers, 12) }, (_, index) => ({
        path: paths[(index + pack.name.length) % Math.max(paths.length, 1)] ?? "",
        emojis: [EMOJIS[(index + pack.name.length) % EMOJIS.length] ?? "🙂"],
      }));
      return { pack, stickers: samples };
    },
    async acceptCommunityNotice() {
      catalog = { ...catalog, notice_accepted: true };
    },
    async downloadFromCommunity(id, onProgress) {
      cancelled = false;
      const pack = catalog.packs.find((candidate) => candidate.id === id);
      if (pack === undefined) throw { kind: "other", message: "Este paquete ya no está en el catálogo" };
      for (let done = 0; done <= pack.stickers; done++) {
        if (cancelled) throw { kind: "cancelled", message: "Descarga cancelada" };
        onProgress({ kind: "downloading", done, total: pack.stickers });
        await pause(120);
      }
      onProgress({ kind: "verifying" });
      await pause(400);
      const backup = library.packs.find((candidate) => candidate.id === pack.downloaded);
      const localId = backup?.id ?? idNext();
      const newPack = demoPack(
        localId,
        pack.name,
        null,
        pack.animated ? "animated" : "static",
        paths.slice(0, pack.stickers),
        { origin: community(pack.id, pack.version, pack.license, pack.author), favorite: backup?.favorite ?? false },
      );
      catalog = {
        ...catalog,
        packs: catalog.packs.map((candidate) => (candidate.id === id ? { ...candidate, downloaded: localId, updatable: false } : candidate)),
      };
      const newLibrary = { ...library, packs: [...library.packs.filter((candidate) => candidate.id !== localId), newPack] };
      return { library: change(newLibrary), pack: localId };
    },
    async cancelCommunityDownload() {
      cancelled = true;
    },
    async proposeToCommunity(pack) {
      await pause(delay);
      return `propuesta-${pack}.zip`;
    },
  };

  return {
    api,
    library: () => library,
    setLibrary: (fresh) => void change(fresh),
    onChange: (listener) => void listeners.push(listener),
    withoutRed: (state) => void (red = state),
  };
}
