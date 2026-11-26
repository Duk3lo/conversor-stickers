import { type ExtensionUi, asErrorUi, installExtension as pickExtension } from "./contract.ts";
import { type BotPanelDependencies, createBotPanel } from "./bot-panel.ts";
import { BOT_EXTENSION_ID, botEnabled, numberHeld } from "./bot-status.ts";
import { create, element, pressedButton, showMessage } from "./dom.ts";
import {
  type InstalledExtension,
  buildNumber,
  changeExtension,
  countryOptions,
  defaultCountry,
  installExtension,
  loadExtensions,
  removeExtension,
  saveExtensions,
} from "./extensions.ts";
import { errorMessage } from "./messages.ts";
import { localStore } from "./store.ts";

export function createExtensionsSection(bot: BotPanelDependencies): void {
  const store = localStore();
  const installButton = element<HTMLButtonElement>("extension-install");
  const errorLine = element<HTMLParagraphElement>("extension-error");
  const empty = element<HTMLParagraphElement>("extension-empty");
  const listBox = element<HTMLDivElement>("extension-list");
  const countries = countryOptions();
  let list = loadExtensions(store);
  const botPanel = createBotPanel(bot);
  /** Locks the bot's number while its connection is enabled; set by the card that has the panel. */
  let holdBotNumber: () => void = () => undefined;

  function set(next: InstalledExtension[]): void {
    list = next;
    saveExtensions(store, list);
  }

  function card(installed: InstalledExtension): HTMLElement {
    const { extension, accepted, number } = installed;
    const box = create("div", "extension-card");
    box.dataset["extension"] = extension.id;
    const title = create("h4", "subtitle", extension.name);
    const description = create("p", "secondary small", extension.description);
    const link = create("a", "", extension.url);
    link.href = extension.url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    const notice = create("p", "secondary small", extension.notice);

    const check = create("label", "check-row");
    const accept = create("input", "");
    accept.type = "checkbox";
    accept.checked = accepted;
    accept.dataset["accept"] = extension.id;
    check.append(accept, " He leído y acepto");

    const remove = create("button", "button-text", "Quitar");
    remove.type = "button";
    remove.dataset["remove"] = extension.id;
    remove.setAttribute("aria-label", `Quitar ${extension.name}`);
    const { form, hold } = numberForm(extension, number);
    box.append(title, description, link, notice, check, form);
    if (extension.id === BOT_EXTENSION_ID) {
      botPanel.hidden = !accepted;
      holdBotNumber = () => {
        const current = list.find((candidate) => candidate.extension.id === BOT_EXTENSION_ID);
        hold(current !== undefined && numberHeld(current, bot.monitor.state()));
      };
      holdBotNumber();
      box.append(botPanel);
    }
    box.append(remove);
    return box;
  }

  /** A saved number is shown locked; it is only stored with «Guardar número». */
  function numberForm({ id, name }: ExtensionUi, saved: string | null): { form: HTMLFormElement; hold(held: boolean): void } {
    const form = create("form", "extension-number");
    form.noValidate = true;
    const inputId = `extension-number-${id}`;
    const previewId = `${inputId}-preview`;
    const errorId = `${inputId}-error`;

    const countryLabel = create("label", "", "País (si escribes el número sin “+”)");
    countryLabel.htmlFor = `${inputId}-country`;
    const countrySelect = create("select", "");
    countrySelect.id = `${inputId}-country`;
    countrySelect.append(new Option("Elige un país", ""), ...countries.map(({ code, label }) => new Option(label, code)));
    countrySelect.value = defaultCountry(navigator.language) ?? "";

    const label = create("label", "", "Número del chat de destino");
    label.htmlFor = inputId;
    const input = create("input", "");
    input.type = "text";
    input.id = inputId;
    input.inputMode = "tel";
    input.autocomplete = "off";
    input.placeholder = "612 345 678 o +34 612 345 678";
    input.setAttribute("aria-describedby", `${previewId} ${errorId}`);

    const preview = create("p", "small");
    preview.id = previewId;
    preview.setAttribute("role", "status");
    const numberError = create("p", "message-error");
    numberError.id = errorId;
    numberError.setAttribute("role", "alert");

    const save = create("button", "button-secondary", "Guardar número");
    save.type = "submit";
    save.setAttribute("aria-label", `Guardar número de ${name}`);
    const change = create("button", "button-secondary", "Cambiar");
    change.type = "button";
    change.setAttribute("aria-label", `Cambiar número de ${name}`);
    const heldNote = create("p", "secondary small", "Para cambiar el número, primero pulsa «Deshabilitar» en «Mi bot».");
    heldNote.id = `${inputId}-held`;
    heldNote.hidden = true;

    const typed = () => buildNumber(input.value, countries.find(({ code }) => code === countrySelect.value)?.code ?? null);

    function showError(message: string | null): void {
      showMessage(numberError, message);
      input.setAttribute("aria-invalid", String(message !== null));
    }

    function showPreview(): void {
      const result = typed();
      if (result.kind === "number") preview.textContent = `Número final: +${result.digits}`;
      else if (result.kind === "none" && saved !== null) preview.textContent = "Al guardar se quitará el número.";
      else preview.textContent = "";
    }

    function setLocked(locked: boolean): void {
      input.readOnly = locked;
      for (const node of [countryLabel, countrySelect, preview, save]) node.hidden = locked;
      change.hidden = !locked;
      if (locked) input.value = `+${saved}`;
      showError(null);
    }

    input.addEventListener("input", () => {
      showError(null);
      showPreview();
    });
    // The error waits until the user leaves the field: a half-typed number is not a mistake yet.
    input.addEventListener("change", () => {
      const result = typed();
      showError(result.kind === "invalid" ? result.message : null);
    });
    countrySelect.addEventListener("change", () => {
      showError(null);
      showPreview();
    });
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      if (input.readOnly) return;
      const result = typed();
      if (result.kind === "invalid") {
        showError(result.message);
        input.focus();
        return;
      }
      saved = result.kind === "number" ? result.digits : null;
      set(changeExtension(list, id, { number: saved }));
      showPreview();
      setLocked(saved !== null);
      if (id === BOT_EXTENSION_ID) holdBotNumber();
      if (saved !== null && !change.disabled) change.focus();
    });
    change.addEventListener("click", () => {
      setLocked(false);
      showPreview();
      input.focus();
    });

    form.append(countryLabel, countrySelect, label, input, preview, numberError, save, change, heldNote);
    setLocked(saved !== null);

    function hold(held: boolean): void {
      // Without a saved number there is nothing to protect: it can still be typed and saved.
      const locked = held && saved !== null;
      if (locked && !input.readOnly) setLocked(true);
      change.disabled = locked;
      heldNote.hidden = !locked;
      if (locked) change.setAttribute("aria-describedby", heldNote.id);
      else change.removeAttribute("aria-describedby");
    }

    return { form, hold };
  }

  function render(): void {
    holdBotNumber = () => undefined;
    listBox.replaceChildren(...list.map(card));
    empty.hidden = list.length > 0;
  }

  /** Without its accepted panel there would be no way to disable the connection. */
  function disableBot(id: string): void {
    if (id !== BOT_EXTENSION_ID || !botEnabled(bot.monitor.state())) return;
    bot.monitor.disable().catch((failure: unknown) => showMessage(errorLine, errorMessage(asErrorUi(failure))));
  }

  installButton.addEventListener("click", () => {
    installButton.disabled = true;
    showMessage(errorLine, null);
    pickExtension()
      .then((extension: ExtensionUi | null) => {
        if (extension === null) return;
        set(installExtension(list, extension));
        render();
        // Installing again asks to accept the notice anew, so the connection stops with it.
        disableBot(extension.id);
      })
      .catch((failure: unknown) => showMessage(errorLine, errorMessage(asErrorUi(failure))))
      .finally(() => {
        installButton.disabled = false;
      });
  });

  listBox.addEventListener("change", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    const acceptId = target.dataset["accept"];
    if (acceptId === undefined) return;
    set(changeExtension(list, acceptId, { accepted: target.checked }));
    if (acceptId === BOT_EXTENSION_ID) {
      botPanel.hidden = !target.checked;
      holdBotNumber();
      if (!target.checked) disableBot(acceptId);
    }
  });

  listBox.addEventListener("click", (event) => {
    const id = pressedButton(event)?.dataset["remove"];
    // The buttons of the bot panel are inside the list too, without `data-remove`.
    if (id === undefined) return;
    set(removeExtension(list, id));
    render();
    disableBot(id);
  });

  bot.monitor.subscribe(() => holdBotNumber());
  render();
}
