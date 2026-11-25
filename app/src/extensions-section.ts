import { type ExtensionUi, asErrorUi, installExtension as pickExtension } from "./contract.ts";
import { create, element, pressedButton, showMessage } from "./dom.ts";
import {
  type InstalledExtension,
  changeExtension,
  installExtension,
  loadExtensions,
  parseNumber,
  removeExtension,
  saveExtensions,
} from "./extensions.ts";
import { errorMessage } from "./messages.ts";
import { localStore } from "./store.ts";

export function createExtensionsSection(): void {
  const store = localStore();
  const installButton = element<HTMLButtonElement>("extension-install");
  const errorLine = element<HTMLParagraphElement>("extension-error");
  const empty = element<HTMLParagraphElement>("extension-empty");
  const listBox = element<HTMLDivElement>("extension-list");
  let list = loadExtensions(store);

  function set(next: InstalledExtension[]): void {
    list = next;
    saveExtensions(store, list);
  }

  function card({ extension, accepted, number }: InstalledExtension): HTMLElement {
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

    const numberId = `extension-number-${extension.id}`;
    const errorId = `${numberId}-error`;
    const label = create("label", "", "Número del chat de destino");
    label.htmlFor = numberId;
    const input = create("input", "");
    input.type = "text";
    input.id = numberId;
    input.inputMode = "numeric";
    input.autocomplete = "off";
    input.placeholder = "34612345678";
    input.value = number ?? "";
    input.setAttribute("aria-describedby", errorId);
    input.dataset["number"] = extension.id;
    const numberError = create("p", "message-error");
    numberError.id = errorId;
    numberError.setAttribute("role", "alert");
    numberError.hidden = true;

    const remove = create("button", "button-text", "Quitar");
    remove.type = "button";
    remove.dataset["remove"] = extension.id;
    remove.setAttribute("aria-label", `Quitar ${extension.name}`);
    box.append(title, description, link, notice, check, label, input, numberError, remove);
    return box;
  }

  function render(): void {
    listBox.replaceChildren(...list.map(card));
    empty.hidden = list.length > 0;
  }

  installButton.addEventListener("click", () => {
    installButton.disabled = true;
    showMessage(errorLine, null);
    pickExtension()
      .then((extension: ExtensionUi | null) => {
        if (extension === null) return;
        set(installExtension(list, extension));
        render();
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
    if (acceptId !== undefined) {
      set(changeExtension(list, acceptId, { accepted: target.checked }));
      return;
    }
    const numberId = target.dataset["number"];
    if (numberId === undefined) return;
    const parsed = parseNumber(target.value);
    const message = target.parentElement?.querySelector<HTMLElement>(`#${target.id}-error`) ?? null;
    if (message !== null) showMessage(message, parsed.kind === "invalid" ? parsed.message : null);
    target.setAttribute("aria-invalid", String(parsed.kind === "invalid"));
    if (parsed.kind !== "invalid") set(changeExtension(list, numberId, { number: parsed.kind === "number" ? parsed.digits : null }));
  });

  listBox.addEventListener("click", (event) => {
    const id = pressedButton(event)?.dataset["remove"];
    if (id === undefined) return;
    set(removeExtension(list, id));
    render();
  });

  render();
}
