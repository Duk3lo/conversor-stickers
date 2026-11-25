import { visibleAttribution, keyLink, providerName } from "./gif-search.ts";
import type { GifProvider, GifResultUi } from "./contract-gifs.ts";
import { keyOriginText } from "./klipy.ts";
import { button, pressedButton, create, element, showMessage } from "./dom.ts";
import type { GifSearchActions, GifSearchView } from "./views.ts";

function providerEs(value: string): value is GifProvider {
  return value === "klipy" || value === "giphy";
}

export function createGifSearchView(
  actions: GifSearchActions,
  fileUrl: (path: string) => string,
): { render(view: GifSearchView): void } {
  const settingsButton = element<HTMLButtonElement>("bgif-settings");
  const settingsForm = element<HTMLFormElement>("bgif-form-settings");
  const providerSelect = element<HTMLSelectElement>("bgif-provider");
  const keyField = element<HTMLInputElement>("bgif-key");
  const keyState = element<HTMLParagraphElement>("bgif-key-state");
  const link = element<HTMLAnchorElement>("bgif-key-link");
  const saveButton = element<HTMLButtonElement>("bgif-save");
  const deleteButton = element<HTMLButtonElement>("bgif-delete");
  const settingsError = element<HTMLParagraphElement>("bgif-settings-error");
  const finder = element<HTMLFormElement>("bgif-finder");
  const textField = element<HTMLInputElement>("bgif-text");
  const title = element<HTMLHeadingElement>("bgif-title");
  const state = element<HTMLDivElement>("bgif-state");
  const grid = element<HTMLUListElement>("bgif-grid");
  const zonePlus = element<HTMLDivElement>("bgif-plus");
  const attribution = element<HTMLParagraphElement>("bgif-attribution");
  const useError = element<HTMLParagraphElement>("bgif-use-error");

  let openSettings = false;
  let shownProvider: string | null = null;
  let paintedResults: readonly GifResultUi[] | null = null;
  let paintedAttribution = "";

  function updateLink(): void {
    const provider = providerEs(providerSelect.value) ? providerSelect.value : "klipy";
    link.href = keyLink(provider);
    link.textContent = `Conseguir una clave de ${providerName(provider)} (${keyLink(provider).replace("https://", "")})`;
    link.setAttribute("aria-label", `Conseguir una clave de ${providerName(provider)} (se abre en el navegador)`);
  }

  settingsButton.addEventListener("click", () => actions.toggleSettings());
  providerSelect.addEventListener("change", updateLink);
  settingsForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (providerEs(providerSelect.value)) actions.saveSettings(providerSelect.value, keyField.value);
  });
  deleteButton.addEventListener("click", () => {
    if (providerEs(providerSelect.value)) actions.saveSettings(providerSelect.value, null);
  });
  finder.addEventListener("submit", (event) => {
    event.preventDefault();
    actions.search(textField.value);
  });
  textField.addEventListener("input", () => actions.write(textField.value));
  state.addEventListener("click", (event) => {
    const pressed = pressedButton(event);
    if (pressed?.dataset["action"] === "settings") actions.toggleSettings();
    else if (pressed?.dataset["action"] === "retry") actions.retry();
  });
  zonePlus.addEventListener("click", (event) => {
    if (pressedButton(event) !== null) actions.loadPlus();
  });
  grid.addEventListener("click", (event) => {
    const id = pressedButton(event)?.closest<HTMLLIElement>("li[data-id]")?.dataset["id"];
    if (id !== undefined) actions.use(id);
  });

  function card(result: GifResultUi, attributionLabel: string): HTMLLIElement {
    const node = create("li", "card-gif");
    node.dataset["id"] = result.id;
    const press = create("button", "button-card-gif");
    press.type = "button";
    press.setAttribute("aria-label", result.title === "" ? "Usar este GIF" : `Usar «${result.title}»`);
    const image = create("img", "");
    image.src = fileUrl(result.thumbnail);
    image.alt = "";
    image.loading = "lazy";
    if (result.width > 0 && result.height > 0) image.style.aspectRatio = `${result.width} / ${result.height}`;
    press.append(image);
    if (attributionLabel !== "") {
      const mark = create("span", "gif-attribution", attributionLabel);
      mark.setAttribute("aria-hidden", "true");
      press.append(mark);
    }
    node.append(press);
    return node;
  }

  function stateBlock(view: GifSearchView): HTMLElement | null {
    const box = create("div", "state-empty");
    const { search } = view;
    if (search.kind === "loading") {
      box.setAttribute("role", "status");
      box.append(create("p", "", "Buscando…"));
      return box;
    }
    if (search.kind === "ready") {
      if (search.results.length > 0) return null;
      const text = view.text.trim();
      box.append(create("p", "", text === "" ? "No hay tendencias ahora mismo." : `Sin resultados para «${text}». Prueba con otra palabra.`));
      return box;
    }
    box.classList.add("state-error");
    box.setAttribute("role", "alert");
    const name = view.provider.kind === "ready" && view.provider.value.provider !== null ? providerName(view.provider.value.provider) : "un proveedor";
    switch (search.reason) {
      case "without-key":
        box.append(
          create("p", "", `Para buscar GIF necesitas una clave gratuita de KLIPY o GIPHY. Elige el proveedor, pega tu clave en los ajustes y listo.`),
          create("p", "secondary small", "La clave se guarda solo en este dispositivo."),
        );
        box.append(actionButton("settings", "Poner la clave", "edit"));
        break;
      case "key-invalid":
        box.append(create("p", "", `La clave de ${name} no es válida. Revísala o consigue otra.`));
        box.append(actionButton("settings", "Cambiar la clave", "edit"));
        break;
      case "limit":
        box.append(create("p", "", "Límite de búsquedas alcanzado: espera un minuto y vuelve a intentarlo."));
        box.append(actionButton("retry", "Reintentar", "next"));
        break;
      case "without-connection":
        box.append(create("p", "", "Sin conexión: no se puede buscar GIF ahora mismo."));
        box.append(actionButton("retry", "Reintentar", "without-connection"));
        break;
      default:
        box.append(create("p", "", search.message));
        box.append(actionButton("retry", "Reintentar", "next"));
    }
    return box;
  }

  function actionButton(action: string, text: string, iconName: string): HTMLButtonElement {
    const node = button("button-secondary button-small", text, iconName);
    node.dataset["action"] = action;
    return node;
  }

  return {
    render(view) {
      settingsButton.setAttribute("aria-expanded", String(view.settings));
      settingsForm.hidden = !view.settings;
      if (openSettings && !view.settings) keyField.value = "";
      openSettings = view.settings;
      const provider = view.provider.kind === "ready" ? view.provider.value : null;
      if (provider?.provider != null && provider.provider !== shownProvider) {
        providerSelect.value = provider.provider;
        shownProvider = provider.provider;
      }
      updateLink();
      const saved = provider?.key_set === true && provider.provider === providerSelect.value;
      const keyOrigin = keyOriginText(provider?.key_origin ?? null);
      keyState.textContent =
        saved && provider.provider !== null
          ? provider.key_origin === "proxy"
            ? `${keyOrigin}: las búsquedas de ${providerName(provider.provider)} pasan por el servicio de la app.`
            : `${keyOrigin ?? "Clave guardada"}: hay una clave de ${providerName(provider.provider)} en uso.`
          : "Todavía no hay clave guardada para este proveedor.";
      saveButton.disabled = view.savingSettings;
      deleteButton.disabled = view.savingSettings || provider?.key_origin !== "user";
      saveButton.setAttribute("aria-busy", String(view.savingSettings));
      showMessage(settingsError, view.settingsError);

      const keyThereIs = provider?.key_set === true;
      finder.hidden = !keyThereIs;
      title.hidden = !keyThereIs;
      const query = view.text.trim();
      title.textContent = query === "" ? "Tendencias" : `Resultados para «${query}»`;
      showMessage(useError, view.useError);

      const block = stateBlock(view);
      state.replaceChildren(...(block === null ? [] : [block]));
      if (view.provider.kind === "error") {
        state.replaceChildren(create("p", "state-empty state-error", view.provider.message), actionButton("retry", "Reintentar", "next"));
      }

      const activeName = provider?.provider ?? null;
      const received = view.search.kind === "ready" ? view.search.attribution : "";
      const attributionText = keyThereIs ? visibleAttribution(activeName, received) : "";
      showMessage(attribution, attributionText === "" ? null : attributionText);
      paintedAttribution = attributionText;

      const results = view.search.kind === "ready" ? view.search.results : [];
      const usingBusy = view.using !== null;
      if (results !== paintedResults) {
        paintedResults = results;
        const focused = document.activeElement?.closest<HTMLLIElement>("li[data-id]")?.dataset["id"];
        grid.replaceChildren(...results.map((result) => card(result, paintedAttribution)));
        if (focused !== undefined) grid.querySelector<HTMLButtonElement>(`li[data-id="${CSS.escape(focused)}"] button`)?.focus();
      }
      grid.setAttribute("aria-busy", String(usingBusy));
      for (const press of grid.querySelectorAll<HTMLButtonElement>("button")) press.disabled = usingBusy;

      zonePlus.replaceChildren();
      if (view.search.kind === "ready") {
        if (view.search.plusError !== null) zonePlus.append(create("p", "message-error", view.search.plusError));
        if (view.search.next !== null) {
          const plus = button("button-secondary", view.search.loadingPlus ? "Cargando…" : "Cargar más", null);
          plus.disabled = view.search.loadingPlus;
          plus.setAttribute("aria-busy", String(view.search.loadingPlus));
          zonePlus.append(plus);
        }
      }
      if (usingBusy) {
        zonePlus.prepend(create("p", "secondary", "Preparando el GIF para el editor…"));
        zonePlus.setAttribute("role", "status");
      } else zonePlus.removeAttribute("role");
    },
  };
}
