import type { ApiGifs } from "./api-gifs.ts";
import type { Context } from "./context.ts";
import { asErrorUi } from "./contract.ts";
import type { LibraryUi, CreatedPack } from "./contract.ts";
import type { StickerKlipyUi } from "./contract-gifs.ts";
import { packDestination, type KlipyRowState, type KlipyRow, initialRows, visibleRow, joinStickers } from "./klipy.ts";
import { errorMessage } from "./messages.ts";
import { compatiblePacks } from "./packs.ts";
import { createKlipyStickerView } from "./klipy-sticker-view.ts";
import type { Load, KlipyStickerView } from "./views.ts";

export interface KlipyDependencies {
  readonly api: ApiGifs;
  readonly fileUrl: (path: string) => string;
  readonly libraryState: () => Load<LibraryUi>;
  readonly setLibrary: (library: LibraryUi) => void;
}

export interface KlipySection {
  start(): void;
  reload(): void;
  rows(): readonly KlipyRow[];
  search(query: string): void;
  searchRow(): KlipyRow | null;
  seePlus(key: string): void;
  open(sticker: StickerKlipyUi): void;
  onChange(listener: () => void): void;
}

const SEARCH_KEY = "search";

export function createKlipySection(context: Context, deps: KlipyDependencies): KlipySection {
  let cover: readonly KlipyRow[] = [];
  let started = false;
  let coverLoad = 0;
  let query: string | null = null;
  let search: KlipyRow | null = null;
  let searchLoad = 0;
  let open: KlipyStickerView | null = null;
  const listeners: (() => void)[] = [];

  function notify(): void {
    for (const listener of listeners) listener();
  }

  function listEs(state: KlipyRowState): state is Extract<KlipyRowState, { kind: "ready" }> {
    return state.kind === "ready";
  }

  async function request(text: string | null, category: string | null, page: number) {
    return deps.api.stickersKlipy(text, category, page);
  }

  function setInCover(key: string, state: KlipyRowState): void {
    cover = cover.map((row) => (row.key === key ? { ...row, state } : row));
    notify();
  }

  async function loadCoverRow(row: KlipyRow, mine: number): Promise<void> {
    let state: KlipyRowState;
    try {
      const page = await request(null, row.category, 1);
      state = { kind: "ready", results: page.results, next: page.next, loadingPlus: false, plusError: null };
    } catch {
      state = { kind: "hidden" };
    }
    if (mine === coverLoad) setInCover(row.key, state);
  }

  async function loadCover(): Promise<void> {
    const mine = ++coverLoad;
    const [trending] = initialRows([]);
    if (trending === undefined) return;
    cover = [trending];
    notify();
    const categories = deps.api.klipyCategories().catch(() => []);
    await loadCoverRow(trending, mine);
    const list = await categories;
    if (mine !== coverLoad) return;
    const fresh = initialRows(list).slice(1);
    cover = [...cover, ...fresh];
    notify();
    await Promise.all(fresh.map((row) => loadCoverRow(row, mine)));
  }

  async function loadSearch(text: string): Promise<void> {
    const mine = ++searchLoad;
    search = { key: SEARCH_KEY, title: `Para «${text}»`, category: null, state: { kind: "loading" } };
    notify();
    let state: KlipyRowState;
    try {
      const page = await request(text, null, 1);
      state = { kind: "ready", results: page.results, next: page.next, loadingPlus: false, plusError: null };
    } catch {
      state = { kind: "hidden" };
    }
    if (mine !== searchLoad || search === null) return;
    search = { ...search, state };
    notify();
  }

  function rowByKey(key: string): KlipyRow | undefined {
    return key === SEARCH_KEY ? (search ?? undefined) : cover.find((row) => row.key === key);
  }

  function setRow(key: string, state: KlipyRowState): void {
    if (key === SEARCH_KEY) {
      if (search !== null) search = { ...search, state };
      notify();
    } else setInCover(key, state);
  }

  function generationOf(key: string): number {
    return key === SEARCH_KEY ? searchLoad : coverLoad;
  }

  async function seePlus(key: string): Promise<void> {
    const row = rowByKey(key);
    if (row === undefined || !listEs(row.state) || row.state.next === null || row.state.loadingPlus) return;
    const current = row.state;
    const generation = generationOf(key);
    setRow(key, { ...current, loadingPlus: true, plusError: null });
    try {
      const page = await request(key === SEARCH_KEY ? query : null, row.category, current.next ?? 1);
      if (generation !== generationOf(key)) return;
      setRow(key, {
        kind: "ready",
        results: joinStickers(current.results, page.results),
        next: page.next,
        loadingPlus: false,
        plusError: null,
      });
    } catch (error) {
      if (generation !== generationOf(key)) return;
      setRow(key, { ...current, loadingPlus: false, plusError: errorMessage(asErrorUi(error)) });
    }
  }

  function renderOpen(): void {
    large.render(open);
  }

  function close(): boolean {
    if (open === null) return false;
    if (open.busy) return true;
    open = null;
    renderOpen();
    return true;
  }

  function library(): LibraryUi | null {
    const state = deps.libraryState();
    return state.kind === "ready" ? state.value : null;
  }

  async function add(choice: string, newName: string): Promise<void> {
    if (open === null || open.busy) return;
    const sticker = open.sticker;
    const destination = packDestination(choice, newName, library()?.packs ?? []);
    if (!destination.ok) {
      open = { ...open, error: destination.reason };
      return renderOpen();
    }
    open = { ...open, busy: true, error: null };
    renderOpen();
    let created: CreatedPack;
    try {
      created = await deps.api.addKlipySticker(sticker.id, destination.pack, destination.newName);
    } catch (error) {
      if (open !== null) open = { ...open, busy: false, error: errorMessage(asErrorUi(error)) };
      return renderOpen();
    }
    deps.setLibrary(created.library);
    open = null;
    renderOpen();
    const name = created.library.packs.find((pack) => pack.id === created.pack)?.name ?? "tu paquete";
    context.notify(`Sticker añadido a «${name}».`);
  }

  const large = createKlipyStickerView(
    {
      close: () => void close(),
      add: (choice, newName) => void add(choice, newName),
    },
    deps.fileUrl,
  );
  context.registerLayer(close);

  return {
    start() {
      if (started) return;
      started = true;
      void loadCover();
    },
    reload() {
      started = true;
      void loadCover();
      if (query !== null) void loadSearch(query);
    },
    rows: () => cover.filter(visibleRow),
    search(text) {
      if (text === query) return;
      query = text;
      if (text === "") {
        searchLoad++;
        search = null;
        return;
      }
      void loadSearch(text);
    },
    searchRow: () => (search !== null && visibleRow(search) ? search : null),
    seePlus: (key) => void seePlus(key),
    open(sticker) {
      const compatible = compatiblePacks(library() ?? { categories: [], packs: [], notice: null }, sticker.animated ? "animated" : "static");
      open = { sticker, packs: compatible, busy: false, error: null };
      renderOpen();
    },
    onChange: (listener) => void listeners.push(listener),
  };
}
