import { localStore } from "./store.ts";
import type { CatalogApi } from "./catalog-api.ts";
import type { Context } from "./context.ts";
import { type LibraryUi, asErrorUi, type PackUi } from "./contract.ts";
import type { CatalogUi, CommunityDetailUi, CommunityPackUi } from "./contract-community.ts";
import { type ActionKey, type DetailModel, communityModel, localModel, moveTo, moveOne } from "./detail.ts";
import { proposableNoReason, validateProposal } from "./community.ts";
import { withRecent, saveRecent, readRecent } from "./emojis.ts";
import { element } from "./dom.ts";
import { savedStickerNotice, sentToWhatsappNotice } from "./share.ts";
import { errorMessage } from "./messages.ts";
import type { Platform } from "./mobile.ts";
import { back, goTo, goToRoot, type Navigation, panelVisible, pathToText } from "./navigation.ts";
import { createDetailView } from "./detail-view.ts";
import { createEmojiSelector } from "./emoji-view.ts";
import type { DetailActions, Load, DetailView } from "./views.ts";

export interface DetailDependencies {
  readonly api: CatalogApi;
  readonly platform: Platform;
  readonly fileUrl: (path: string) => string;
  readonly libraryState: () => Load<LibraryUi>;
  readonly setLibrary: (library: LibraryUi) => void;
  readonly reloadLibrary: () => void;
  readonly onLibraryChange: (listener: () => void) => void;
  readonly catalog: () => CatalogUi | null;
  readonly reloadCatalog: (force: boolean) => void;
}

export interface DetailSection {
  onNavigate(nav: Navigation): void;
  onCatalogChange(): void;
}

type TransientState = Omit<DetailView, "state" | "categories" | "communityCategories" | "proposeReason">;

const Clean: TransientState = {
  busy: null,
  progress: null,
  error: null,
  notice: null,
  reordering: false,
  openSticker: null,
  confirmingDelete: false,
  focus: null,
  communityNotice: false,
  proposing: false,
};

