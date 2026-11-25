import type { LibraryUi, PackUi } from "./contract.ts";
import { pressedButton, create, element, icon, showMessage } from "./dom.ts";
import { CREATE_HELP, whatsappState } from "./mobile.ts";
import { filterPacks, MAX_STICKERS, exportableNoReason, countText, kindText } from "./packs.ts";
import { platform } from "./platform.ts";
import type { StickersActions, CategoryFilter, StickersForm, StickersView } from "./views.ts";

function filterSame(a: CategoryFilter, b: CategoryFilter): boolean {
  return a.kind === b.kind && (a.kind !== "category" || (b.kind === "category" && a.id === b.id));
}

function formText(form: StickersForm, library: LibraryUi | null) {
  switch (form.kind) {
    case "category-new":
      return { label: "Nombre de la categoría nueva", button: "Crear", value: "" };
    case "rename-category": {
      const name = library?.categories.find((c) => c.id === form.id)?.name ?? "";
      return { label: `Nuevo nombre para «${name}»`, button: "Renombrar", value: name };
    }
    case "pack-new":
      return { label: "Nombre del paquete nuevo", button: "Crear paquete", value: "" };
  }
}

function thumbnail(container: HTMLElement, url: string | null, name: string): void {
  const current = container.querySelector("img");
  if (url !== null) {
    if (current?.getAttribute("src") === url) return;
    const image = create("img", "");
    image.src = url;
    image.alt = "";
    container.replaceChildren(image);
  } else {
    container.replaceChildren(create("span", "thumbnail-initial", [...name.trim()][0]?.toUpperCase() ?? "?"));
  }
}

