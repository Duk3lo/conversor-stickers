import { sizeText } from "./queue.ts";
import { pressedButton, create, element, icon, showMessage } from "./dom.ts";
import type { GifsActions, CreatedGif, GifsView } from "./views.ts";

export function createGifsView(actions: GifsActions): { render(view: GifsView): void } {
  const videoButton = element<HTMLButtonElement>("gif-video");
  const videoTitle = element<HTMLSpanElement>("gif-video-title");
  const error = element<HTMLDivElement>("gifs-error");
  const errorText = element<HTMLParagraphElement>("gifs-error-text");
  const empty = element<HTMLParagraphElement>("gifs-empty");
  const list = element<HTMLUListElement>("list-gifs");

  videoButton.addEventListener("click", () => actions.pickVideo());
  list.addEventListener("click", (event) => {
    const button = pressedButton(event);
    const row = button?.closest<HTMLLIElement>("li[data-entry]");
    if (row) actions.save(Number(row.dataset.entry));
  });

  function row(gif: CreatedGif): HTMLLIElement {
    const node = create("li", "row-gif");
    node.dataset.entry = String(gif.entry);
    const shell = create("span", "thumbnail");
    const image = create("img", "");
    image.src = gif.image;
    image.alt = "";
    shell.append(image);
    const data = create("div", "row-gif-data");
    const name = create("span", "row-name", gif.name);
    name.title = gif.name;
    data.append(name, create("span", "secondary small", `${gif.destination} · ${sizeText(gif.sizeBytes)}`));
    const state = create("span", "small");
    state.setAttribute("aria-live", "polite");
    if (gif.saveError !== null) {
      state.classList.add("message-error");
      state.textContent = gif.saveError;
    } else if (gif.saved !== null) {
      state.classList.add("secondary");
      state.textContent = `Guardado como ${gif.saved}`;
    }
    data.append(state);
    const save = create("button", "button-secondary button-small");
    save.type = "button";
    save.disabled = gif.saving;
    save.append(icon("save"), document.createTextNode(gif.saving ? "Guardando…" : "Guardar"));
    save.setAttribute("aria-label", `Guardar ${gif.name}`);
    node.append(shell, data, save);
    return node;
  }

  return {
    render(view) {
      videoButton.disabled = view.choosing;
      videoButton.setAttribute("aria-busy", String(view.choosing));
      videoTitle.textContent = view.choosing ? "Eligiendo un video…" : "Video";
      showMessage(errorText, view.error);
      error.hidden = view.error === null;
      empty.hidden = view.created.length > 0;
      const focused = document.activeElement?.closest<HTMLLIElement>("li[data-entry]")?.dataset.entry;
      list.replaceChildren(...view.created.map(row));
      if (focused) list.querySelector<HTMLButtonElement>(`li[data-entrada="${focused}"] button`)?.focus();
    },
  };
}
