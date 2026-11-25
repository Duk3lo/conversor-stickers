import { Channel } from "@tauri-apps/api/core";

import {
  addEntries,
  applyEvent,
  emptyQueue,
  startDownloads,
  interruptDownloads,
  removeFromQueue,
} from "./queue.ts";
import type { Context } from "./context.ts";
import * as commands from "./contract.ts";
import * as mobileCommands from "./contract-mobile.ts";
import { asErrorUi, type EntryUi, type QueueEvent, type SaveVideoEvent } from "./contract.ts";
import { createDownloads } from "./downloads.ts";
import { errorMessage } from "./messages.ts";
import { controls } from "./mobile.ts";
import { openEditor, changeSection, withoutEntry } from "./navigation.ts";
import {
  classifyPasted,
  destinationOfPasted,
  linksOfText,
  PASTED_LIMIT_BYTES,
  pastedMessage,
  pasteableKind,
} from "./paste.ts";
import { platform } from "./platform.ts";
import { savedNotice, startSaving } from "./saving.ts";
import { createDownloadView } from "./download-view.ts";
import type { DownloadView } from "./views.ts";

const V_CTRL_NOTICE = "El botón Pegar no puede leer el portapapeles aquí: pulsa Ctrl+V.";

export function createDownloadSection(context: Context, busyEditor: () => boolean): void {
  let view: DownloadView = {
    queue: emptyQueue(),
    sendingLinks: false,
    linksError: null,
    queueError: null,
    pasted: { kind: "inactive" },
    saving: null,
  };

  function set(changes: Partial<DownloadView>): void {
    view = { ...view, ...changes };
    screen.render(view);
  }

  const downloads = createDownloads({
    download(emit) {
      const channel = new Channel<QueueEvent>();
      channel.onmessage = emit;
      return commands.downloadQueue(channel);
    },
    onStart: () => set({ queue: startDownloads(view.queue), queueError: null }),
    onEvent(event) {
      if (event.kind === "change") context.remember(event.entry);
      set({ queue: applyEvent(view.queue, event) });
    },
    onFail: (error) => set({ queue: interruptDownloads(view.queue, error), queueError: errorMessage(error) }),
  });

  async function saveVideo(id: number): Promise<void> {
    if (view.saving !== null) return;
    set({ saving: startSaving(id), queueError: null });
    const channel = new Channel<SaveVideoEvent>();
    channel.onmessage = (event) => {
      if (view.saving !== null && view.saving.id === id) set({ saving: { ...view.saving, event } });
    };
    try {
      const name = await commands.saveVideo(id, channel);
      if (name !== null) context.notify(savedNotice(platform, name), "videos");
      set({ saving: null });
    } catch (failure) {
      set({ saving: null, queueError: errorMessage(asErrorUi(failure)) });
    }
  }

  function addToList(entries: readonly EntryUi[]): void {
    entries.forEach((entry) => context.remember(entry));
    set({ queue: addEntries(view.queue, entries) });
  }

  async function addLinks(text: string): Promise<boolean> {
    set({ sendingLinks: true, linksError: null });
    try {
      addToList(await commands.addLinks(text));
      set({ sendingLinks: false });
      downloads.start();
      return true;
    } catch (failure) {
      set({ sendingLinks: false, linksError: errorMessage(asErrorUi(failure)) });
      return false;
    }
  }

  function notifyPasted(text: string, error = false): void {
    const nav = context.nav();
    if (nav.path.kind === "create" && nav.section === "download" && nav.editor === null) {
      set({ pasted: { kind: error ? "error" : "notice", text } });
    } else {
      context.notify(text);
    }
  }

  function carryLinks(text: string): void {
    context.navigate(changeSection(context.nav(), "download"));
    screen.setLinks(text);
  }

  async function pasteEntry(read: () => Promise<EntryUi>): Promise<void> {
    if (busyEditor()) return notifyPasted("Termina o cancela lo que se está creando antes de pegar otra cosa.");
    set({ pasted: { kind: "pasting" } });
    try {
      const entry = await read();
      set({ pasted: { kind: "inactive" } });
      addToList([entry]);
      const destination = destinationOfPasted(context.nav().section, entry.media);
      if (destination.kind === "editor") context.navigate(openEditor(context.nav(), entry.id, destination.destination));
      else if (destination.notice !== null) context.notify(destination.notice);
    } catch (failure) {
      notifyPasted(errorMessage(asErrorUi(failure)), true);
    }
  }

  function pasteBytes(bytes: Blob, mime: string): Promise<void> {
    return pasteEntry(async () => commands.paste(new Uint8Array(await bytes.arrayBuffer()), mime));
  }

  async function pasteWithButton(): Promise<void> {
    if (platform === "android") return pasteEntry(mobileCommands.pasteFromClipboard);
    if (typeof navigator.clipboard?.read !== "function") return notifyPasted(V_CTRL_NOTICE);
    set({ pasted: { kind: "pasting" } });
    let elements: ClipboardItems;
    try {
      elements = await navigator.clipboard.read();
    } catch {
      return notifyPasted(V_CTRL_NOTICE);
    }
    try {
      for (const element of elements) {
        const mime = pasteableKind(element.types);
        if (mime === null) continue;
        const blob = await element.getType(mime);
        if (blob.size > PASTED_LIMIT_BYTES) {
          return notifyPasted(pastedMessage({ kind: "too-much-large", bytes: blob.size }));
        }
        return await pasteBytes(blob, mime);
      }
      for (const element of elements) {
        if (!element.types.includes("text/plain")) continue;
        const links = linksOfText(await (await element.getType("text/plain")).text());
        if (links.length > 0 && controls(platform).links) {
          set({ pasted: { kind: "inactive" } });
          return carryLinks(links.join("\n"));
        }
      }
      notifyPasted("No hay nada que pegar: copia una imagen, un video o un enlace.");
    } catch (failure) {
      notifyPasted(errorMessage(asErrorUi(failure)), true);
    }
  }

  async function receiveShared(): Promise<void> {
    try {
      const entries = await mobileCommands.receiveShared();
      if (entries.length === 0) return;
      addToList(entries);
      context.navigate(changeSection(context.nav(), "download"));
      if (entries.some((entry) => entry.origin === "link")) downloads.start();
    } catch (failure) {
      notifyPasted(errorMessage(asErrorUi(failure)), true);
    }
  }

  if (platform === "android") {
    window.addEventListener("android-shared", () => void receiveShared());
    void receiveShared();
  }

  if (controls(platform).ctrlV) document.addEventListener("paste", pastedOfSystem);

  function pastedOfSystem(event: ClipboardEvent): void {
    const target = event.target;
    if (target instanceof HTMLTextAreaElement || target instanceof HTMLInputElement) return;
    if (event.clipboardData === null) return;
    const pasteable = classifyPasted(event.clipboardData);
    switch (pasteable.kind) {
      case "file":
        event.preventDefault();
        void pasteBytes(pasteable.file, pasteable.mime);
        return;
      case "links":
        if (!controls(platform).links) return;
        event.preventDefault();
        carryLinks(pasteable.text);
        return;
      case "nothing":
        return;
      default:
        event.preventDefault();
        notifyPasted(pastedMessage(pasteable));
    }
  }

  const screen = createDownloadView({
    addLinks,
    cancelDownloads() {
      downloads.cancel();
      commands.cancelDownloads().catch((failure: unknown) => {
        set({ queueError: errorMessage(asErrorUi(failure)) });
      });
    },
    resumeDownloads: () => downloads.start(),
    async remove(id) {
      try {
        await commands.removeEntry(id);
        context.forget(id);
        set({ queue: removeFromQueue(view.queue, id), queueError: null });
        context.navigate(withoutEntry(context.nav(), id));
      } catch (failure) {
        set({ queueError: errorMessage(asErrorUi(failure)) });
      }
    },
    makeSticker: (id) => context.navigate(openEditor(context.nav(), id, "sticker")),
    makeGif: (id) => context.navigate(openEditor(context.nav(), id, "gif")),
    saveVideo: (id) => void saveVideo(id),
    openFolder: (folder) => context.openFolder(folder),
    cancelSaving() {
      commands.cancelDownloads().catch((failure: unknown) => {
        set({ queueError: errorMessage(asErrorUi(failure)) });
      });
    },
    paste: () => void pasteWithButton(),
  });
  screen.render(view);
}

