import { communityErrorText } from "./community.ts";
import { button, create, element, renderFocusKeeping } from "./dom.ts";
import {
  createNoticeWithoutConnection,
  createFinder,
  createChips,
  createState,
  createRow,
  createRowOfCategory,
  createAvailableNoCommunityLine,
  type FileUrl,
} from "./catalog-view.ts";
import { splitTrending, type KlipyRow } from "./klipy.ts";
import { createKlipyRow } from "./klipy-view.ts";
import { platform } from "./platform.ts";
import type { ExplorerActions, ExplorerView } from "./views.ts";

const EMPTY_TEXT = "Todavía no hay paquetes. Crea uno en Crear o descubre los de la comunidad.";

export function createExplorerView(
  actions: ExplorerActions,
  fileUrl: FileUrl,
): { render(view: ExplorerView): void } {
  const panel = element<HTMLElement>("panel-start");

  return {
    render(view) {
      renderFocusKeeping(panel, () => {
        const content = create("div", "stack");
        const title = create("h2", "title-view", "Inicio");
        title.id = "title-start";
        content.append(title);
        panel.setAttribute("aria-labelledby", title.id);
        panel.removeAttribute("aria-label");
        content.append(createFinder(view.history, actions.search, "", "finder-start"));
        if (view.withoutConnection !== null) {
          content.append(createNoticeWithoutConnection(`Sin conexión: catálogo del ${view.withoutConnection}.`, actions.update));
        } else if (view.catalogError !== null) {
          content.append(createAvailableNoCommunityLine(communityErrorText(view.catalogError).start, actions.goToCommunity));
        }
        renderContent(content, view);
        panel.replaceChildren(content);
      });
    },
  };

  function renderContent(content: HTMLElement, view: ExplorerView): void {
    const { state } = view;
    if (state.kind === "loading") {
      content.append(createState("Cargando tus paquetes…", "loading"));
      return;
    }
    if (state.kind === "error") {
      const text = communityErrorText(state.message);
      content.append(createState(text.community, "error", { text: "Reintentar", onPress: actions.update }, text.detail));
      return;
    }
    const model = state.value;
    const trending = model.chip === null ? splitTrending(view.klipy) : null;
    if (model.chips.length > 0) content.append(createChips(model.chips, model.chip, actions.pickChip));
    if (trending !== null) renderKlipy(content, trending.first);
    if (model.rows.length === 0 && model.follow.length === 0) {
      content.append(createState(EMPTY_TEXT, "empty"));
      if (platform !== "android") {
        const buttons = create("div", "actions-row actions-centered");
        const createPack = button("button-main", "Crear un sticker", "create");
        createPack.addEventListener("click", actions.goToCreate);
        const community = button("button-secondary", "Ver la comunidad", "community");
        community.addEventListener("click", actions.goToCommunity);
        buttons.append(createPack, community);
        content.append(buttons);
      }
      renderKlipy(content, trending?.rest ?? view.klipy);
      return;
    }
    if (model.follow.length > 0 && model.chip === null) {
      content.append(createRow("Seguir editando", model.follow, fileUrl, actions.openPack, null, "follow"));
    }
    if (model.chip === null) renderKlipy(content, trending?.rest ?? view.klipy);
    content.append(...model.rows.map((row) => createRowOfCategory(row, fileUrl, actions.openPack, actions.seeAll)));
  }

  function renderKlipy(content: HTMLElement, rows: readonly KlipyRow[]): void {
    for (const row of rows) content.append(createKlipyRow(row, fileUrl, actions));
  }
}