export function createStickersView(
  actions: StickersActions,
  fileUrl: (path: string) => string,
): { render(view: StickersView): void } {
  element<HTMLParagraphElement>("create-help").textContent = CREATE_HELP[platform];
  const list = element<HTMLDivElement>("stickers-list");
  const importButton = element<HTMLButtonElement>("import");
  const newButton = element<HTMLButtonElement>("new-pack");
  const categories = element<HTMLDivElement>("categories");
  const actionsCategory = element<HTMLDivElement>("category-actions");
  const nameCategory = element<HTMLSpanElement>("category-name");
  const categoryRenameButton = element<HTMLButtonElement>("rename-category");
  const categoryDeleteButton = element<HTMLButtonElement>("delete-category");
  const confirmCategory = element<HTMLDivElement>("confirm-category");
  const confirmTextCategory = element<HTMLParagraphElement>("confirm-category-text");
  const confirmSiCategory = element<HTMLButtonElement>("confirm-category-si");
  const confirmNoCategory = element<HTMLButtonElement>("confirm-category-no");
  const form = element<HTMLFormElement>("form-stickers");
  const labelForm = element<HTMLLabelElement>("form-stickers-label");
  const textForm = element<HTMLInputElement>("form-stickers-text");
  const sendForm = element<HTMLButtonElement>("form-stickers-send");
  const cancelForm = element<HTMLButtonElement>("form-stickers-cancel");
  const errorForm = element<HTMLParagraphElement>("form-stickers-error");
  const packsState = element<HTMLParagraphElement>("packs-state");
  const retryButton = element<HTMLButtonElement>("library-retry");
  const packsGrid = element<HTMLUListElement>("grid-packs");

  const article = element<HTMLElement>("pack");
  const backButton = element<HTMLButtonElement>("pack-back");
  const packIcon = element<HTMLDivElement>("pack-icon");
  const packTitle = element<HTMLHeadingElement>("pack-title");
  const packData = element<HTMLParagraphElement>("pack-data");
  const packForm = element<HTMLFormElement>("form-pack");
  const nameField = element<HTMLInputElement>("pack-name");
  const authorField = element<HTMLInputElement>("pack-author");
  const categoryField = element<HTMLSelectElement>("pack-category");
  const packSaveButton = element<HTMLButtonElement>("pack-save");
  const packFormError = element<HTMLParagraphElement>("form-pack-error");
  const stickersGrid = element<HTMLUListElement>("grid-stickers");
  const actionsSticker = element<HTMLDivElement>("sticker-actions");
  const chosenSticker = element<HTMLSpanElement>("sticker-chosen");
  const iconUseButton = element<HTMLButtonElement>("use-icon");
  const stickerRemoveButton = element<HTMLButtonElement>("remove-sticker");
  const exportButton = element<HTMLButtonElement>("export");
  const exportReason = element<HTMLParagraphElement>("export-reason");
  const whatsappBlock = element<HTMLDivElement>("whatsapp-block");
  const whatsappButton = element<HTMLButtonElement>("whatsapp");
  const whatsappReason = element<HTMLParagraphElement>("whatsapp-reason");
  const packDeleteButton = element<HTMLButtonElement>("delete-pack");
  const confirmPack = element<HTMLDivElement>("confirm-pack");
  const confirmTextPack = element<HTMLParagraphElement>("confirm-pack-text");
  const confirmSiPack = element<HTMLButtonElement>("confirm-pack-si");
  const confirmNoPack = element<HTMLButtonElement>("confirm-pack-no");

  const notices = element<HTMLDivElement>("stickers-notices");
  const noticesList = element<HTMLUListElement>("stickers-notices-list");
  const closeNotices = element<HTMLButtonElement>("stickers-notices-close");
  const error = element<HTMLDivElement>("stickers-error");
  const errorText = element<HTMLParagraphElement>("stickers-error-text");
  const createDestination = element<HTMLParagraphElement>("create-sticker-destination");
  const imageButton = element<HTMLButtonElement>("from-image");
  const videoButton = element<HTMLButtonElement>("from-video");

  let view: StickersView | null = null;
  let paintedForm: StickersForm | null = null;
  let paintedPack = "";
  let paintedConfirmation: StickersView["confirmation"] = null;

  function library(): LibraryUi | null {
    return view?.library.kind === "ready" ? view.library.value : null;
  }

  importButton.addEventListener("click", () => actions.doImport());
  newButton.addEventListener("click", () => actions.openForm({ kind: "pack-new" }));
  retryButton.addEventListener("click", () => actions.reload());

  categories.addEventListener("click", (event) => {
    const button = pressedButton(event);
    if (button === null) return;
    const { filter, id } = button.dataset;
    if (filter === "new") actions.openForm({ kind: "category-new" });
    else if (filter === "all") actions.pickFilter({ kind: "all" });
    else if (filter === "without-category") actions.pickFilter({ kind: "without-category" });
    else if (filter === "category") actions.pickFilter({ kind: "category", id: Number(id) });
  });

  function chosenCategory(): number | null {
    return view?.filter.kind === "category" ? view.filter.id : null;
  }

  categoryRenameButton.addEventListener("click", () => {
    const id = chosenCategory();
    if (id !== null) actions.openForm({ kind: "rename-category", id });
  });
  categoryDeleteButton.addEventListener("click", () => {
    const id = chosenCategory();
    if (id !== null) actions.requestConfirmation({ kind: "delete-category", id });
  });
  confirmSiCategory.addEventListener("click", () => actions.confirm());
  confirmNoCategory.addEventListener("click", () => actions.cancelConfirmation());

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    actions.sendForm(textForm.value);
  });
  cancelForm.addEventListener("click", () => actions.closeForm());
  textForm.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      actions.closeForm();
    }
  });

  packsGrid.addEventListener("click", (event) => {
    const button = pressedButton(event);
    if (button?.dataset.id) actions.openPack(Number(button.dataset.id));
  });

  backButton.addEventListener("click", () => actions.closePack());
  packForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const category = categoryField.value === "" ? null : Number(categoryField.value);
    actions.savePack({ name: nameField.value, author: authorField.value, category });
  });
  stickersGrid.addEventListener("click", (event) => {
    const button = pressedButton(event);
    if (!button?.dataset.id) return;
    const id = Number(button.dataset.id);
    actions.pickSticker(view?.chosenSticker === id ? null : id);
  });
  iconUseButton.addEventListener("click", () => actions.useAsIcon());
  stickerRemoveButton.addEventListener("click", () => actions.removeSticker());
  exportButton.addEventListener("click", () => {
    if (exportButton.getAttribute("aria-disabled") !== "true") actions.doExport();
  });
  whatsappButton.addEventListener("click", () => {
    if (whatsappButton.getAttribute("aria-disabled") !== "true") actions.addToWhatsapp();
  });
  packDeleteButton.addEventListener("click", () => {
    if (view?.openedPack != null) actions.requestConfirmation({ kind: "delete-pack", id: view.openedPack });
  });
  confirmSiPack.addEventListener("click", () => actions.confirm());
  confirmNoPack.addEventListener("click", () => actions.cancelConfirmation());

  closeNotices.addEventListener("click", () => actions.closeNotices());
  imageButton.addEventListener("click", () => actions.createFromGallery("image"));
  videoButton.addEventListener("click", () => actions.createFromGallery("video"));

  function chip(text: string, data: Record<string, string>, pressed: boolean | null): HTMLButtonElement {
    const button = create("button", "chip", text);
    button.type = "button";
    Object.assign(button.dataset, data);
    if (pressed !== null) button.setAttribute("aria-pressed", String(pressed));
    return button;
  }

  function renderCategories(v: StickersView, b: LibraryUi): void {
    const focused = document.activeElement instanceof HTMLElement && categories.contains(document.activeElement)
      ? { ...document.activeElement.dataset }
      : null;
    const chips = [
      chip("Todas", { filter: "all" }, filterSame(v.filter, { kind: "all" })),
      ...b.categories.map((category) =>
        chip(
          category.name,
          { filter: "category", id: String(category.id) },
          filterSame(v.filter, { kind: "category", id: category.id }),
        ),
      ),
      chip("Sin categoría", { filter: "without-category" }, filterSame(v.filter, { kind: "without-category" })),
    ];
    const fresh = chip("", { filter: "new" }, null);
    fresh.classList.add("chip-new");
    fresh.append(icon("plus"), document.createTextNode("Categoría"));
    fresh.setAttribute("aria-label", "Crear una categoría");
    chips.push(fresh);
    categories.replaceChildren(...chips);
    if (focused) {
      const same = chips.find((c) => c.dataset.filter === focused.filter && c.dataset.id === focused.id);
      same?.focus();
    }

    const id = chosenCategory();
    const name = b.categories.find((category) => category.id === id)?.name ?? null;
    actionsCategory.hidden = name === null;
    nameCategory.textContent = name === null ? "" : `Categoría «${name}»:`;
    categoryRenameButton.disabled = v.busy;
    categoryDeleteButton.disabled = v.busy;
  }

  function renderForm(v: StickersView): void {
    form.hidden = v.form === null;
    if (v.form !== null && v.form !== paintedForm) {
      const texts = formText(v.form, library());
      labelForm.textContent = texts.label;
      sendForm.textContent = texts.button;
      textForm.value = texts.value;
      textForm.focus();
      textForm.select();
    }
    paintedForm = v.form;
    textForm.setAttribute("aria-invalid", String(v.formError !== null));
    showMessage(errorForm, v.formError);
    sendForm.disabled = v.busy;
  }

  function packCard(pack: PackUi): HTMLLIElement {
    const listElement = create("li", "");
    const button = create("button", "card-pack");
    button.type = "button";
    button.dataset.id = String(pack.id);
    const image = create("span", "thumbnail");
    thumbnail(image, pack.icon === null ? null : fileUrl(pack.icon), pack.name);
    const name = create("span", "card-pack-name", pack.name);
    const data = create("span", "card-pack-data", `${countText(pack)} · ${kindText(pack.kind)}`);
    button.append(image, name, data);
    button.setAttribute(
      "aria-label",
      `${pack.name}: ${pack.stickers.length} de ${MAX_STICKERS} stickers, ${kindText(pack.kind).toLowerCase()}`,
    );
    listElement.append(button);
    return listElement;
  }

  function renderList(v: StickersView): void {
    const b = library();
    newButton.disabled = b === null || v.busy;
    importButton.disabled = b === null || v.busy;
    retryButton.hidden = v.library.kind !== "error";
    packsState.classList.toggle("state-error", v.library.kind === "error");

    if (b === null) {
      categories.replaceChildren();
      actionsCategory.hidden = true;
      packsGrid.replaceChildren();
      packsState.hidden = false;
      packsState.textContent =
        v.library.kind === "error"
          ? `No se pudieron leer tus paquetes: ${v.library.message}`
          : "Cargando tus paquetes…";
      packsState.setAttribute("aria-busy", String(v.library.kind === "loading"));
      return;
    }
    packsState.removeAttribute("aria-busy");
    renderCategories(v, b);
    const visible = filterPacks(b, v.filter);
    if (b.packs.length === 0) {
      packsState.hidden = false;
      packsState.textContent = "Todavía no tienes paquetes. Crea uno para empezar.";
    } else if (visible.length === 0) {
      packsState.hidden = false;
      packsState.textContent =
        v.filter.kind === "without-category" ? "Todos tus paquetes tienen categoría." : "No hay paquetes en esta categoría.";
    } else {
      packsState.hidden = true;
    }
    packsGrid.replaceChildren(...visible.map(packCard));
  }

  function renderPack(v: StickersView, pack: PackUi, b: LibraryUi): void {
    packTitle.textContent = pack.name;
    packData.textContent = [
      `${countText(pack)} stickers`,
      kindText(pack.kind),
      pack.author ? `de ${pack.author}` : null,
    ]
      .filter((part) => part !== null)
      .join(" · ");
    thumbnail(packIcon, pack.icon === null ? null : fileUrl(pack.icon), pack.name);
    packIcon.setAttribute("aria-label", pack.icon === null ? "Sin icono todavía" : "Icono de la bandeja");
    packIcon.setAttribute("role", "img");

    const options = [
      new Option("Sin categoría", ""),
      ...b.categories.map((category) => new Option(category.name, String(category.id))),
    ];
    const signature = JSON.stringify([pack.id, pack.name, pack.author, pack.category, b.categories]);
    if (signature !== paintedPack) {
      paintedPack = signature;
      nameField.value = pack.name;
      authorField.value = pack.author;
      categoryField.replaceChildren(...options);
      categoryField.value = pack.category === null ? "" : String(pack.category);
    }
    packSaveButton.disabled = v.busy;
    showMessage(packFormError, v.form === null ? v.formError : null);

    const gaps: HTMLLIElement[] = [];
    pack.stickers.forEach((sticker, index) => {
      const gap = create("li", "gap gap-full");
      const button = create("button", "sticker");
      button.type = "button";
      button.dataset.id = String(sticker.id);
      button.setAttribute("aria-pressed", String(v.chosenSticker === sticker.id));
      button.setAttribute("aria-label", `Sticker ${index + 1}`);
      const image = create("img", "");
      image.src = fileUrl(sticker.path);
      image.alt = "";
      image.loading = "lazy";
      button.append(image);
      gap.append(button);
      gaps.push(gap);
    });
    for (let index = pack.stickers.length; index < MAX_STICKERS; index += 1) {
      const gap = create("li", "gap");
      gap.setAttribute("aria-hidden", "true");
      gaps.push(gap);
    }
    const focused = document.activeElement instanceof HTMLElement ? document.activeElement.dataset.id : undefined;
    const focusWas = stickersGrid.contains(document.activeElement);
    stickersGrid.replaceChildren(...gaps);
    if (focusWas && focused) stickersGrid.querySelector<HTMLButtonElement>(`[data-id="${focused}"]`)?.focus();

    const chosenIndex = pack.stickers.findIndex((sticker) => sticker.id === v.chosenSticker);
    actionsSticker.hidden = chosenIndex === -1;
    chosenSticker.textContent = `Sticker ${chosenIndex + 1}:`;
    iconUseButton.disabled = v.busy;
    stickerRemoveButton.disabled = v.busy;

    const reason = exportableNoReason(pack);
    exportButton.setAttribute("aria-disabled", String(reason !== null || v.busy));
    exportReason.textContent = reason ?? "Para importarlo en el móvil con una app de stickers.";
    const whatsapp = whatsappState(platform, pack);
    whatsappBlock.hidden = !whatsapp.visible;
    whatsappButton.setAttribute("aria-disabled", String(!whatsapp.active || v.busy));
    whatsappReason.textContent = whatsapp.reason ?? "Se abre WhatsApp para confirmar.";
    packDeleteButton.disabled = v.busy;
  }

  function renderConfirmations(v: StickersView, b: LibraryUi | null): void {
    const c = v.confirmation;
    confirmPack.hidden = c?.kind !== "delete-pack";
    confirmCategory.hidden = c?.kind !== "delete-category";
    confirmSiPack.disabled = v.busy;
    confirmSiCategory.disabled = v.busy;
    if (c?.kind === "delete-pack") {
      const pack = b?.packs.find((p) => p.id === c.id);
      const howMany = pack?.stickers.length ?? 0;
      confirmTextPack.textContent =
        howMany === 0
          ? `¿Borrar el paquete «${pack?.name ?? ""}»?`
          : `¿Borrar «${pack?.name ?? ""}» y sus ${howMany} stickers? No se puede deshacer.`;
    } else if (c?.kind === "delete-category") {
      const name = b?.categories.find((category) => category.id === c.id)?.name ?? "";
      confirmTextCategory.textContent = `¿Borrar la categoría «${name}»? Sus paquetes no se borran: quedan sin categoría.`;
    }
    if (c !== null && c !== paintedConfirmation) {
      (c.kind === "delete-pack" ? confirmNoPack : confirmNoCategory).focus();
    }
    paintedConfirmation = c;
  }

  function renderCreate(v: StickersView, open: PackUi | null): void {
    imageButton.disabled = v.choosing !== null;
    videoButton.disabled = v.choosing !== null;
    imageButton.setAttribute("aria-busy", String(v.choosing === "image"));
    videoButton.setAttribute("aria-busy", String(v.choosing === "video"));
    if (v.choosing !== null) {
      createDestination.textContent = v.choosing === "image" ? "Eligiendo una imagen…" : "Eligiendo un video…";
    } else if (open !== null) {
      createDestination.textContent = `Al terminar podrás añadirlo a «${open.name}».`;
    } else {
      createDestination.textContent = "Elige una imagen o un video. Al terminar podrás guardarlo o añadirlo a un paquete.";
    }
  }

  return {
    render(fresh) {
      view = fresh;
      const b = library();
      const open = b?.packs.find((pack) => pack.id === fresh.openedPack) ?? null;
      list.hidden = open !== null;
      article.hidden = open === null;
      if (open === null) {
        paintedPack = "";
        renderList(fresh);
        renderForm(fresh);
      } else if (b !== null) {
        form.hidden = true;
        paintedForm = null;
        renderPack(fresh, open, b);
      }
      renderConfirmations(fresh, b);
      noticesList.replaceChildren(...fresh.notices.map((notice) => create("li", "", notice)));
      notices.hidden = fresh.notices.length === 0;
      showMessage(errorText, fresh.error);
      error.hidden = fresh.error === null;
      renderCreate(fresh, open);
    },
  };
}
