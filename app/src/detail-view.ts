import { COMMUNITY_NOTICE, Licenses } from "./community.ts";
import { checkText, withdrawnUrl } from "./detail.ts";
import { button, create, element, icon, image, renderFocusKeeping } from "./dom.ts";
import { originLabel } from "./klipy.ts";
import { platform } from "./platform.ts";
import { SHARE_AS_IMAGE_HINT, stickerOptions } from "./share.ts";
import { createState, type FileUrl } from "./catalog-view.ts";
import type { DetailActions, DetailView } from "./views.ts";

const LONG_PRESS_MS = 600;

function progressText(view: DetailView): string | null {
  const progress = view.progress;
  if (progress === null) return view.busy;
  return progress.kind === "verifying" ? "Verificando los archivos…" : `Descargando ${progress.done} de ${progress.total}…`;
}

export function createDetailView(actions: DetailActions, fileUrl: FileUrl): { render(view: DetailView): void } {
  const panel = element<HTMLElement>("panel-detail");
  let openLayer: number | null = null;

  return {
    render(view) {
      renderFocusKeeping(panel, () => {
        const content = create("div", "stack detail");
        const back = button("button-back", "Atrás", "back", "Volver");
        back.dataset["focus"] = "back";
        back.addEventListener("click", actions.back);
        content.append(back);

        if (view.state.kind === "loading") {
          content.append(createState("Cargando el paquete…", "loading"));
        } else if (view.state.kind === "error") {
          content.append(createState(view.state.message, "error", { text: "Reintentar", onPress: actions.retry }));
        } else {
          renderModel(content, view, view.state.value);
        }
        panel.replaceChildren(content);
      });
      moveFocusOfLayer(view);
      if (view.focus !== null) panel.querySelector<HTMLElement>(`[data-focus="${view.focus}"]`)?.focus({ preventScroll: true });
    },
  };

  function renderModel(content: HTMLElement, view: DetailView, model: CurrentModel): void {
    const title = create("h2", "title-view", model.name);
    title.id = "title-detail";
    panel.setAttribute("aria-labelledby", title.id);
    panel.removeAttribute("aria-label");

    const header = create("header", "detail-header");
    header.append(image(model.icon === null ? null : fileUrl(model.icon), "detail-icon"));
    const data = create("div", "detail-data");
    data.append(title);
    const parts = [
      model.author === "" ? "" : `de ${model.author}`,
      model.count === 1 ? "1 sticker" : `${model.count} stickers`,
      model.animated === null ? "" : model.animated ? "Animado" : "Estático",
      model.license ?? "",
    ].filter((part) => part !== "");
    data.append(create("p", "secondary", parts.join(" · ")));
    if (model.version !== null) data.append(create("p", "secondary small", model.version));
    header.append(data);
    content.append(header);

    if (model.withdrawn !== null) {
      const notice = create("div", "panel-error");
      notice.setAttribute("role", "alert");
      notice.append(icon("flag"), create("p", "", `Este paquete se retiró del catálogo: ${model.withdrawn}`));
      content.append(notice);
    }

    if (view.communityNotice) content.append(createCommunityNotice());
    if (model.origin === "community" || model.communityId !== null) {
      content.append(create("p", "secondary small", "Paquete de otra persona de la comunidad: se revisa antes de publicarse, pero puede haber errores."));
    }
    content.append(createSecondaryActions(view, model));
    if (view.confirmingDelete) content.append(createConfirmation(view));
    if (view.proposing) content.append(createProposeForm(view));

    if (model.stickers.length === 0) {
      content.append(createState("Este paquete todavía no tiene stickers.", "empty"));
    } else {
      content.append(createGrid(view, model));
    }
    if (model.checks.length > 0) content.append(createChecks(model));
    if (view.error !== null) {
      const error = create("div", "panel-error");
      error.setAttribute("role", "alert");
      error.append(icon("notice"), create("p", "", view.error));
      content.append(error);
    }
    if (view.notice !== null) {
      const notice = create("div", "panel-notice");
      notice.setAttribute("role", "status");
      notice.append(icon("info"), create("p", "", view.notice));
      content.append(notice);
    }
    content.append(createBar(view, model));
    if (view.openSticker !== null) content.append(createLayer(view, model, view.openSticker));
  }

  function createSecondaryActions(view: DetailView, model: CurrentModel): HTMLElement {
    const row = create("div", "actions-row detail-actions");
    const busy = view.busy !== null;
    if (model.favorite !== null) {
      const favorite = button("button-secondary button-small", model.favorite ? "Quitar de favoritos" : "Favorito", model.favorite ? "favorite-full" : "favorite");
      favorite.setAttribute("aria-pressed", String(model.favorite));
      favorite.dataset["focus"] = "favorite";
      favorite.disabled = busy;
      favorite.addEventListener("click", actions.toggleFavorite);
      row.append(favorite);
    }
    if (model.pack !== null && model.stickers.length > 1) {
      const reorder = button("button-secondary button-small", view.reordering ? "Terminar de ordenar" : "Reordenar", view.reordering ? "ok" : "right");
      reorder.setAttribute("aria-pressed", String(view.reordering));
      reorder.dataset["focus"] = "reorder";
      reorder.disabled = busy;
      reorder.addEventListener("click", actions.toggleReorder);
      row.append(reorder);
    }
    if (model.origin === "local" && model.pack !== null) {
      const field = create("label", "field-in-line");
      field.append(create("span", "secondary small", "Categoría"));
      const selector = document.createElement("select");
      selector.dataset["focus"] = "category";
      selector.disabled = busy;
      selector.append(new Option("Sin categoría", ""));
      for (const category of view.categories) {
        selector.append(new Option(category.name, String(category.id), false, category.id === model.category));
      }
      selector.value = model.category === null ? "" : String(model.category);
      selector.addEventListener("change", () => actions.changeCategory(selector.value === "" ? null : Number(selector.value)));
      field.append(selector);
      row.append(field);
      const doDelete = button("button-text button-text-danger", "Borrar paquete", "delete");
      doDelete.dataset["focus"] = "delete";
      doDelete.disabled = busy;
      doDelete.addEventListener("click", actions.requestDelete);
      row.append(doDelete);
    }
    if (model.communityId !== null) {
      row.append(createLink("Solicitar retirada", withdrawnUrl(model.communityId), "flag"));
    }
    if (model.origin === "local" && model.pack !== null && model.communityId === null) {
      const propose = button("button-text", "Proponer a la comunidad", "export");
      propose.dataset["focus"] = "propose";
      propose.setAttribute("aria-expanded", String(view.proposing));
      propose.disabled = busy;
      propose.addEventListener("click", view.proposing ? actions.closePropose : actions.openPropose);
      row.append(propose);
    }
    return row;
  }

  function createLink(text: string, url: string, iconName: string): HTMLAnchorElement {
    const link = document.createElement("a");
    link.className = "button-text link-external";
    link.href = url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.append(icon(iconName), document.createTextNode(text));
    link.setAttribute("aria-label", `${text} (se abre en el navegador)`);
    return link;
  }

  function createCommunityNotice(): HTMLElement {
    const node = create("div", "panel-confirm");
    node.setAttribute("role", "alertdialog");
    node.setAttribute("aria-labelledby", "notice-community-text");
    const text = create("p", "", COMMUNITY_NOTICE);
    text.id = "notice-community-text";
    const buttons = create("div", "actions-row");
    const si = button("button-main", "Entendido, descargar", "download");
    si.dataset["focus"] = "notice-si";
    si.addEventListener("click", actions.acceptNotice);
    const no = button("button-secondary", "Cancelar", null);
    no.dataset["focus"] = "notice-no";
    no.addEventListener("click", actions.cancelNotice);
    buttons.append(si, no);
    node.append(text, buttons);
    return node;
  }

  function createProposeForm(view: DetailView): HTMLElement {
    const form = create("form", "block form-propose");
    form.noValidate = true;
    form.setAttribute("aria-label", "Proponer a la comunidad");
    const explanation = create("p", "secondary small", "No se sube nada: se crea un .zip con la carpeta del paquete para proponerlo a la comunidad.");
    form.append(explanation);

    const license = document.createElement("select");
    license.id = "propose-license";
    license.dataset["focus"] = "propose-license";
    license.append(new Option("Elige una licencia", ""));
    for (const allowed of Licenses) license.append(new Option(allowed.name, allowed.id));
    const category = document.createElement("select");
    category.id = "propose-category";
    category.append(new Option("Elige una categoría", ""));
    for (const candidate of view.communityCategories) category.append(new Option(candidate.name, candidate.id));
    form.append(field("Licencia", license), field("Categoría del catálogo", category));

    const reason = view.proposeReason ?? (view.communityCategories.length === 0 ? "Hace falta el catálogo para elegir categoría: conéctate y vuelve a intentarlo." : null);
    if (reason !== null) {
      const notice = create("p", "message-error", reason);
      notice.setAttribute("role", "alert");
      form.append(notice);
    }
    const buttons = create("div", "actions-row");
    const send = button("button-main", "Crear carpeta para el PR", null);
    send.type = "submit";
    send.disabled = reason !== null || view.busy !== null;
    const close = button("button-secondary", "Cancelar", null);
    close.addEventListener("click", actions.closePropose);
    buttons.append(send, close);
    form.append(buttons);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      actions.propose(license.value, category.value);
    });
    return form;
  }

  function field(text: string, control: HTMLSelectElement): HTMLElement {
    const node = create("div", "field");
    const label = create("label", "", text);
    label.htmlFor = control.id;
    node.append(label, control);
    return node;
  }

  function createConfirmation(view: DetailView): HTMLElement {
    const node = create("div", "panel-confirm");
    node.setAttribute("role", "group");
    node.setAttribute("aria-label", "Confirmar el borrado");
    node.append(create("p", "", "¿Borrar este paquete y todos sus stickers? No se puede deshacer."));
    const buttons = create("div", "actions-row");
    const si = button("button-danger", "Borrar paquete", null);
    si.dataset["focus"] = "delete-si";
    si.disabled = view.busy !== null;
    si.addEventListener("click", actions.confirmDelete);
    const no = button("button-secondary", "Cancelar", null);
    no.dataset["focus"] = "delete-no";
    no.addEventListener("click", actions.cancelDelete);
    buttons.append(si, no);
    node.append(buttons);
    return node;
  }

  function createGrid(view: DetailView, model: CurrentModel): HTMLElement {
    const block = create("section", "detail-stickers");
    const title = create("h3", "subtitle", "Stickers");
    title.id = "title-stickers-detail";
    block.append(title);
    if (view.reordering) {
      block.append(create("p", "secondary small", "Usa las flechas de cada sticker, o arrástralos, para cambiar el orden."));
    }
    const list = create("ul", `grid-stickers-detail${view.reordering ? " reordering" : ""}`);
    list.setAttribute("aria-labelledby", title.id);
    model.stickers.forEach((sticker, index) => {
      const element = document.createElement("li");
      element.className = "sticker-detail";
      const emojis = sticker.emojis.join("");
      const open = button("sticker-detail-button", "", null, `Sticker ${index + 1} de ${model.stickers.length}, emoji ${emojis}${sticker.favorite ? ", favorito" : ""}`);
      open.dataset["focus"] = `sticker-${index}`;
      open.append(image(fileUrl(sticker.path), "sticker-detail-image"));
      open.append(create("span", "sticker-detail-emoji", emojis));
      if (sticker.favorite) open.append(icon("favorite-full", "icon sticker-detail-favorite"));
      open.addEventListener("click", () => actions.openSticker(index));
      if (!view.reordering && model.pack !== null) activateLongPress(open, actions.toggleReorder);
      element.append(open);
      if (view.reordering) {
        element.draggable = true;
        element.addEventListener("dragstart", (event) => event.dataTransfer?.setData("text/plain", String(index)));
        element.addEventListener("dragover", (event) => event.preventDefault());
        element.addEventListener("drop", (event) => {
          event.preventDefault();
          const from = Number(event.dataTransfer?.getData("text/plain"));
          if (Number.isInteger(from)) actions.dragSticker(from, index);
        });
        const controls = create("span", "sticker-detail-controls");
        const left = button("button-icon", "", "left", `Mover el sticker ${index + 1} a la izquierda`);
        left.dataset["focus"] = `left-${index}`;
        left.disabled = index === 0 || view.busy !== null;
        left.addEventListener("click", () => actions.moveSticker(index, -1));
        const right = button("button-icon", "", "right", `Mover el sticker ${index + 1} a la derecha`);
        right.dataset["focus"] = `right-${index}`;
        right.disabled = index === model.stickers.length - 1 || view.busy !== null;
        right.addEventListener("click", () => actions.moveSticker(index, 1));
        controls.append(left, right);
        element.append(controls);
      }
      list.append(element);
    });
    block.append(list);
    return block;
  }

  function createChecks(model: CurrentModel): HTMLElement {
    const block = create("section", "detail-checks");
    const title = create("h3", "subtitle", "Antes de publicar");
    title.id = "title-checks";
    block.append(title);
    const list = create("ul", "checks");
    list.setAttribute("aria-labelledby", title.id);
    for (const check of model.checks) {
      const item = create("li", `check ${check.ok ? "check-ok" : "check-failure"}`);
      item.append(icon(check.ok ? "ok" : "close"));
      const visible = create("span", "", check.text);
      visible.setAttribute("aria-hidden", "true");
      item.append(visible, create("span", "hidden-visual", checkText(check)));
      list.append(item);
    }
    block.append(list);
    return block;
  }

  function createBar(view: DetailView, model: CurrentModel): HTMLElement {
    const bar = create("div", "bar-action");
    bar.setAttribute("role", "region");
    bar.setAttribute("aria-label", "Acción del paquete");
    const text = create("div", "bar-action-text");
    const inGear = progressText(view);
    if (inGear !== null) {
      const state = create("p", "small", inGear);
      state.setAttribute("role", "status");
      text.append(state);
      if (view.progress?.kind === "downloading") {
        const meter = document.createElement("progress");
        meter.max = view.progress.total;
        meter.value = view.progress.done;
        meter.setAttribute("aria-label", "Progreso de la descarga");
        text.append(meter);
      }
    } else if (model.bar.reason !== null) {
      const reason = create("p", "secondary small", model.bar.reason);
      reason.id = "bar-reason";
      text.append(reason);
    } else if (model.bar.state !== null) {
      const state = create("p", "small bar-action-state", model.bar.state);
      state.prepend(icon("ok"));
      text.append(state);
    }
    bar.append(text);
    const { main, secondary } = model.bar;
    if (view.progress !== null) {
      const cancel = button("button-secondary", "Cancelar", "close");
      cancel.dataset["focus"] = "cancel-download";
      cancel.addEventListener("click", actions.cancelDownload);
      bar.append(cancel);
    } else if (secondary !== null) {
      const other = button("button-secondary", secondary.text, null);
      other.dataset["focus"] = `action-${secondary.key}`;
      other.disabled = !secondary.active || view.busy !== null;
      other.addEventListener("click", () => actions.action(secondary.key));
      bar.append(other);
    }
    const action = button("button-main", main.text, null);
    action.dataset["focus"] = `action-${main.key}`;
    const active = main.active && view.busy === null;
    if (!active) action.setAttribute("aria-disabled", "true");
    if (model.bar.reason !== null) action.setAttribute("aria-describedby", "bar-reason");
    action.addEventListener("click", () => {
      if (active) actions.action(main.key);
    });
    bar.append(action);
    return bar;
  }

  function createLayer(view: DetailView, model: CurrentModel, index: number): HTMLElement {
    const sticker = model.stickers[index];
    const layer = create("div", "layer-sticker");
    if (sticker === undefined) return layer;
    layer.setAttribute("role", "dialog");
    layer.setAttribute("aria-modal", "true");
    layer.setAttribute("aria-label", `Sticker ${index + 1} de ${model.stickers.length}`);
    layer.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        actions.openSticker(null);
      } else if (event.key === "ArrowLeft" && index > 0) actions.openSticker(index - 1);
      else if (event.key === "ArrowRight" && index < model.stickers.length - 1) actions.openSticker(index + 1);
    });
    const card = create("div", "layer-sticker-card");
    const large = image(fileUrl(sticker.path), "layer-sticker-image");
    if (large instanceof HTMLImageElement) large.alt = `Sticker ${index + 1}, emoji ${sticker.emojis.join(" ")}`;
    card.append(large, create("p", "layer-sticker-emoji", sticker.emojis.join(" ")));
    const mediaOrigin = originLabel(sticker.mediaOrigin);
    if (mediaOrigin !== null) card.append(create("span", "badge badge-klipy", `Medio de ${mediaOrigin}`));
    const controls = create("div", "actions-row actions-centered");
    const previous = button("button-secondary", "", "left", "Sticker anterior");
    previous.disabled = index === 0;
    previous.addEventListener("click", () => actions.openSticker(index - 1));
    const next = button("button-secondary", "", "right", "Sticker siguiente");
    next.disabled = index === model.stickers.length - 1;
    next.addEventListener("click", () => actions.openSticker(index + 1));
    controls.append(previous, next);
    if (sticker.id !== null && model.pack !== null) {
      const favorite = button("button-secondary", sticker.favorite ? "Quitar de favoritos" : "Favorito", sticker.favorite ? "favorite-full" : "favorite");
      favorite.setAttribute("aria-pressed", String(sticker.favorite));
      favorite.disabled = view.busy !== null;
      favorite.addEventListener("click", actions.toggleStickerFavorite);
      controls.append(favorite);
      if (model.origin === "local") {
        const emoji = button("button-secondary", "Cambiar emoji", "emoji");
        emoji.disabled = view.busy !== null;
        emoji.dataset["focus"] = "change-emoji";
        emoji.addEventListener("click", actions.editEmojis);
        controls.append(emoji);
        const remove = button("button-secondary", "Quitar", "delete");
        remove.disabled = view.busy !== null;
        remove.addEventListener("click", actions.removeSticker);
        controls.append(remove);
      }
    }
    const close = button("button-secondary", "Cerrar", "close");
    close.dataset["focus"] = "close-layer";
    close.addEventListener("click", () => actions.openSticker(null));
    controls.append(close);
    card.append(controls);
    const options = stickerOptions(platform, model.pack, sticker.id);
    if (options.share || options.save || options.whatsapp) {
      const message = view.error ?? view.notice ?? view.busy;
      if (message !== null) {
        const state = create("p", view.error === null ? "small" : "small message-error", message);
        state.setAttribute("role", view.error === null ? "status" : "alert");
        card.append(state);
      }
      const sheet = create("div", "layer-sticker-sheet");
      if (options.whatsapp) {
        const send = button("button-main", "Enviar a WhatsApp como sticker", "export");
        send.dataset["focus"] = "send-sticker-whatsapp";
        send.disabled = view.busy !== null;
        send.addEventListener("click", actions.sendStickerToWhatsapp);
        sheet.append(send);
      }
      if (options.share) {
        const share = button("button-secondary", "Compartir como imagen", "export");
        share.title = SHARE_AS_IMAGE_HINT;
        share.setAttribute("aria-description", SHARE_AS_IMAGE_HINT);
        share.dataset["focus"] = "share-sticker";
        share.disabled = view.busy !== null;
        share.addEventListener("click", actions.shareSticker);
        sheet.append(share);
      }
      if (options.save) {
        const save = button("button-secondary", "Guardar como archivo", "save");
        save.dataset["focus"] = "save-sticker";
        save.disabled = view.busy !== null;
        save.addEventListener("click", actions.saveSticker);
        sheet.append(save);
      }
      card.append(sheet);
    }
    layer.append(card);
    return layer;
  }

  function moveFocusOfLayer(view: DetailView): void {
    const now = view.openSticker;
    if (now !== null && openLayer === null) {
      panel.querySelector<HTMLElement>('[data-focus="close-layer"]')?.focus({ preventScroll: true });
    } else if (now === null && openLayer !== null) {
      panel.querySelector<HTMLElement>(`[data-focus="sticker-${openLayer}"]`)?.focus({ preventScroll: true });
    }
    openLayer = now;
  }
}

type CurrentModel = Extract<DetailView["state"], { kind: "ready" }>["value"];

function activateLongPress(node: HTMLElement, onActivate: () => void): void {
  let timer: number | undefined;
  const cancel = (): void => window.clearTimeout(timer);
  node.addEventListener("pointerdown", (event) => {
    if (event.pointerType === "mouse") return;
    timer = window.setTimeout(onActivate, LONG_PRESS_MS);
  });
  for (const kind of ["pointerup", "pointerleave", "pointercancel"]) node.addEventListener(kind, cancel);
}
