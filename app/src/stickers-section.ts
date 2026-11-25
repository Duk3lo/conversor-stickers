import type { Context } from "./context.ts";
import * as commands from "./contract.ts";
import { type LibraryUi, asErrorUi, type PackEdit, type MediaKind } from "./contract.ts";
import * as mobileCommands from "./contract-mobile.ts";
import { errorMessage } from "./messages.ts";
import { openEditor, openPack, closePack, withPacks, type Navigation } from "./navigation.ts";
import {
  categoryForNew,
  currentFilter,
  validateAuthor,
  validateCategoryName,
  validatePackName,
} from "./packs.ts";
import { createStickersView } from "./stickers-view.ts";
import type { StickersActions, Load, StickersView } from "./views.ts";

export interface StickersSection {
  onNavigate(nav: Navigation): void;
  library(): LibraryUi | null;
  setLibrary(library: LibraryUi): void;
  libraryState(): Load<LibraryUi>;
  onLibraryChange(listener: () => void): void;
  reload(): void;
}

export function createStickersSection(context: Context, fileUrl: (path: string) => string): StickersSection {
  let view: StickersView = {
    library: { kind: "loading" },
    filter: { kind: "all" },
    openedPack: context.nav().openedPack,
    form: null,
    formError: null,
    confirmation: null,
    chosenSticker: null,
    busy: false,
    choosing: null,
    error: null,
    notices: [],
  };

  const listeners: (() => void)[] = [];
  let libraryLast = view.library;

  function render(): void {
    screen.render(view);
    if (view.library === libraryLast) return;
    libraryLast = view.library;
    for (const listener of listeners) listener();
  }

  function set(changes: Partial<StickersView>): void {
    view = { ...view, ...changes };
    render();
  }

  function currentLibrary(): LibraryUi | null {
    return view.library.kind === "ready" ? view.library.value : null;
  }

  function setLibrary(library: LibraryUi): void {
    const notices = library.notice === null ? view.notices : [...view.notices, library.notice];
    view = {
      ...view,
      library: { kind: "ready", value: library },
      filter: currentFilter(view.filter, library),
      notices,
    };
    const nav = context.nav();
    const fresh = withPacks(
      nav,
      library.packs.map((pack) => pack.id),
    );
    if (fresh !== nav) context.navigate(fresh);
    render();
  }

  async function load(): Promise<void> {
    set({ library: { kind: "loading" } });
    try {
      setLibrary(await commands.library());
    } catch (failure) {
      set({ library: { kind: "error", message: errorMessage(asErrorUi(failure)) } });
    }
  }

  async function run(
    command: Promise<LibraryUi>,
    onFinish?: (library: LibraryUi) => Partial<StickersView>,
    inForm = false,
  ): Promise<void> {
    set({ busy: true, error: null, formError: null });
    try {
      const library = await command;
      setLibrary(library);
      set({ busy: false, ...onFinish?.(library) });
    } catch (failure) {
      const message = errorMessage(asErrorUi(failure));
      set({ busy: false, ...(inForm ? { formError: message } : { error: message }) });
    }
  }

  function openedPack(): number | null {
    return view.openedPack;
  }

  function sendForm(text: string): void {
    const library = currentLibrary();
    const form = view.form;
    if (library === null || form === null) return;
    const close = { form: null, formError: null };
    switch (form.kind) {
      case "category-new": {
        const validation = validateCategoryName(text, library.categories, null);
        if (!validation.valid) return set({ formError: validation.reason });
        const name = validation.value;
        void run(
          commands.createCategory(name),
          (fresh) => {
            const created = fresh.categories.find((category) => category.name === name);
            return { ...close, ...(created ? { filter: { kind: "category" as const, id: created.id } } : {}) };
          },
          true,
        );
        return;
      }
      case "rename-category": {
        const validation = validateCategoryName(text, library.categories, form.id);
        if (!validation.valid) return set({ formError: validation.reason });
        void run(commands.renameCategory(form.id, validation.value), () => close, true);
        return;
      }
      case "pack-new": {
        const validation = validatePackName(text);
        if (!validation.valid) return set({ formError: validation.reason });
        const before = new Set(library.packs.map((pack) => pack.id));
        void run(
          commands.createPack(validation.value, categoryForNew(view.filter)),
          (fresh) => {
            const created = fresh.packs.find((pack) => !before.has(pack.id));
            if (created) context.navigate(openPack(context.nav(), created.id));
            return close;
          },
          true,
        );
        return;
      }
    }
  }

  function savePack(edit: PackEdit): void {
    const id = openedPack();
    if (id === null) return;
    const name = validatePackName(edit.name);
    if (!name.valid) return set({ formError: name.reason });
    const author = validateAuthor(edit.author);
    if (!author.valid) return set({ formError: author.reason });
    void run(
      commands.editPack(id, { name: name.value, author: author.value, category: edit.category }),
      () => ({ notices: [...view.notices, "Paquete guardado."] }),
      true,
    );
  }

  function confirm(): void {
    const confirmation = view.confirmation;
    if (confirmation === null) return;
    const command =
      confirmation.kind === "delete-pack"
        ? commands.deletePack(confirmation.id)
        : commands.deleteCategory(confirmation.id);
    void run(command, () => ({ confirmation: null, chosenSticker: null }));
  }

  function withChosenSticker(command: (pack: number, sticker: number) => Promise<LibraryUi>, clear: boolean): void {
    const pack = openedPack();
    const sticker = view.chosenSticker;
    if (pack === null || sticker === null) return;
    void run(command(pack, sticker), () => (clear ? { chosenSticker: null } : {}));
  }

  async function doExport(): Promise<void> {
    const id = openedPack();
    if (id === null) return;
    set({ busy: true, error: null });
    try {
      const name = await commands.exportPack(id);
      set({ busy: false, notices: name === null ? view.notices : [...view.notices, `Exportado como ${name}.`] });
    } catch (failure) {
      set({ busy: false, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function addToWhatsapp(): Promise<void> {
    const id = openedPack();
    if (id === null) return;
    set({ busy: true, error: null });
    try {
      const { result, notice } = await mobileCommands.addToWhatsapp(id);
      const fresh = [
        ...(result === "added" ? ["Paquete añadido a WhatsApp."] : []),
        ...(notice === null ? [] : [notice]),
      ];
      set({ busy: false, notices: [...view.notices, ...fresh] });
    } catch (failure) {
      set({ busy: false, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function doImport(): Promise<void> {
    set({ busy: true, error: null });
    try {
      const result = await commands.importWastickers();
      if (result === null) return set({ busy: false });
      setLibrary(result.library);
      set({ busy: false, notices: [...view.notices, ...result.notices] });
      context.navigate(openPack(context.nav(), result.pack));
    } catch (failure) {
      set({ busy: false, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function createFromGallery(kind: MediaKind): Promise<void> {
    set({ choosing: kind, error: null });
    try {
      const entries = await commands.pickFromGallery(kind);
      entries.forEach((entry) => context.remember(entry));
      const first = entries[0];
      if (first === undefined) return set({ choosing: null });
      if (first.media === null) {
        return set({ choosing: null, error: `No se pudo abrir ${first.name}.` });
      }
      set({ choosing: null });
      context.navigate(openEditor(context.nav(), first.id, "sticker"));
    } catch (failure) {
      set({ choosing: null, error: errorMessage(asErrorUi(failure)) });
    }
  }

  const actions: StickersActions = {
    reload: () => void load(),
    pickFilter: (filter) => set({ filter }),
    openForm: (form) => set({ form, formError: null }),
    closeForm: () => set({ form: null, formError: null }),
    sendForm,
    requestConfirmation: (confirmation) => set({ confirmation, error: null }),
    cancelConfirmation: () => set({ confirmation: null }),
    confirm,
    openPack: (id) => context.navigate(openPack(context.nav(), id)),
    closePack: () => context.navigate(closePack(context.nav())),
    savePack,
    pickSticker: (id) => set({ chosenSticker: id }),
    removeSticker: () => withChosenSticker(commands.removeSticker, true),
    useAsIcon: () => withChosenSticker(commands.useAsIcon, false),
    doExport: () => void doExport(),
    addToWhatsapp: () => void addToWhatsapp(),
    doImport: () => void doImport(),
    createFromGallery: (kind) => void createFromGallery(kind),
    closeNotices: () => set({ notices: [] }),
  };
  const screen = createStickersView(actions, fileUrl);
  render();
  void load();

  return {
    onNavigate(nav) {
      if (nav.openedPack === view.openedPack) return;
      set({ openedPack: nav.openedPack, chosenSticker: null, formError: null, confirmation: null });
    },
    library: currentLibrary,
    setLibrary,
    libraryState: () => view.library,
    onLibraryChange: (listener) => void listeners.push(listener),
    reload: () => void load(),
  };
}
