import { downloadPercentage, stateText } from "./queue.ts";
import type { EntryUi } from "./contract.ts";
import { create, element, icon, showMessage } from "./dom.ts";
import { pendingThereIs, errorMessage } from "./messages.ts";
import { savingPercentage, savingText } from "./saving.ts";
import { PASTE_HELP, PASTE_HELP_PLAY, EMPTY_QUEUE, EMPTY_QUEUE_PLAY, controls } from "./mobile.ts";
import { play } from "./edition.ts";
import { platform } from "./platform.ts";
import type { Saving } from "./saving.ts";
import { canOpenFolder } from "./saved-folder.ts";
import type { DownloadActions, DownloadView } from "./views.ts";

export function createDownloadView(actions: DownloadActions): {
  render(view: DownloadView): void;
  setLinks(text: string): void;
} {
  const pasteButton = element<HTMLButtonElement>("paste");
  const pasteText = element<HTMLSpanElement>("paste-text");
  const pasteNotice = element<HTMLParagraphElement>("paste-notice");
  const linksForm = element<HTMLFormElement>("form-links");
  const linksText = element<HTMLTextAreaElement>("text-links");
  const addButton = element<HTMLButtonElement>("add");
  const linksError = element<HTMLParagraphElement>("error-links");
  const downloadsCancelButton = element<HTMLButtonElement>("cancel-downloads");
  const downloadsResumeButton = element<HTMLButtonElement>("resume-downloads");
  const emptyQueue = element<HTMLParagraphElement>("queue-empty");
  const queueList = element<HTMLUListElement>("list-queue");
  const queueError = element<HTMLParagraphElement>("error-queue");
  const videosFolder = element<HTMLButtonElement>("open-videos-folder");
  videosFolder.hidden = !canOpenFolder(platform);
  videosFolder.addEventListener("click", () => actions.openFolder("videos"));

  element<HTMLParagraphElement>("paste-help").textContent = play ? PASTE_HELP_PLAY : PASTE_HELP[platform];
  emptyQueue.textContent = play ? EMPTY_QUEUE_PLAY : EMPTY_QUEUE[platform];
  linksForm.hidden = !controls(platform, play).links;

  pasteButton.addEventListener("click", () => actions.paste());
  linksForm.addEventListener("submit", (event) => {
    event.preventDefault();
    void actions.addLinks(linksText.value).then((added) => {
      if (added) linksText.value = "";
    });
  });
  linksText.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      linksForm.requestSubmit();
    }
  });
  downloadsCancelButton.addEventListener("click", () => actions.cancelDownloads());
  downloadsResumeButton.addEventListener("click", () => actions.resumeDownloads());

  queueList.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest("button");
    const row = button?.closest<HTMLLIElement>("li[data-id]");
    if (!button || !row || button.disabled) return;
    const id = Number(row.dataset.id);
    if (button.classList.contains("row-remove")) actions.remove(id);
    else if (button.dataset.action === "sticker") actions.makeSticker(id);
    else if (button.dataset.action === "gif") actions.makeGif(id);
    else if (button.dataset.action === "save") actions.saveVideo(id);
    else if (button.dataset.action === "cancel-save") actions.cancelSaving();
  });

  function button(text: string, action: string, iconName: string): HTMLButtonElement {
    const node = create("button", "button-secondary button-small");
    node.type = "button";
    node.dataset.action = action;
    node.append(icon(iconName), document.createTextNode(text));
    return node;
  }

  function createRow(): HTMLLIElement {
    const row = create("li", "row");
    const header = create("div", "row-header");
    const data = create("div", "row-data");
    data.append(create("span", "row-name"), create("span", "row-state"));
    const remove = create("button", "row-remove");
    remove.type = "button";
    remove.append(icon("close"));
    header.append(data, remove);
    const rowActions = create("div", "row-actions");
    rowActions.append(
      button("Hacer sticker", "sticker", "sticker"),
      button("Hacer GIF", "gif", "gif"),
      button("Guardar vídeo", "save", "save"),
      button("Cancelar", "cancel-save", "close"),
    );
    const progress = create("div", "row-progress");
    progress.setAttribute("aria-hidden", "true");
    row.append(header, rowActions, progress);
    return row;
  }

  function updateRow(row: HTMLLIElement, entry: EntryUi, saving: Saving | null): void {
    row.dataset.id = String(entry.id);
    row.dataset.state = entry.state.kind;
    const name = row.querySelector<HTMLSpanElement>(".row-name");
    const state = row.querySelector<HTMLSpanElement>(".row-state");
    const remove = row.querySelector<HTMLButtonElement>(".row-remove");
    const rowActions = row.querySelector<HTMLDivElement>(".row-actions");
    const makeGif = row.querySelector<HTMLButtonElement>('[data-action="gif"]');
    const makeSticker = row.querySelector<HTMLButtonElement>('[data-action="sticker"]');
    const saveVideo = row.querySelector<HTMLButtonElement>('[data-action="save"]');
    const cancelSaving = row.querySelector<HTMLButtonElement>('[data-action="cancel-save"]');
    const progress = row.querySelector<HTMLDivElement>(".row-progress");
    if (!name || !state || !remove || !rowActions || !makeGif || !makeSticker || !progress) return;
    if (!saveVideo || !cancelSaving) return;
    const savingThis = saving !== null && saving.id === entry.id;

    name.textContent = entry.name;
    name.title = entry.name;
    const media = entry.media;
    const textOfState =
      entry.state.kind === "error"
        ? `${stateText(entry.state)}: ${errorMessage(entry.state.error)}`
        : media !== null
          ? `${media.kind === "image" ? "Imagen" : "Video"} ${media.width} × ${media.height}`
          : stateText(entry.state);
    state.replaceChildren(
      ...(entry.state.kind === "error" ? [icon("notice")] : []),
      document.createTextNode(savingThis ? `${textOfState} · ${savingText(saving)}` : textOfState),
    );
    remove.disabled = entry.state.kind === "downloading";
    remove.setAttribute("aria-label", `Quitar ${entry.name} de la lista`);

    rowActions.hidden = media === null || entry.state.kind !== "ready";
    makeGif.hidden = media?.kind !== "video";
    saveVideo.hidden = !controls(platform, play).saveVideo || media?.kind !== "video" || entry.origin !== "link" || savingThis;
    saveVideo.disabled = saving !== null;
    cancelSaving.hidden = !savingThis;
    saveVideo.setAttribute("aria-label", `Guardar el vídeo de ${entry.name} con sonido`);
    cancelSaving.setAttribute("aria-label", `Cancelar el guardado de ${entry.name}`);
    makeSticker.setAttribute("aria-label", `Hacer un sticker con ${entry.name}`);
    makeGif.setAttribute("aria-label", `Hacer un GIF con ${entry.name}`);

    const percentage = savingThis ? savingPercentage(saving) : downloadPercentage(entry.state);
    progress.hidden = percentage === null;
    progress.style.width = `${percentage ?? 0}%`;
  }

  function renderQueue(view: DownloadView): void {
    const existing = new Map<number, HTMLLIElement>();
    for (const row of Array.from(queueList.children) as HTMLLIElement[]) existing.set(Number(row.dataset.id), row);
    view.queue.entries.forEach((entry, index) => {
      const row = existing.get(entry.id) ?? createRow();
      existing.delete(entry.id);
      updateRow(row, entry, view.saving);
      if (queueList.children[index] !== row) queueList.insertBefore(row, queueList.children[index] ?? null);
    });
    for (const extra of existing.values()) extra.remove();
  }

  return {
    render(view) {
      const p = view.pasted;
      pasteButton.disabled = p.kind === "pasting";
      pasteButton.setAttribute("aria-busy", String(p.kind === "pasting"));
      pasteText.textContent = p.kind === "pasting" ? "Pegando…" : "Pegar";
      pasteNotice.classList.toggle("message-error", p.kind === "error");
      pasteNotice.classList.toggle("message-notice", p.kind === "notice");
      showMessage(pasteNotice, p.kind === "notice" || p.kind === "error" ? p.text : null);

      addButton.disabled = view.sendingLinks;
      linksText.setAttribute("aria-invalid", String(view.linksError !== null));
      showMessage(linksError, view.linksError);
      downloadsCancelButton.hidden = !view.queue.downloading;
      downloadsResumeButton.hidden = view.queue.downloading || !pendingThereIs(view.queue.entries);
      showMessage(queueError, view.queueError);
      emptyQueue.hidden = view.queue.entries.length > 0;
      renderQueue(view);
    },
    setLinks(text) {
      const current = linksText.value.trimEnd();
      linksText.value = current ? `${current}\n${text.trim()}` : text.trim();
      linksText.focus();
    },
  };
}
