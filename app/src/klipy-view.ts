import { button, create } from "./dom.ts";
import { KLIPY_ATTRIBUTION, type KlipyRow } from "./klipy.ts";
import type { FileUrl } from "./catalog-view.ts";
import type { KlipyActions } from "./views.ts";

export function createKlipyRow(row: KlipyRow, fileUrl: FileUrl, actions: KlipyActions): HTMLElement {
  const node = create("section", "lane lane-klipy");
  const titleId = `lane-klipy-${row.key}`;
  node.setAttribute("aria-labelledby", titleId);
  const header = create("div", "lane-header");
  const title = create("h3", "lane-title", row.title);
  title.id = titleId;
  header.append(title, create("span", "badge badge-klipy", `Stickers de ${KLIPY_ATTRIBUTION}`));
  node.append(header);

  const { state } = row;
  if (state.kind === "hidden") return node;
  if (state.kind === "loading") {
    const loading = create("p", "secondary small", "Cargando stickers…");
    loading.setAttribute("role", "status");
    node.append(loading);
    return node;
  }
  const list = create("ul", "lane-list");
  for (const sticker of state.results) {
    const item = create("li", "");
    const open = button("card-klipy", "", null, `Ver «${sticker.title}»${sticker.animated ? ", animado" : ""}`);
    open.dataset["focus"] = `klipy-${row.key}-${sticker.id}`;
    const thumbnail = create("img", "card-klipy-image");
    thumbnail.src = fileUrl(sticker.thumbnail);
    thumbnail.alt = "";
    thumbnail.loading = "lazy";
    open.append(thumbnail);
    open.addEventListener("click", () => actions.openKlipySticker(sticker));
    item.append(open);
    list.append(item);
  }
  if (state.next !== null) {
    const item = create("li", "");
    const plus = button("boton-secundario tarjeta-klipy-mas", state.loadingPlus ? "Cargando…" : "Ver más", null, `Ver más: ${row.title}`);
    plus.dataset["focus"] = `klipy-plus-${row.key}`;
    plus.disabled = state.loadingPlus;
    plus.setAttribute("aria-busy", String(state.loadingPlus));
    plus.addEventListener("click", () => actions.plusSeeKlipy(row.key));
    item.append(plus);
    list.append(item);
  }
  node.append(list);
  if (state.plusError !== null) {
    const error = create("p", "message-error", `No se pudieron cargar más: ${state.plusError}`);
    error.setAttribute("role", "alert");
    node.append(error);
  }
  return node;
}
