import type { CategoryRow, PackCard } from "./catalog.ts";
import { pathOfCard } from "./catalog.ts";
import { button, create, icon, image } from "./dom.ts";
import type { StickerResult } from "./search.ts";
import type { Path } from "./navigation.ts";

export type FileUrl = (path: string) => string;

export function cardDescription(card: PackCard): string {
  const parts = [card.name];
  if (card.author !== "") parts.push(`de ${card.author}`);
  parts.push(card.stickers === 1 ? "1 sticker" : `${card.stickers} stickers`);
  if (card.animated !== null) parts.push(card.animated ? "animated" : "estático");
  parts.push(...card.marks.map((mark) => mark.toLowerCase()));
  if (card.favorite) parts.push("favorite");
  return parts.join(", ");
}

export function createCard(card: PackCard, fileUrl: FileUrl, onOpen: (path: Path) => void): HTMLLIElement {
  const element = document.createElement("li");
  element.className = "card-item";
  const open = button("card", "", null, cardDescription(card));
  open.dataset["focus"] = `card-${card.key}`;

  const header = create("span", "card-header");
  header.append(image(card.icon === null ? null : fileUrl(card.icon), "card-icon"));
  const titles = create("span", "card-titles");
  titles.append(create("strong", "card-name", card.name));
  const detail = [card.author, card.animated === null ? "" : card.animated ? "Animado" : "Estático"].filter((part) => part !== "");
  titles.append(create("span", "secondary small card-author", detail.join(" · ")));
  header.append(titles);
  if (card.favorite) {
    const star = icon("favorite-full", "icon card-favorite");
    header.append(star);
  }
  open.append(header);

  if (card.samples.length > 0) {
    const samples = create("span", "card-samples");
    for (const path of card.samples) samples.append(image(fileUrl(path), "card-sample"));
    open.append(samples);
  } else {
    open.append(create("span", "secondary small card-count", `${card.stickers} stickers`));
  }
  if (card.marks.length > 0) {
    const marks = create("span", "badges");
    for (const text of card.marks) {
      const badge = create("span", `badge${text === "Retirado" ? " badge-danger" : text === "Actualización" ? " badge-accent" : ""}`, text);
      marks.append(badge);
    }
    open.append(marks);
  }
  open.addEventListener("click", () => onOpen(pathOfCard(card)));
  element.append(open);
  return element;
}

export function createRow(
  title: string,
  cards: readonly PackCard[],
  fileUrl: FileUrl,
  onOpen: (path: Path) => void,
  seeAll: (() => void) | null,
  key: string,
): HTMLElement {
  const row = create("section", "lane");
  const titleId = `lane-${key}`;
  row.setAttribute("aria-labelledby", titleId);
  const header = create("div", "lane-header");
  const heading = create("h3", "lane-title", title);
  heading.id = titleId;
  header.append(heading);
  if (seeAll !== null) {
    const see = button("button-text lane-see-all", "Ver todo", "next", `Ver todo: ${title}`);
    see.dataset["focus"] = `see-all-${key}`;
    see.addEventListener("click", seeAll);
    header.append(see);
  }
  row.append(header);
  const list = create("ul", "lane-list");
  for (const card of cards) list.append(createCard(card, fileUrl, onOpen));
  row.append(list);
  return row;
}

export function createRowOfCategory(
  row: CategoryRow,
  fileUrl: FileUrl,
  onOpen: (path: Path) => void,
  onAllSee: (path: Path) => void,
): HTMLElement {
  return createRow(row.name, row.cards, fileUrl, onOpen, () => onAllSee(row.path), row.key);
}

export function createGrid(cards: readonly PackCard[], fileUrl: FileUrl, onOpen: (path: Path) => void): HTMLElement {
  const list = create("ul", "grid-cards");
  for (const card of cards) list.append(createCard(card, fileUrl, onOpen));
  return list;
}

