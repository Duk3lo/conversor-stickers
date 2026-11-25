import { localStore } from "./store.ts";
import type { CatalogApi } from "./catalog-api.ts";
import { search, withSearch, saveHistory, readHistory, withoutSearch } from "./search.ts";
import { keyByPath, startModel, cards, cardsOfCategory, updatedText } from "./catalog.ts";
import type { Context } from "./context.ts";
import { asErrorUi } from "./contract.ts";
import type { LibraryUi } from "./contract.ts";
import type { CatalogUi } from "./contract-community.ts";
import { errorMessage } from "./messages.ts";
import type { KlipySection } from "./klipy-section.ts";
import { back, goTo, goToRoot, type Navigation, panelVisible, replace, type Path } from "./navigation.ts";
import { createSearchView } from "./search-view.ts";
import { createCategoryView } from "./category-view.ts";
import { createCommunityView, type CommunityView } from "./community-view.ts";
import { createExplorerView } from "./explorer-view.ts";
import type { ExplorerActions, Load, SearchView, CategoryView, ExplorerView } from "./views.ts";

export interface CatalogDependencies {
  readonly api: CatalogApi;
  readonly fileUrl: (path: string) => string;
  readonly libraryState: () => Load<LibraryUi>;
  readonly reloadLibrary: () => void;
  readonly onLibraryChange: (listener: () => void) => void;
  readonly klipy: KlipySection;
}

export interface CatalogSection {
  onNavigate(nav: Navigation): void;
  reload(force: boolean): void;
  catalog(): CatalogUi | null;
  onChange(listener: () => void): void;
}

