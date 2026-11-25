import { toggleEmoji, searchEmojis, type EmojiEntry, Groups, MAX_EMOJIS_STICKER } from "./emojis.ts";
import { button, create, image } from "./dom.ts";
import type { FileUrl } from "./catalog-view.ts";

export interface EmojiSelectorOptions {
  readonly path: string | null;
  readonly initial: readonly string[];
  readonly recent: readonly string[];
  save(emojis: readonly string[]): Promise<string | null>;
  onClose(): void;
}

export interface SelectorEmoji {
  open(options: EmojiSelectorOptions): void;
  close(): boolean;
}

const Focusable = 'button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])';

export function createEmojiSelector(host: HTMLElement, fileUrl: FileUrl): SelectorEmoji {
  let options: EmojiSelectorOptions | null = null;
  let selection: readonly string[] = [];
  let query = "";
  let notice: string | null = null;
  let saving = false;
  let preview: Element | null = null;

  let bar: HTMLElement;
  let results: HTMLElement;
  let message: HTMLElement;
  let save: HTMLButtonElement;

  function emojiButton(entry: EmojiEntry): HTMLButtonElement {
    const node = button("emoji-button", entry.emoji, null, `${entry.emoji} ${entry.words.split(" ")[0] ?? ""}`.trim());
    node.dataset["emoji"] = entry.emoji;
    node.setAttribute("aria-pressed", String(selection.includes(entry.emoji)));
    node.addEventListener("click", () => toggle(entry.emoji));
    return node;
  }

  function grid(entries: readonly EmojiEntry[]): HTMLElement {
    const list = create("div", "emoji-grid");
    for (const entry of entries) list.append(emojiButton(entry));
    return list;
  }

  function renderResults(): void {
    const nodes: HTMLElement[] = [];
    if (query.trim() !== "") {
      const found = searchEmojis(query);
      if (found.length === 0) nodes.push(create("p", "secondary", `No hay emojis para «${query.trim()}».`));
      else nodes.push(grid(found));
    } else {
      const recent = options?.recent ?? [];
      if (recent.length > 0) {
        nodes.push(create("h3", "subtitle", "Recientes"), grid(recent.map((emoji) => ({ emoji, words: "" }))));
      }
      for (const group of Groups) nodes.push(create("h3", "subtitle", group.name), grid(group.emojis));
    }
    results.replaceChildren(...nodes);
  }

  function renderSelection(): void {
    const chips = selection.map((emoji) => {
      const remove = button("emoji-chosen", emoji, "close", `Quitar ${emoji}`);
      remove.addEventListener("click", () => toggle(emoji));
      return remove;
    });
    const count = create("span", "secondary small", `${selection.length} de ${MAX_EMOJIS_STICKER}`);
    bar.replaceChildren(...chips, count);
    message.textContent = notice ?? "";
    message.hidden = notice === null;
    for (const node of results.querySelectorAll<HTMLElement>("[data-emoji]")) {
      node.setAttribute("aria-pressed", String(selection.includes(node.dataset["emoji"] ?? "")));
    }
  }

  function toggle(emoji: string): void {
    const change = toggleEmoji(selection, emoji);
    selection = change.emojis;
    notice = change.notice;
    renderSelection();
  }

  async function accept(): Promise<void> {
    if (options === null || saving) return;
    saving = true;
    save.disabled = true;
    const error = await options.save(selection);
    saving = false;
    save.disabled = false;
    if (error === null) close();
    else {
      notice = error;
      renderSelection();
    }
  }

  function close(): boolean {
    if (options === null) return false;
    const { onClose } = options;
    options = null;
    host.hidden = true;
    host.replaceChildren();
    if (preview instanceof HTMLElement) preview.focus({ preventScroll: true });
    onClose();
    return true;
  }

  function open(fresh: EmojiSelectorOptions): void {
    options = fresh;
    selection = fresh.initial;
    query = "";
    notice = null;
    preview = document.activeElement;

    const dialog = create("div", "layer-sticker-card selector-emoji");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "selector-emoji-title");
    const header = create("div", "selector-emoji-header");
    if (fresh.path !== null) header.append(image(fileUrl(fresh.path), "selector-emoji-sticker"));
    const title = create("h2", "selector-emoji-title", "Emojis del sticker");
    title.id = "selector-emoji-title";
    header.append(title);
    bar = create("div", "emoji-selection");
    bar.setAttribute("aria-live", "polite");

    const label = create("label", "hidden-visual", "Buscar emoji por palabra");
    label.htmlFor = "selector-emoji-search";
    const field = document.createElement("input");
    field.type = "search";
    field.id = "selector-emoji-search";
    field.className = "finder-text";
    field.placeholder = "Buscar: gato, corazón, fiesta…";
    field.autocomplete = "off";
    field.addEventListener("input", () => {
      query = field.value;
      renderResults();
    });

    results = create("div", "emoji-results");
    message = create("p", "message-error");
    message.setAttribute("role", "alert");
    message.hidden = true;
    const footer = create("div", "actions-row actions-centered");
    save = button("button-main", "Guardar", "ok");
    save.addEventListener("click", () => void accept());
    const cancel = button("button-secondary", "Cancelar", null);
    cancel.addEventListener("click", () => void close());
    footer.append(save, cancel);

    dialog.append(header, bar, label, field, results, message, footer);
    dialog.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        close();
      } else if (event.key === "Tab") {
        const inside = [...dialog.querySelectorAll<HTMLElement>(Focusable)];
        const first = inside[0];
        const last = inside[inside.length - 1];
        if (first === undefined || last === undefined) return;
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    });
    host.replaceChildren(dialog);
    host.hidden = false;
    renderResults();
    renderSelection();
    field.focus({ preventScroll: true });
  }

  return { open, close };
}

