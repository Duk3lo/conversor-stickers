import { button, create, element, renderFocusKeeping } from "./dom.ts";
import { createState, createGrid, type FileUrl } from "./catalog-view.ts";
import type { CategoryActions, CategoryView } from "./views.ts";

export function createCategoryView(actions: CategoryActions, fileUrl: FileUrl): { render(view: CategoryView): void } {
  const panel = element<HTMLElement>("panel-category");
  return {
    render(view) {
      renderFocusKeeping(panel, () => {
        const content = create("div", "stack");
        const back = button("button-back", "Atrás", "back", "Volver");
        back.dataset["focus"] = "back";
        back.addEventListener("click", actions.back);
        content.append(back);
        const title = create("h2", "title-view", view.state.kind === "ready" ? view.state.value.name : "Categoría");
        title.id = "title-category";
        panel.setAttribute("aria-labelledby", title.id);
        panel.removeAttribute("aria-label");
        content.append(title);
        if (view.state.kind === "loading") content.append(createState("Cargando…", "loading"));
        else if (view.state.kind === "error") content.append(createState(view.state.message, "error"));
        else if (view.state.value.cards.length === 0) content.append(createState("No hay paquetes en esta categoría.", "empty"));
        else content.append(createGrid(view.state.value.cards, fileUrl, actions.openPack));
        panel.replaceChildren(content);
      });
    },
  };
}