export function createDetailSection(context: Context, deps: DetailDependencies): DetailSection {
  let transient: TransientState = Clean;
  let viewPath: string | null = null;
  const remote = new Map<string, Load<CommunityDetailUi>>();
  let recent = readRecent(localStore());
  let acceptedNotice = false;
  let pendingDownload: string | null = null;

  function localPack(): PackUi | null {
    const path = context.nav().path;
    const library = deps.libraryState();
    if (path.kind !== "pack" || path.origin !== "local" || library.kind !== "ready") return null;
    return library.value.packs.find((pack) => String(pack.id) === path.id) ?? null;
  }

  function packOfCatalog(id: string): CommunityPackUi | null {
    return deps.catalog()?.packs.find((pack) => pack.id === id) ?? null;
  }

  function readLibrary(): LibraryUi | null {
    const library = deps.libraryState();
    return library.kind === "ready" ? library.value : null;
  }

  function currentState(): Load<DetailModel> {
    const path = context.nav().path;
    if (path.kind !== "pack") return { kind: "loading" };
    if (path.origin === "local") {
      const library = deps.libraryState();
      if (library.kind !== "ready") return library;
      const pack = localPack();
      return pack === null
        ? { kind: "error", message: "Este paquete ya no está en tu biblioteca." }
        : { kind: "ready", value: localModel(pack, deps.platform) };
    }
    const detail = remote.get(path.id) ?? { kind: "loading" as const };
    if (detail.kind !== "ready") return detail;
    const pack = packOfCatalog(path.id) ?? detail.value.pack;
    return {
      kind: "ready",
      value: communityModel({ ...detail.value, pack }, readLibrary(), transient.progress !== null),
    };
  }

  function render(): void {
    if (panelVisible(context.nav()) !== "detail") return;
    const library = readLibrary();
    const own = localPack();
    screen.render({
      state: currentState(),
      categories: library?.categories ?? [],
      communityCategories: deps.catalog()?.categories ?? [],
      proposeReason: own === null ? null : proposableNoReason(own),
      ...transient,
    });
    if (transient.focus !== null) transient = { ...transient, focus: null };
  }

  function set(changes: Partial<TransientState>): void {
    transient = { ...transient, ...changes };
    render();
  }

  async function loadRemote(id: string): Promise<void> {
    remote.set(id, { kind: "loading" });
    render();
    try {
      remote.set(id, { kind: "ready", value: await deps.api.communityDetail(id) });
    } catch (failure) {
      remote.set(id, { kind: "error", message: errorMessage(asErrorUi(failure)) });
    }
    render();
  }

  async function run(text: string, task: () => Promise<void>): Promise<void> {
    set({ busy: text, error: null, notice: null });
    try {
      await task();
      set({ busy: null });
    } catch (failure) {
      set({ busy: null, error: errorMessage(asErrorUi(failure)) });
    }
  }

  function readModel(): DetailModel | null {
    const state = currentState();
    return state.kind === "ready" ? state.value : null;
  }

  async function download(id: string): Promise<void> {
    const pack = packOfCatalog(id);
    set({ busy: "Descargando…", progress: { kind: "downloading", done: 0, total: pack?.stickers ?? 0 }, error: null, notice: null });
    try {
      const created = await deps.api.downloadFromCommunity(id, (progress) => set({ progress }));
      deps.setLibrary(created.library);
      deps.reloadCatalog(false);
      set({ notice: `«${pack?.name ?? id}» está en tu biblioteca.` });
    } catch (failure) {
      const error = asErrorUi(failure);
      if (error.kind === "cancelled") set({ notice: "Descarga cancelada." });
      else set({ error: errorMessage(error) });
    } finally {
      set({ busy: null, progress: null });
    }
  }

  async function action(key: ActionKey): Promise<void> {
    const model = readModel();
    if (model === null) return;
    switch (key) {
      case "export":
        if (model.pack === null) return;
        return run("Exportando…", async () => {
          const name = await deps.api.exportPack(model.pack ?? 0);
          if (name !== null) transient = { ...transient, notice: `Exportado como ${name}.` };
        });
      case "add-whatsapp":
      case "update-whatsapp":
        if (model.pack === null) return;
        return run("Abriendo WhatsApp…", async () => {
          const { result, notice } = await deps.api.addToWhatsapp(model.pack ?? 0);
          const texts = [...(result === "added" ? ["Paquete añadido a WhatsApp."] : []), ...(notice === null ? [] : [notice])];
          transient = { ...transient, notice: texts.length === 0 ? null : texts.join(" ") };
          deps.reloadLibrary();
        });
      case "download":
      case "update":
        if (model.communityId === null) return;
        if (!acceptedNotice && deps.catalog()?.notice_accepted !== true) {
          pendingDownload = model.communityId;
          return set({ communityNotice: true, focus: "notice-si" });
        }
        return download(model.communityId);
      case "open-backup":
        if (model.pack !== null) context.navigate(goTo(context.nav(), { kind: "pack", origin: "local", id: String(model.pack) }));
        return;
    }
  }

  function withLibrary(text: string, command: (model: DetailModel & { pack: number }) => Promise<LibraryUi>, changes: Partial<TransientState> = {}): void {
    const model = readModel();
    if (model === null || model.pack === null) return;
    const withId = { ...model, pack: model.pack };
    void run(text, async () => {
      deps.setLibrary(await command(withId));
      transient = { ...transient, ...changes };
    });
  }

  function idsDeStickers(model: DetailModel): number[] {
    return model.stickers.flatMap((sticker) => (sticker.id === null ? [] : [sticker.id]));
  }

  function reorder(order: number[] | null, focus: string): void {
    if (order === null) return;
    withLibrary("Guardando el orden…", (model) => deps.api.reorderStickers(model.pack, order), { focus });
  }

  function stickerInView(): { pack: number; sticker: number } | null {
    const model = readModel();
    const index = transient.openSticker;
    const sticker = index === null ? undefined : model?.stickers[index];
    if (model === null || model.pack === null || sticker === undefined || sticker.id === null) return null;
    return { pack: model.pack, sticker: sticker.id };
  }

  const actions: DetailActions = {
    back() {
      const previous = back(context.nav());
      context.navigate(previous ?? goToRoot(context.nav(), "start"));
    },
    retry() {
      const path = context.nav().path;
      if (path.kind === "pack" && path.origin === "community") void loadRemote(path.id);
      else deps.reloadLibrary();
    },
    action: (key) => void action(key),
    toggleFavorite() {
      const model = readModel();
      if (model?.favorite == null) return;
      const favorite = !model.favorite;
      withLibrary("Guardando…", (withId) => deps.api.markFavorite({ kind: "pack", id: withId.pack }, favorite), { focus: "favorite" });
    },
    toggleReorder: () => set({ reordering: !transient.reordering, focus: "reorder" }),
    moveSticker(index, delta) {
      const model = readModel();
      if (model === null) return;
      reorder(moveOne(idsDeStickers(model), index, delta), `${delta === -1 ? "left" : "right"}-${index + delta}`);
    },
    dragSticker(from, until) {
      const model = readModel();
      if (model === null) return;
      reorder(moveTo(idsDeStickers(model), from, until), `sticker-${until}`);
    },
    changeCategory(category) {
      const pack = localPack();
      if (pack === null) return;
      withLibrary("Guardando…", () => deps.api.editPack(pack.id, { name: pack.name, author: pack.author, category }), { focus: "category" });
    },
    requestDelete: () => set({ confirmingDelete: true, focus: "delete-no" }),
    cancelDelete: () => set({ confirmingDelete: false, focus: "delete" }),
    confirmDelete() {
      const pack = localPack();
      if (pack === null) return;
      void run("Borrando…", async () => {
        transient = { ...transient, confirmingDelete: false };
        deps.setLibrary(await deps.api.deletePack(pack.id));
      });
    },
    openSticker: (index) => set({ openSticker: index }),
    toggleStickerFavorite() {
      const model = readModel();
      const index = transient.openSticker;
      const sticker = index === null ? undefined : model?.stickers[index];
      if (sticker === undefined || sticker.id === null) return;
      const id = sticker.id;
      withLibrary("Guardando…", (withId) => deps.api.markFavorite({ kind: "sticker", pack: withId.pack, id }, !sticker.favorite));
    },
    removeSticker() {
      const model = readModel();
      const index = transient.openSticker;
      const sticker = index === null ? undefined : model?.stickers[index];
      if (sticker === undefined || sticker.id === null) return;
      const id = sticker.id;
      withLibrary("Quitando…", (withId) => deps.api.removeSticker(withId.pack, id), { openSticker: null });
    },
    saveSticker() {
      const target = stickerInView();
      if (target === null) return;
      void run("Guardando el sticker…", async () => {
        const name = await deps.api.exportSticker(target.pack, target.sticker);
        transient = { ...transient, notice: savedStickerNotice(name) };
      });
    },
    shareSticker() {
      const target = stickerInView();
      if (target === null) return;
      void run("Abriendo el menú de compartir…", () => deps.api.shareSticker(target.pack, target.sticker));
    },
    sendStickerToWhatsapp() {
      const target = stickerInView();
      if (target === null) return;
      void run("Abriendo WhatsApp…", async () => {
        const { result, notice } = await deps.api.sendStickerToWhatsapp(target.pack, target.sticker);
        transient = { ...transient, notice: sentToWhatsappNotice(result, notice) };
        deps.reloadLibrary();
      });
    },
    editEmojis() {
      const model = readModel();
      const index = transient.openSticker;
      const sticker = index === null ? undefined : model?.stickers[index];
      if (model === null || model.pack === null || sticker === undefined || sticker.id === null) return;
      const pack = model.pack;
      const id = sticker.id;
      selector.open({
        path: sticker.path,
        initial: sticker.emojis,
        recent,
        async save(emojis) {
          try {
            deps.setLibrary(await deps.api.setEmojis(pack, id, [...emojis]));
          } catch (failure) {
            return errorMessage(asErrorUi(failure));
          }
          for (const emoji of emojis) recent = withRecent(recent, emoji);
          saveRecent(localStore(), recent);
          return null;
        },
        onClose: () => set({ focus: "change-emoji" }),
      });
    },
    async acceptNotice() {
      const id = pendingDownload;
      set({ communityNotice: false, error: null });
      try {
        await deps.api.acceptCommunityNotice();
      } catch (failure) {
        return set({ error: errorMessage(asErrorUi(failure)) });
      }
      acceptedNotice = true;
      deps.reloadCatalog(false);
      if (id !== null) await download(id);
    },
    cancelNotice() {
      pendingDownload = null;
      set({ communityNotice: false, focus: "action-download" });
    },
    openPropose: () => set({ proposing: true, focus: "propose-license" }),
    closePropose: () => set({ proposing: false, focus: "propose" }),
    propose(license, category) {
      const pack = localPack();
      if (pack === null || proposableNoReason(pack) !== null) return;
      const validation = validateProposal(license, category, deps.catalog()?.categories ?? []);
      if (!validation.valid) return set({ error: validation.reason });
      void run("Creando la carpeta…", async () => {
        const path = await deps.api.proposeToCommunity(pack.id, validation.license, validation.category);
        if (path !== null) {
          transient = { ...transient, proposing: false, notice: `Carpeta guardada en ${path}. Ábrela y sigue las instrucciones para proponerla.` };
        }
      });
    },
    cancelDownload: () => void deps.api.cancelCommunityDownload().catch((failure: unknown) => set({ error: errorMessage(asErrorUi(failure)) })),
  };

  const screen = createDetailView(actions, deps.fileUrl);
  const selector = createEmojiSelector(element<HTMLElement>("layer-emoji"), deps.fileUrl);
  context.registerLayer(() => {
    if (selector.close()) return true;
    if (transient.openSticker === null) return false;
    set({ openSticker: null });
    return true;
  });
  deps.onLibraryChange(render);

  return {
    onNavigate(nav) {
      if (panelVisible(nav) !== "detail") {
        viewPath = null;
        return;
      }
      const key = pathToText(nav.path);
      if (key !== viewPath) {
        viewPath = key;
        transient = Clean;
        if (nav.path.kind === "pack" && nav.path.origin === "community" && remote.get(nav.path.id)?.kind !== "ready") {
          void loadRemote(nav.path.id);
        }
      }
      render();
    },
    onCatalogChange: render,
  };
}