export function createChips(
  chips: readonly { readonly key: string; readonly name: string }[],
  chosen: string | null,
  onPick: (key: string | null) => void,
): HTMLElement {
  const group = create("div", "chips chips-row");
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", "Filtrar por categoría");
  const all = [{ key: "", name: "Todo" }, ...chips];
  for (const chip of all) {
    const node = button("chip", chip.name, null);
    const active = (chip.key === "" ? null : chip.key) === chosen;
    node.setAttribute("aria-pressed", String(active));
    node.dataset["focus"] = `chip-${chip.key}`;
    node.addEventListener("click", () => onPick(chip.key === "" ? null : chip.key));
    group.append(node);
  }
  return group;
}

export function createFinder(
  history: readonly string[],
  onSearch: (text: string) => void,
  query = "",
  uniqueId = "finder",
): HTMLFormElement {
  const form = create("form", "finder");
  form.setAttribute("role", "search");
  form.noValidate = true;
  const label = create("label", "hidden-visual", "Buscar paquetes y stickers");
  label.htmlFor = `${uniqueId}-text`;
  const field = document.createElement("input");
  field.type = "search";
  field.id = `${uniqueId}-text`;
  field.className = "finder-text";
  field.placeholder = "Buscar por nombre, autor, etiqueta o emoji";
  field.autocomplete = "off";
  field.enterKeyHint = "search";
  field.maxLength = 100;
  field.value = query;
  field.dataset["focus"] = `${uniqueId}-text`;
  if (history.length > 0) {
    const suggestions = document.createElement("datalist");
    suggestions.id = `${uniqueId}-history`;
    for (const text of history) suggestions.append(new Option(text));
    field.setAttribute("list", suggestions.id);
    form.append(suggestions);
  }
  const send = button("button-main finder-send", "", "search", "Buscar");
  send.type = "submit";
  form.append(label, field, send);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    onSearch(field.value);
  });
  return form;
}

export function createState(text: string, kind: "loading" | "empty" | "error", retry?: { text: string; onPress: () => void }, detail: string | null = null): HTMLElement {
  const node = create("div", `state-empty${kind === "error" ? " state-error" : ""}`);
  node.setAttribute("role", kind === "error" ? "alert" : "status");
  node.append(create("p", "", text));
  if (detail !== null) {
    const dropdown = create("details", "state-detail");
    dropdown.append(create("summary", "", "Detalle técnico"), create("p", "small secondary", detail));
    node.append(dropdown);
  }
  if (retry !== undefined) {
    const action = button("button-secondary button-centered", retry.text, null);
    action.dataset["focus"] = "state-action";
    action.addEventListener("click", retry.onPress);
    node.append(action);
  }
  return node;
}

export function createAvailableNoCommunityLine(text: string, onGoToCommunity: () => void): HTMLElement {
  const line = create("p", "small secondary line-community");
  line.setAttribute("role", "status");
  const link = button("button-link", "Ver Comunidad", null);
  link.addEventListener("click", onGoToCommunity);
  line.append(`${text} `, link);
  return line;
}

export function createNoticeWithoutConnection(text: string, onRetry: (() => void) | null): HTMLElement {
  const notice = create("div", "panel-aviso aviso-sin-conexion");
  notice.setAttribute("role", "status");
  notice.append(icon("without-connection"), create("p", "", text));
  if (onRetry !== null) {
    const retry = button("button-secondary button-small", "Reintentar", null);
    retry.dataset["focus"] = "retry-catalog";
    retry.addEventListener("click", onRetry);
    notice.append(retry);
  }
  return notice;
}

export function createStickersGrid(
  stickers: readonly StickerResult[],
  fileUrl: FileUrl,
  onOpen: (sticker: StickerResult) => void,
): HTMLElement {
  const list = create("ul", "grid-stickers-detail");
  for (const sticker of stickers) {
    const item = create("li", "sticker-detail");
    const open = button("sticker-detail-button", "", null, `Sticker ${sticker.emojis.join("")} del paquete ${sticker.namePack}`);
    open.dataset["focus"] = `sticker-loose-${sticker.pack}-${sticker.id}`;
    open.append(image(fileUrl(sticker.path), "sticker-detail-image"), create("span", "sticker-detail-emoji", sticker.emojis.join("")));
    const label = create("span", "secondary small card-author label-sticker", sticker.namePack);
    label.title = sticker.namePack;
    open.append(label);
    open.addEventListener("click", () => onOpen(sticker));
    item.append(open);
    list.append(item);
  }
  return list;
}
