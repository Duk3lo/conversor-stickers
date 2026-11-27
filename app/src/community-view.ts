import { communityErrorText } from "./community.ts";
import { button, create, element, image, renderFocusKeeping } from "./dom.ts";
import type { FileUrl } from "./catalog-view.ts";
import { createNoticeWithoutConnection, createState } from "./catalog-view.ts";
import {
  communityEntries,
  entryAction,
  featuredEntries,
  filterChips,
  filterEntries,
  packsCountText,
  type CommunityEntry,
  type CommunitySort,
} from "./community-list.ts";
import type { CatalogUi } from "./contract-community.ts";
import type { LibraryUi } from "./contract.ts";
import type { Path } from "./navigation.ts";
import type { Load } from "./views.ts";

export interface CommunityView {
  readonly catalog: Load<CatalogUi>;
  readonly library: LibraryUi | null;
  readonly updated: string | null;
  readonly offline: boolean;
}

export interface CommunityActions {
  openPack(path: Path): void;
  update(): void;
}

const SortTexts: readonly { sort: CommunitySort; name: string }[] = [
  { sort: "recent", name: "Recientes" },
  { sort: "az", name: "A–Z" },
];

export function createCommunityView(actions: CommunityActions, fileUrl: FileUrl): { render(view: CommunityView): void } {
  const panel = element<HTMLElement>("panel-community");
  let query = "";
  let filter = "all";
  let sort: CommunitySort = "recent";
  let current: CommunityView | null = null;

  function render(view: CommunityView): void {
    current = view;
    renderFocusKeeping(panel, () => {
      const content = create("div", "stack community");
      content.append(createHeader(view));
      if (view.catalog.kind === "loading") {
        content.append(createState("Cargando el catálogo…", "loading"));
      } else if (view.catalog.kind === "error") {
        const text = communityErrorText(view.catalog.message);
        content.append(createState(text.community, "error", { text: "Reintentar", onPress: actions.update }, text.detail));
      } else {
        content.append(...createBody(view.catalog.value, view.library));
      }
      panel.replaceChildren(content);
    });
  }

  function createHeader(view: CommunityView): HTMLElement {
    const header = create("header", "community-header");
    const title = create("h2", "title-view", "Comunidad");
    title.id = "title-community";
    panel.setAttribute("aria-labelledby", title.id);
    panel.removeAttribute("aria-label");
    const texts = create("div", "community-header-texts");
    texts.append(title, create("p", "secondary", "Paquetes creados por la comunidad"));
    const facts: string[] = [];
    if (view.catalog.kind === "ready") facts.push(packsCountText(view.catalog.value.packs.length));
    if (view.updated !== null) facts.push(`Catálogo del ${view.updated}`);
    if (facts.length > 0) texts.append(create("p", "small secondary community-facts", facts.join(" · ")));
    header.append(texts);
    if (view.offline && view.updated !== null) {
      const notice = createNoticeWithoutConnection(`Sin conexión: catálogo del ${view.updated}.`, actions.update);
      notice.classList.add("community-notice");
      return wrap(header, notice);
    }
    return header;
  }

  function createBody(catalog: CatalogUi, library: LibraryUi | null): HTMLElement[] {
    const entries = communityEntries(catalog, library);
    if (entries.length === 0) return [createState("El catálogo no tiene paquetes todavía.", "empty")];
    const chips = filterChips(catalog);
    if (!chips.some((chip) => chip.key === filter)) filter = "all";
    const results = create("div", "community-results");
    const draw = (): void => {
      const list = filterEntries(entries, { query, filter, sort });
      const nodes: HTMLElement[] = [];
      if (query.trim() === "" && filter === "all") nodes.push(createFeatured(entries));
      nodes.push(createListHeader(list.length, draw));
      nodes.push(list.length === 0 ? createState("Ningún paquete coincide con la búsqueda.", "empty") : createList(list));
      results.replaceChildren(...nodes);
    };
    draw();
    return [createSearchField(draw), createFilters(chips), results];
  }

  function wrap(...nodes: HTMLElement[]): HTMLElement {
    const box = create("div", "community-top");
    box.append(...nodes);
    return box;
  }

  function createSearchField(redraw: () => void): HTMLElement {
    const form = create("form", "finder");
    form.setAttribute("role", "search");
    form.noValidate = true;
    const label = create("label", "hidden-visual", "Buscar en la comunidad");
    label.htmlFor = "community-text";
    const field = document.createElement("input");
    field.type = "search";
    field.id = "community-text";
    field.className = "finder-text";
    field.placeholder = "Buscar paquetes por nombre, autor o etiqueta";
    field.autocomplete = "off";
    field.enterKeyHint = "search";
    field.maxLength = 100;
    field.value = query;
    field.dataset["focus"] = "community-text";
    field.addEventListener("input", () => {
      query = field.value;
      redraw();
    });
    form.addEventListener("submit", (event) => event.preventDefault());
    form.append(label, field);
    return form;
  }

  function createFilters(chips: readonly { key: string; name: string }[]): HTMLElement {
    const group = create("div", "chips chips-row");
    group.setAttribute("role", "group");
    group.setAttribute("aria-label", "Filtrar paquetes de la comunidad");
    for (const chip of chips) {
      const node = button("chip", chip.name, null);
      node.setAttribute("aria-pressed", String(chip.key === filter));
      node.dataset["focus"] = `community-chip-${chip.key}`;
      node.addEventListener("click", () => {
        filter = chip.key;
        if (current !== null) render(current);
      });
      group.append(node);
    }
    return group;
  }

  function createFeatured(entries: readonly CommunityEntry[]): HTMLElement {
    const section = create("section", "lane");
    section.setAttribute("aria-labelledby", "community-news");
    const heading = create("h3", "lane-title", "Novedades");
    heading.id = "community-news";
    const list = create("ul", "lane-list");
    for (const entry of featuredEntries(entries)) {
      const item = create("li", "featured-item");
      const open = button("featured", "", null, `${entry.card.name}, de ${entry.card.author}, novedad`);
      open.append(image(entry.card.icon === null ? null : fileUrl(entry.card.icon), "featured-icon"));
      open.append(create("strong", "card-name", entry.card.name));
      open.append(create("span", "secondary small", entry.card.author));
      open.append(create("span", "secondary small", `${entry.card.stickers} stickers · ${entry.card.added}`));
      open.addEventListener("click", () => actions.openPack({ kind: "pack", origin: "community", id: entry.card.id }));
      item.append(open);
      list.append(item);
    }
    section.append(heading, list);
    return section;
  }

  function createListHeader(count: number, redraw: () => void): HTMLElement {
    const row = create("div", "community-list-header");
    const heading = create("h3", "lane-title", `Todos los paquetes (${count})`);
    const group = create("div", "chips");
    group.setAttribute("role", "group");
    group.setAttribute("aria-label", "Ordenar paquetes");
    for (const option of SortTexts) {
      const node = button("chip", option.name, null);
      node.setAttribute("aria-pressed", String(option.sort === sort));
      node.addEventListener("click", () => {
        sort = option.sort;
        redraw();
      });
      group.append(node);
    }
    row.append(heading, group);
    return row;
  }

  function createList(list: readonly CommunityEntry[]): HTMLElement {
    const nodes = create("ul", "community-list");
    for (const entry of list) nodes.append(createEntry(entry));
    return nodes;
  }

  function createEntry(entry: CommunityEntry): HTMLLIElement {
    const item = document.createElement("li");
    item.className = "community-entry";
    const action = entryAction(entry);
    const info = button("community-entry-info", "", null, `${entry.card.name}, de ${entry.card.author}. Ver detalle`);
    info.dataset["focus"] = `community-info-${entry.card.id}`;
    info.append(image(entry.card.icon === null ? null : fileUrl(entry.card.icon), "community-entry-icon"));
    const texts = create("span", "community-entry-texts");
    texts.append(create("strong", "card-name", entry.card.name));
    const detail = [entry.card.author, entry.license, `${entry.card.stickers} stickers`].filter((part) => part !== "");
    texts.append(create("span", "secondary small", detail.join(" · ")));
    const badges = create("span", "badges");
    if (entry.card.animated !== null) badges.append(create("span", "badge", entry.card.animated ? "Animado" : "Estático"));
    if (entry.withdrawn) badges.append(create("span", "badge badge-danger", "Retirado"));
    if (badges.childElementCount > 0) texts.append(badges);
    info.append(texts);
    info.addEventListener("click", () =>
      actions.openPack({ kind: "pack", origin: "community", id: entry.card.id }),
    );
    const act = button(`community-action ${action.kind === "open" ? "button-secondary" : "button-main"}`, action.text, null, `${action.text} «${entry.card.name}»`);
    act.dataset["focus"] = `community-action-${entry.card.id}`;
    act.addEventListener("click", () => actions.openPack(action.path));
    item.append(info, act);
    return item;
  }

  return { render };
}