export function createCatalogSection(context: Context, deps: CatalogDependencies): CatalogSection {
  let catalog: Load<CatalogUi> = { kind: "loading" };
  let startChip: string | null = null;
  const listeners: (() => void)[] = [];
  let history = readHistory(localStore());

  async function loadCatalog(force: boolean): Promise<void> {
    const previous = catalog;
    if (previous.kind !== "ready") catalog = { kind: "loading" };
    renderVisible();
    try {
      catalog = { kind: "ready", value: await deps.api.catalog(force) };
    } catch (failure) {
      const message = errorMessage(asErrorUi(failure));
      if (previous.kind === "ready") {
        catalog = previous;
        context.notify(`No se pudo actualizar la comunidad: ${message}`);
      } else {
        catalog = { kind: "error", message };
      }
    }
    renderVisible();
    for (const listener of listeners) listener();
  }

  function readCatalog(): CatalogUi | null {
    return catalog.kind === "ready" ? catalog.value : null;
  }

  function notices(): Pick<ExplorerView, "catalogError" | "withoutConnection" | "history" | "klipy"> {
    const read = readCatalog();
    return {
      catalogError: catalog.kind === "error" ? catalog.message : null,
      withoutConnection: read?.offline ? updatedText(read.updated) : null,
      history,
      klipy: deps.klipy.rows(),
    };
  }

  function startView(): ExplorerView {
    const library = deps.libraryState();
    const state: ExplorerView["state"] =
      library.kind === "ready"
        ? { kind: "ready", value: startModel(library.value, readCatalog(), startChip) }
        : library;
    return { state, ...notices() };
  }

  function communityView(): CommunityView {
    const read = readCatalog();
    const library = deps.libraryState();
    return {
      catalog,
      library: library.kind === "ready" ? library.value : null,
      updated: read === null ? null : updatedText(read.updated),
      offline: read?.offline ?? false,
    };
  }

  function goToSearch(text: string): void {
    const query = text.trim().replace(/\s+/g, " ");
    history = withSearch(history, query);
    saveHistory(localStore(), history);
    const nav = context.nav();
    const path: Path = { kind: "search", query };
    context.navigate(nav.path.kind === "search" ? replace(nav, path) : goTo(nav, path));
  }

  function searchView(): SearchView {
    const path = context.nav().path;
    const query = path.kind === "search" ? path.query : "";
    const library = deps.libraryState();
    const klipy = deps.klipy.searchRow();
    if (library.kind === "loading") return { query, history, klipy, state: { kind: "loading" } };
    return {
      query,
      history,
      klipy,
      state: { kind: "ready", value: search(query, library.kind === "ready" ? library.value : null, readCatalog()) },
    };
  }

  const searchPanel = createSearchView(
    {
      back: () => context.navigate(back(context.nav()) ?? goToRoot(context.nav(), "start")),
      search: goToSearch,
      removeFromHistory(text) {
        history = withoutSearch(history, text);
        saveHistory(localStore(), history);
        renderVisible();
      },
      openPack: (path: Path) => context.navigate(goTo(context.nav(), path)),
      openSticker: (sticker) => context.navigate(goTo(context.nav(), { kind: "pack", origin: "local", id: String(sticker.pack) })),
      openKlipySticker: deps.klipy.open,
      plusSeeKlipy: deps.klipy.seePlus,
    },
    deps.fileUrl,
  );

  function actions(): ExplorerActions {
    return {
      pickChip(key) {
        startChip = key;
        renderVisible();
      },
      openPack: (path: Path) => context.navigate(goTo(context.nav(), path)),
      seeAll: (path: Path) => context.navigate(goTo(context.nav(), path)),
      search: (text) => goToSearch(text),
      update() {
        if (deps.libraryState().kind === "error") deps.reloadLibrary();
        deps.klipy.reload();
        void loadCatalog(true);
      },
      goToCreate: () => context.navigate(goToRoot(context.nav(), "create")),
      goToCommunity: () => context.navigate(goToRoot(context.nav(), "community")),
      openKlipySticker: deps.klipy.open,
      plusSeeKlipy: deps.klipy.seePlus,
    };
  }

  const start = createExplorerView(actions(), deps.fileUrl);
  const community = createCommunityView(
    {
      openPack: (path: Path) => context.navigate(goTo(context.nav(), path)),
      update() {
        void loadCatalog(true);
      },
    },
    deps.fileUrl,
  );

  function categoryView(): CategoryView {
    const path = context.nav().path;
    if (path.kind !== "category") return { state: { kind: "loading" } };
    const library = deps.libraryState();
    if (library.kind === "loading" || (catalog.kind === "loading" && path.origin === "community")) {
      return { state: { kind: "loading" } };
    }
    const readLibrary = library.kind === "ready" ? library.value : null;
    const category = keyByPath(path.origin, path.id, readLibrary, readCatalog());
    if (category === null) return { state: { kind: "error", message: "Esa categoría ya no existe." } };
    const list = cardsOfCategory(cards(readLibrary, readCatalog()), category.key);
    return { state: { kind: "ready", value: { name: category.name, cards: list } } };
  }

  const category = createCategoryView(
    {
      back: () => context.navigate(back(context.nav()) ?? goToRoot(context.nav(), "start")),
      openPack: (path: Path) => context.navigate(goTo(context.nav(), path)),
    },
    deps.fileUrl,
  );

  function renderVisible(): void {
    const panel = panelVisible(context.nav());
    if (panel === "start" || panel === "search") deps.klipy.start();
    if (panel === "search") {
      const path = context.nav().path;
      deps.klipy.search(path.kind === "search" ? path.query : "");
    }
    switch (panel) {
      case "start":
        return start.render(startView());
      case "community":
        return community.render(communityView());
      case "category":
        return category.render(categoryView());
      case "search":
        return searchPanel.render(searchView());
      default:
    }
  }

  deps.onLibraryChange(renderVisible);
  deps.klipy.onChange(renderVisible);
  renderVisible();
  void loadCatalog(false);

  return {
    onNavigate: renderVisible,
    reload: (force) => void loadCatalog(force),
    catalog: readCatalog,
    onChange: (listener) => void listeners.push(listener),
  };
}
