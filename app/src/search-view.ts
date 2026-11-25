import { button, create, element, icon, renderFocusKeeping } from "./dom.ts";
import { createFinder, createState, createGrid, createStickersGrid, type FileUrl } from "./catalog-view.ts";
import { visibleRow } from "./klipy.ts";
import { createKlipyRow } from "./klipy-view.ts";
import type { SearchActions, SearchView } from "./views.ts";

export function createSearchView(actions: SearchActions, fileUrl: FileUrl): { render(view: SearchView): void } {
  const panel = element<HTMLElement>("panel-search");
  return {
    render(view) {
      renderFocusKeeping(panel, () => {
        const content = create("div", "stack");
        const back = button("button-back", "Atrás", "back", "Volver");
        back.dataset["focus"] = "back";
        back.addEventListener("click", actions.back);
        content.append(back);
        const title = create("h2", "title-view", "Buscar");
        title.id = "title-search";
        panel.setAttribute("aria-labelledby", title.id);
        panel.removeAttribute("aria-label");
        content.append(title, createFinder(view.history, actions.search, view.query, "finder-search"));

        if (view.history.length > 0 && view.query === "") {
          const history = create("section", "history");
          const heading = create("h3", "subtitle", "Búsquedas recientes");
          heading.id = "title-history";
          history.append(heading);
          const list = create("ul", "chips");
          list.setAttribute("aria-labelledby", heading.id);
          for (const text of view.history) {
            const item = create("li", "chip-with-closing");
            const search = button("chip", text, null);
            search.dataset["focus"] = `history-${text}`;
            search.addEventListener("click", () => actions.search(text));
            const remove = button("button-icon", "", "close", `Quitar «${text}» del historial`);
            remove.dataset["focus"] = `remove-${text}`;
            remove.addEventListener("click", () => actions.removeFromHistory(text));
            item.append(search, remove);
            list.append(item);
          }
          history.append(list);
          content.append(history);
        }

        renderResult(content, view);
        panel.replaceChildren(content);
      });
    },
  };

  function renderResult(content: HTMLElement, view: SearchView): void {
    if (view.query === "") {
      content.append(createState("Escribe un nombre, un autor, una etiqueta o un emoji.", "empty"));
      return;
    }
    if (view.state.kind === "loading") {
      content.append(createState("Buscando…", "loading"));
      return;
    }
    if (view.state.kind === "error") {
      content.append(createState(view.state.message, "error"));
      return;
    }
    const { packs, stickers } = view.state.value;
    const klipy = view.klipy !== null && visibleRow(view.klipy) ? view.klipy : null;
    if (packs.length === 0 && stickers.length === 0 && klipy === null) {
      content.append(createState(`No hay nada para «${view.query}».`, "empty"));
      return;
    }
    if (packs.length > 0 || stickers.length > 0) {
      const summary = create("p", "secondary small", `${packs.length} paquetes y ${stickers.length} stickers`);
      summary.setAttribute("role", "status");
      content.append(summary);
    }
    if (packs.length > 0) {
      const section = create("section", "stack");
      const heading = create("h3", "subtitle", "Paquetes");
      section.append(heading, createGrid(packs, fileUrl, actions.openPack));
      content.append(section);
    }
    if (stickers.length > 0) {
      const section = create("section", "stack");
      const heading = create("h3", "subtitle");
      heading.append(icon("emoji"), document.createTextNode(" Stickers"));
      section.append(heading, createStickersGrid(stickers, fileUrl, actions.openSticker));
      content.append(section);
    }
    if (klipy !== null) content.append(createKlipyRow(klipy, fileUrl, actions));
  }
}
