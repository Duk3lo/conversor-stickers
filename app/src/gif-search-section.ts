import type { ApiGifs } from "./api-gifs.ts";
import {
  visibleAttribution,
  type GifSearch,
  classifyError,
  createDelay,
  WRITING_DELAY_MS,
  queryText,
  joinPage,
  validateKey,
} from "./gif-search.ts";
import type { Context } from "./context.ts";
import { asErrorUi } from "./contract.ts";
import type { GifProvider } from "./contract-gifs.ts";
import { errorMessage } from "./messages.ts";
import { openEditor, type Navigation } from "./navigation.ts";
import { createGifSearchView } from "./gif-search-view.ts";
import type { GifSearchView } from "./views.ts";

export interface GifSearchDependencies {
  readonly api: ApiGifs;
  readonly fileUrl: (path: string) => string;
}

export interface GifSearchSection {
  onNavigate(nav: Navigation): void;
  reload(): void;
}

export function createGifSearchSection(context: Context, deps: GifSearchDependencies): GifSearchSection {
  let view: GifSearchView = {
    provider: { kind: "loading" },
    settings: false,
    savingSettings: false,
    settingsError: null,
    text: "",
    search: { kind: "loading" },
    using: null,
    useError: null,
  };
  let started = false;
  let generation = 0;

  function set(changes: Partial<GifSearchView>): void {
    view = { ...view, ...changes };
    screen.render(view);
  }

  function currentQuery(): string | null {
    return queryText(view.text);
  }

  function failure(error: unknown): GifSearch {
    const message = errorMessage(asErrorUi(error));
    return { kind: "error", reason: classifyError(message), message };
  }

  async function loadProvider(): Promise<void> {
    set({ provider: { kind: "loading" } });
    try {
      const value = await deps.api.gifProvider();
      set({ provider: { kind: "ready", value }, settings: value.provider === null || !value.key_set });
    } catch (error) {
      set({ provider: { kind: "error", message: errorMessage(asErrorUi(error)) } });
    }
  }

  async function start(): Promise<void> {
    await loadProvider();
    if (view.provider.kind === "ready" && view.provider.value.key_set) await search();
    else set({ search: { kind: "error", reason: "without-key", message: "Falta la clave del proveedor" } });
  }

  async function search(): Promise<void> {
    delay.cancel();
    const mine = ++generation;
    set({ search: { kind: "loading" }, useError: null });
    try {
      const page = await deps.api.searchGifs(currentQuery(), 1);
      if (mine !== generation) return;
      const provider = view.provider.kind === "ready" ? view.provider.value.provider : null;
      set({
        search: {
          kind: "ready",
          results: page.results,
          next: page.next,
          attribution: visibleAttribution(provider, page.attribution),
          loadingPlus: false,
          plusError: null,
        },
      });
    } catch (error) {
      if (mine === generation) set({ search: failure(error) });
    }
  }

  async function loadPlus(): Promise<void> {
    const current = view.search;
    if (current.kind !== "ready" || current.next === null || current.loadingPlus) return;
    const mine = generation;
    set({ search: { ...current, loadingPlus: true, plusError: null } });
    try {
      const page = await deps.api.searchGifs(currentQuery(), current.next);
      if (mine !== generation || view.search.kind !== "ready") return;
      set({
        search: {
          ...view.search,
          results: joinPage(view.search.results, page.results),
          next: page.next,
          loadingPlus: false,
        },
      });
    } catch (error) {
      if (mine === generation && view.search.kind === "ready") {
        set({ search: { ...view.search, loadingPlus: false, plusError: errorMessage(asErrorUi(error)) } });
      }
    }
  }

  async function saveSettings(provider: GifProvider, key: string | null): Promise<void> {
    let value: string | null = null;
    if (key !== null) {
      const validation = validateKey(key);
      if (!validation.ok) return set({ settingsError: validation.reason });
      value = validation.value;
    }
    set({ savingSettings: true, settingsError: null });
    try {
      await deps.api.configureGifProvider(provider, value);
    } catch (error) {
      return set({ savingSettings: false, settingsError: errorMessage(asErrorUi(error)) });
    }
    set({ savingSettings: false });
    await start();
    if (view.provider.kind === "ready" && view.provider.value.key_set) set({ settings: false });
  }

  async function use(id: string): Promise<void> {
    if (view.using !== null) return;
    set({ using: id, useError: null });
    try {
      const entry = await deps.api.useGif(id);
      context.remember(entry);
      set({ using: null });
      context.navigate(openEditor(context.nav(), entry.id, "sticker"));
    } catch (error) {
      set({ using: null, useError: errorMessage(asErrorUi(error)) });
    }
  }

  const delay = createDelay<void>(() => void search(), WRITING_DELAY_MS);

  const screen = createGifSearchView(
    {
      toggleSettings: () => set({ settings: !view.settings, settingsError: null }),
      saveSettings: (provider, key) => void saveSettings(provider, key),
      write(text) {
        view = { ...view, text };
        delay.request();
      },
      search(text) {
        view = { ...view, text };
        void search();
      },
      loadPlus: () => void loadPlus(),
      retry: () => void start(),
      use: (id) => void use(id),
    },
    deps.fileUrl,
  );
  screen.render(view);

  return {
    reload: () => void start(),
    onNavigate(nav) {
      if (started || nav.section !== "search-gif" || nav.path.kind !== "create" || nav.editor !== null) return;
      started = true;
      void start();
    },
  };
}
