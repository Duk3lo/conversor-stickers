import { sizeText } from "./queue.ts";
import { pressedButton, create, element, icon, showMessage } from "./dom.ts";
import { imageSecondsText, imagesChosenText } from "./images-gif.ts";
import type { GifsActions, CreatedGif, GifsView } from "./views.ts";

export function createGifsView(actions: GifsActions): { render(view: GifsView): void } {
  const videoButton = element<HTMLButtonElement>("gif-video");
  const videoTitle = element<HTMLSpanElement>("gif-video-title");
  const imagesButton = element<HTMLButtonElement>("gif-images");
  const imagesTitle = element<HTMLSpanElement>("gif-images-title");
  const form = element<HTMLFormElement>("gif-images-form");
  const count = element<HTMLParagraphElement>("gif-images-count");
  const timeField = element<HTMLDivElement>("gif-images-time-field");
  const time = element<HTMLInputElement>("gif-images-time");
  const timeText = element<HTMLOutputElement>("gif-images-time-text");
  const repeatField = element<HTMLLabelElement>("gif-images-repeat-field");
  const repeat = element<HTMLInputElement>("gif-images-repeat");
  const createButton = element<HTMLButtonElement>("gif-images-create");
  const discardButton = element<HTMLButtonElement>("gif-images-discard");
  const error = element<HTMLDivElement>("gifs-error");
  const errorText = element<HTMLParagraphElement>("gifs-error-text");
  const empty = element<HTMLParagraphElement>("gifs-empty");
  const list = element<HTMLUListElement>("list-gifs");

  videoButton.addEventListener("click", () => actions.pickVideo());
  imagesButton.addEventListener("click", () => actions.pickImages());
  time.addEventListener("input", () => actions.setImageSeconds(Number(time.value)));
  repeat.addEventListener("change", () => actions.setRepeat(repeat.checked));
  discardButton.addEventListener("click", () => actions.discardImages());
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    actions.createFromImages();
  });
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
      const busy = view.choosing || view.choosingImages || view.images?.creating === true;
      videoButton.disabled = busy;
      videoButton.setAttribute("aria-busy", String(view.choosing));
      videoTitle.textContent = view.choosing ? "Eligiendo un video…" : "Desde un video";
      imagesButton.disabled = busy;
      imagesButton.setAttribute("aria-busy", String(view.choosingImages));
      imagesTitle.textContent = view.choosingImages ? "Eligiendo imágenes…" : "Desde imágenes";
      const images = view.images;
      form.hidden = images === null;
      if (images !== null) {
        const several = images.ids.length > 1;
        count.textContent = imagesChosenText(images.ids.length);
        timeField.hidden = !several;
        repeatField.hidden = !several;
        time.value = String(images.imageSeconds);
        timeText.textContent = imageSecondsText(images.imageSeconds);
        repeat.checked = images.repeat;
        time.disabled = images.creating;
        repeat.disabled = images.creating;
        createButton.disabled = images.creating;
        createButton.textContent = images.creating ? "Creando…" : "Crear GIF";
        discardButton.disabled = images.creating;
      }
      showMessage(errorText, view.error);
      error.hidden = view.error === null;
      empty.hidden = view.created.length > 0;
      const focused = document.activeElement?.closest<HTMLLIElement>("li[data-entry]")?.dataset.entry;
      list.replaceChildren(...view.created.map(row));
      if (focused) list.querySelector<HTMLButtonElement>(`li[data-entrada="${focused}"] button`)?.focus();
    },
  };
}
