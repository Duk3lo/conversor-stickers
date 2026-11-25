import { button, create } from "./dom.ts";
import { KLIPY_ATTRIBUTION, NEW_CHOICE } from "./klipy.ts";
import type { FileUrl } from "./catalog-view.ts";
import type { KlipyStickerActions, KlipyStickerView } from "./views.ts";

export function createKlipyStickerView(
  actions: KlipyStickerActions,
  fileUrl: FileUrl,
): { render(view: KlipyStickerView | null): void } {
  let layer: HTMLElement | null = null;
  let shownId: string | null = null;
  let previous: Element | null = null;
  let parts: { send: HTMLButtonElement; close: HTMLButtonElement; selector: HTMLSelectElement; name: HTMLInputElement; progress: HTMLElement; error: HTMLElement } | null = null;

  function build(view: KlipyStickerView): HTMLElement {
    const { sticker } = view;
    const fresh = create("div", "layer-sticker layer-klipy");
    fresh.setAttribute("role", "dialog");
    fresh.setAttribute("aria-modal", "true");
    fresh.setAttribute("aria-label", `Sticker de KLIPY: ${sticker.title}`);
    fresh.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        actions.close();
      }
    });
    const card = create("form", "layer-sticker-card");
    card.noValidate = true;
    const large = create("img", "layer-sticker-image");
    large.src = fileUrl(sticker.thumbnail);
    large.alt = sticker.title;
    const titles = create("div", "layer-klipy-title");
    titles.append(create("strong", "", sticker.title), create("span", "badge badge-klipy", `${sticker.animated ? "Animado" : "Estático"} · ${KLIPY_ATTRIBUTION}`));

    const packField = create("label", "campo-en-linea campo-klipy");
    packField.append(create("span", "secondary small", "Añadir a un paquete"));
    const selector = document.createElement("select");
    selector.dataset["focus"] = "klipy-pack";
    for (const pack of view.packs) selector.append(new Option(pack.name, String(pack.id)));
    selector.append(new Option("Paquete nuevo…", NEW_CHOICE));
    selector.value = view.packs[0] === undefined ? NEW_CHOICE : String(view.packs[0].id);
    packField.append(selector);

    const nameField = create("label", "campo-en-linea campo-klipy");
    nameField.append(create("span", "secondary small", "Nombre del paquete nuevo"));
    const name = document.createElement("input");
    name.type = "text";
    name.maxLength = 128;
    name.autocomplete = "off";
    name.dataset["focus"] = "klipy-name";
    nameField.append(name);
    const toggleName = (): void => void (nameField.hidden = selector.value !== NEW_CHOICE);
    selector.addEventListener("change", toggleName);
    toggleName();

    const progress = create("div", "klipy-progress");
    progress.setAttribute("role", "status");
    progress.hidden = true;
    const bar = document.createElement("progress");
    bar.setAttribute("aria-label", "Añadiendo el sticker");
    progress.append(create("span", "secondary", "Descargando y convirtiendo el sticker…"), bar);
    const error = create("p", "message-error");
    error.setAttribute("role", "alert");
    error.hidden = true;

    const controls = create("div", "actions-row actions-centered");
    const close = button("button-secondary", "Cerrar", "close");
    close.dataset["focus"] = "klipy-close";
    close.addEventListener("click", actions.close);
    const send = button("button-main", "Añadir", "plus");
    send.type = "submit";
    send.dataset["focus"] = "klipy-add";
    controls.append(close, send);

    card.append(large, titles, packField, nameField, progress, error, controls);
    card.addEventListener("submit", (event) => {
      event.preventDefault();
      actions.add(selector.value, name.value);
    });
    fresh.addEventListener("click", (event) => {
      if (event.target === fresh) actions.close();
    });
    fresh.append(card);
    parts = { send, close, selector, name, progress, error };
    return fresh;
  }

  return {
    render(view) {
      if (view === null) {
        layer?.remove();
        layer = null;
        shownId = null;
        parts = null;
        if (previous instanceof HTMLElement) previous.focus({ preventScroll: true });
        previous = null;
        return;
      }
      if (layer === null || shownId !== view.sticker.id) {
        layer?.remove();
        if (previous === null) previous = document.activeElement;
        layer = build(view);
        shownId = view.sticker.id;
        document.body.append(layer);
        parts?.selector.focus();
      }
      if (parts === null) return;
      parts.send.disabled = view.busy;
      parts.send.setAttribute("aria-busy", String(view.busy));
      parts.selector.disabled = view.busy;
      parts.name.disabled = view.busy;
      parts.progress.hidden = !view.busy;
      parts.error.hidden = view.error === null;
      parts.error.textContent = view.error ?? "";
    },
  };
}
