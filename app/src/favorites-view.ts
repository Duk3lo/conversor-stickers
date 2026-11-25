import { button, create, element, renderFocusKeeping } from "./dom.ts";
import { createState, createGrid, createStickersGrid, type FileUrl } from "./catalog-view.ts";
import type { FavoritesActions, FavoritesView } from "./views.ts";

export function createFavoritesView(actions: FavoritesActions, fileUrl: FileUrl): { render(view: FavoritesView): void } {
  const panel = element<HTMLElement>("panel-favorites");
  return {
    render(view) {
      renderFocusKeeping(panel, () => {
        const content = create("div", "stack");
        const title = create("h2", "title-view", "Favoritos");
        title.id = "title-favorites";
        panel.setAttribute("aria-labelledby", title.id);
        panel.removeAttribute("aria-label");
        content.append(title);

        if (view.state.kind === "loading") {
          content.append(createState("Cargando tus favoritos…", "loading"));
        } else if (view.state.kind === "error") {
          content.append(createState(view.state.message, "error"));
        } else {
          const model = view.state.value;
          if (model.packs.length === 0 && model.stickers.length === 0) {
            content.append(createState("Todavía no has marcado favoritos. Pulsa la estrella en un paquete o en un sticker.", "empty"));
            const go = button("button-secondary button-centered", "Ver mis paquetes", "start");
            go.addEventListener("click", actions.goToStart);
            content.append(go);
          } else {
            if (model.stickers.length > 0) content.append(createCreate(view, model.note, model.stickers.length));
            if (model.packs.length > 0) {
              const section = create("section", "stack");
              section.append(create("h3", "subtitle", "Paquetes"), createGrid(model.packs, fileUrl, actions.openPack));
              content.append(section);
            }
            if (model.stickers.length > 0) {
              const section = create("section", "stack");
              section.append(create("h3", "subtitle", "Stickers"), createStickersGrid(model.stickers, fileUrl, actions.openSticker));
              content.append(section);
            }
          }
        }
        if (view.error !== null) {
          const error = create("div", "panel-error");
          error.setAttribute("role", "alert");
          error.append(create("p", "", view.error));
          content.append(error);
        }
        panel.replaceChildren(content);
      });
    },
  };

  function createCreate(view: FavoritesView, note: string | null, howMany: number): HTMLElement {
    const block = create("section", "block");
    block.append(create("p", "secondary", `${howMany} ${howMany === 1 ? "sticker favorito" : "stickers favoritos"}.`));
    if (note !== null) block.append(create("p", "secondary small", note));
    if (!view.form) {
      const open = button("button-main", "Crear paquete con mis favoritos", "plus");
      open.dataset["focus"] = "create-favorites";
      open.disabled = view.busy;
      open.addEventListener("click", actions.openForm);
      block.append(open);
      return block;
    }
    const form = create("form", "form-line");
    form.noValidate = true;
    const label = create("label", "", "Nombre del paquete");
    label.htmlFor = "favorites-name";
    const row = create("div", "form-line-row");
    const field = document.createElement("input");
    field.id = "favorites-name";
    field.type = "text";
    field.value = "Mis favoritos";
    field.maxLength = 128;
    field.autocomplete = "off";
    field.dataset["focus"] = "favorites-name";
    if (view.formError !== null) field.setAttribute("aria-describedby", "favorites-error");
    const send = button("button-main", view.busy ? "Creando…" : "Crear", null);
    send.type = "submit";
    send.disabled = view.busy;
    const cancel = button("button-text", "Cancelar", null);
    cancel.addEventListener("click", actions.closeForm);
    row.append(field, send, cancel);
    form.append(label, row);
    if (view.formError !== null) {
      const error = create("p", "message-error", view.formError);
      error.id = "favorites-error";
      error.setAttribute("role", "alert");
      form.append(error);
    }
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      actions.createPack(field.value);
    });
    block.append(form);
    return block;
  }
}
