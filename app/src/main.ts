import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { convertFileSrc } from "@tauri-apps/api/core";

import type { Context } from "./context.ts";
import type { EntryUi } from "./contract.ts";
import { asErrorUi } from "./contract.ts";
import { backAction } from "./mobile.ts";
import { element } from "./dom.ts";
import { editionBadge, githubEdition } from "./edition.ts";
import { platform } from "./platform.ts";
import { openSavedFolder, openWhatsapp } from "./contract-mobile.ts";
import { errorMessage } from "./messages.ts";
import type { SavedFolder } from "./saved-folder.ts";
import { back, changeSection, goToRoot, type Navigation, initialNavigation } from "./navigation.ts";
import { apiGifsReal } from "./api-gifs.ts";
import { apiReal } from "./catalog-api.ts";
import { createGifSearchSection } from "./gif-search-section.ts";
import { createCatalogSection } from "./catalog-section.ts";
import { createKlipySection } from "./klipy-section.ts";
import { createBackupSection } from "./backup-section.ts";
import { createExtensionsSection } from "./extensions-section.ts";
import { createDetailSection } from "./detail-section.ts";
import { createFavoritesSection } from "./favorites-section.ts";
import { createDownloadSection } from "./download-section.ts";
import { createEditorSection } from "./editor-section.ts";
import { createGifsSection } from "./gifs-section.ts";
import { createStickersSection } from "./stickers-section.ts";
import { createShellView } from "./shell-view.ts";

function startApp(): void {
  let nav: Navigation = initialNavigation();
  let notice: string | null = null;
  let noticeFolder: SavedFolder | null = null;
  const entries = new Map<number, EntryUi>();
  const layers: (() => boolean)[] = [];

  const shell = createShellView({
    goToRoot: (root) => context.navigate(goToRoot(nav, root)),
    goToCreate: () => context.navigate(goToRoot(nav, "create")),
    changeSection: (section) => context.navigate(changeSection(nav, section)),
    closeNotice: () => context.notify(null),
    openFolder: (folder) => context.openFolder(folder),
  });

  const context: Context = {
    nav: () => nav,
    navigate(fresh) {
      nav = fresh;
      shell.render({ nav, notice, noticeFolder });
      stickers.onNavigate(nav);
      catalog.onNavigate(nav);
      detail.onNavigate(nav);
      favorites.onNavigate(nav);
      editor.onNavigate(nav);
      searchGif.onNavigate(nav);
    },
    notify(text, folder) {
      notice = text;
      noticeFolder = folder ?? null;
      shell.render({ nav, notice, noticeFolder });
    },
    openFolder(folder) {
      openSavedFolder(folder).catch((failure: unknown) => context.notify(errorMessage(asErrorUi(failure))));
    },
    openWhatsapp() {
      openWhatsapp().catch((failure: unknown) => context.notify(errorMessage(asErrorUi(failure))));
    },
    entry: (id) => entries.get(id),
    remember: (entry) => void entries.set(entry.id, entry),
    forget: (id) => void entries.delete(id),
    registerLayer: (close) => void layers.push(close),
    closeLayer: () => layers.some((close) => close()),
  };

  const stickers = createStickersSection(context, convertFileSrc);
  const gifs = createGifsSection(context);
  const searchGif = createGifSearchSection(context, { api: apiGifsReal, fileUrl: convertFileSrc });
  const editor = createEditorSection(context, {
    fileUrl: convertFileSrc,
    library: stickers.library,
    setLibrary: stickers.setLibrary,
    onGifCreate: gifs.addCreated,
  });
  const klipy = createKlipySection(context, {
    api: apiGifsReal,
    fileUrl: convertFileSrc,
    libraryState: stickers.libraryState,
    setLibrary: stickers.setLibrary,
  });
  const catalog = createCatalogSection(context, {
    klipy,
    api: apiReal,
    fileUrl: convertFileSrc,
    libraryState: stickers.libraryState,
    reloadLibrary: stickers.reload,
    onLibraryChange: stickers.onLibraryChange,
  });
  const detail = createDetailSection(context, {
    api: apiReal,
    platform,
    fileUrl: convertFileSrc,
    libraryState: stickers.libraryState,
    setLibrary: stickers.setLibrary,
    reloadLibrary: stickers.reload,
    onLibraryChange: stickers.onLibraryChange,
    catalog: catalog.catalog,
    reloadCatalog: catalog.reload,
  });
  const favorites = createFavoritesSection(context, {
    api: apiReal,
    fileUrl: convertFileSrc,
    libraryState: stickers.libraryState,
    setLibrary: stickers.setLibrary,
    onLibraryChange: stickers.onLibraryChange,
    catalog: catalog.catalog,
  });
  createBackupSection({ api: apiReal, setLibrary: stickers.setLibrary });
  const badge = editionBadge(import.meta.env?.VITE_EDITION);
  if (badge !== null) {
    const label = element("edition-badge");
    label.textContent = badge;
    label.hidden = false;
  }
  if (githubEdition) createExtensionsSection();
  else element("section-extensions").remove();
  catalog.onChange(detail.onCatalogChange);
  catalog.onChange(favorites.onCatalogChange);
  createDownloadSection(context, editor.creating);

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || event.defaultPrevented || editor.creating()) return;
    if (context.closeLayer()) return event.preventDefault();
    const previous = back(nav);
    if (previous === null) return;
    event.preventDefault();
    context.navigate(previous);
  });

  if (platform === "android") {
    onBackButtonPress(() => {
      if (editor.creating()) return context.notify("Termina o cancela lo que se está creando antes de volver.");
      if (context.closeLayer()) return;
      const action = backAction(platform, nav);
      if (action.kind === "navigate") context.navigate(action.nav);
      else if (action.kind === "exit") void exit(0);
    }).catch((failure: unknown) => {
      context.notify(`El botón atrás no responde: ${asErrorUi(failure).message}`);
    });
  }

  shell.render({ nav, notice, noticeFolder });
}

const demoState = document.querySelector<HTMLMetaElement>('meta[name="demo-state"]');
if ("__TAURI_INTERNALS__" in window && demoState === null) {
  startApp();
} else {
  import("./demo.ts")
    .then(({ startDemo }) => startDemo(demoState?.content ?? null))
    .catch((failure: unknown) => console.error("no se pudo cargar la demostración", failure));
}
