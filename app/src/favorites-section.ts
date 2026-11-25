import type { CatalogApi } from "./catalog-api.ts";
import { favoritesModel } from "./search.ts";
import type { Context } from "./context.ts";
import { type LibraryUi, asErrorUi } from "./contract.ts";
import type { CatalogUi } from "./contract-community.ts";
import { errorMessage } from "./messages.ts";
import { goTo, goToRoot, type Navigation, panelVisible } from "./navigation.ts";
import { validatePackName } from "./packs.ts";
import { createFavoritesView } from "./favorites-view.ts";
import type { Load, FavoritesView } from "./views.ts";

export interface FavoritesDependencies {
  readonly api: CatalogApi;
  readonly fileUrl: (path: string) => string;
  readonly libraryState: () => Load<LibraryUi>;
  readonly setLibrary: (library: LibraryUi) => void;
  readonly onLibraryChange: (listener: () => void) => void;
  readonly catalog: () => CatalogUi | null;
}

export interface FavoritesSection {
  onNavigate(nav: Navigation): void;
  onCatalogChange(): void;
}

export function createFavoritesSection(context: Context, deps: FavoritesDependencies): FavoritesSection {
  let form = false;
  let formError: string | null = null;
  let busy = false;
  let error: string | null = null;

  function view(): FavoritesView {
    const library = deps.libraryState();
    const state: FavoritesView["state"] =
      library.kind === "ready" ? { kind: "ready", value: favoritesModel(library.value, deps.catalog()) } : library;
    return { state, form, formError, busy, error };
  }

  function render(): void {
    if (panelVisible(context.nav()) === "favorites") screen.render(view());
  }

  async function createPack(text: string): Promise<void> {
    const validation = validatePackName(text);
    if (!validation.valid) {
      formError = validation.reason;
      return render();
    }
    busy = true;
    formError = null;
    error = null;
    render();
    try {
      const created = await deps.api.createPackFromFavorites(validation.value);
      form = false;
      deps.setLibrary(created.library);
      context.navigate(goTo(context.nav(), { kind: "pack", origin: "local", id: String(created.pack) }));
    } catch (failure) {
      error = errorMessage(asErrorUi(failure));
    } finally {
      busy = false;
      render();
    }
  }

  const screen = createFavoritesView(
    {
      openPack: (path) => context.navigate(goTo(context.nav(), path)),
      openSticker: (sticker) => context.navigate(goTo(context.nav(), { kind: "pack", origin: "local", id: String(sticker.pack) })),
      openForm() {
        form = true;
        formError = null;
        render();
      },
      closeForm() {
        form = false;
        formError = null;
        render();
      },
      createPack: (name) => void createPack(name),
      goToStart: () => context.navigate(goToRoot(context.nav(), "start")),
    },
    deps.fileUrl,
  );
  deps.onLibraryChange(render);

  return { onNavigate: render, onCatalogChange: render };
}
