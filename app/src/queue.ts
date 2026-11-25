import type { EntryUi, UiError, EntryState, QueueEvent } from "./contract.ts";

export interface Queue {
  readonly entries: readonly EntryUi[];
  readonly removed: ReadonlySet<number>;
  readonly downloading: boolean;
}

export function emptyQueue(): Queue {
  return { entries: [], removed: new Set(), downloading: false };
}

function esFinal(state: EntryState): boolean {
  return state.kind === "ready" || state.kind === "error";
}

function changeAccepts(current: EntryState, fresh: EntryState): boolean {
  if (esFinal(current)) return esFinal(fresh);
  return !(current.kind === "downloading" && fresh.kind === "pending");
}

function replace(queue: Queue, entry: EntryUi): Queue {
  if (queue.removed.has(entry.id)) return queue;
  const index = queue.entries.findIndex((current) => current.id === entry.id);
  if (index === -1) return { ...queue, entries: [...queue.entries, entry] };
  const current = queue.entries[index];
  if (current === undefined || !changeAccepts(current.state, entry.state)) return queue;
  const entries = queue.entries.slice();
  entries[index] = entry;
  return { ...queue, entries };
}

export function addEntries(queue: Queue, fresh: readonly EntryUi[]): Queue {
  return fresh.reduce(replace, queue);
}

export function startDownloads(queue: Queue): Queue {
  return { ...queue, downloading: true };
}

export function applyEvent(queue: Queue, event: QueueEvent): Queue {
  switch (event.kind) {
    case "change":
      return replace(queue, event.entry);
    case "finished":
      return { ...queue, downloading: false };
  }
}

export function interruptDownloads(queue: Queue, error: UiError): Queue {
  const entries = queue.entries.map((entry): EntryUi =>
    entry.state.kind === "downloading" ? { ...entry, state: { kind: "error", error } } : entry,
  );
  return { ...queue, entries };
}

export function removeFromQueue(queue: Queue, id: number): Queue {
  const removed = new Set(queue.removed);
  removed.add(id);
  return { ...queue, entries: queue.entries.filter((entry) => entry.id !== id), removed };
}

export function downloadPercentage(state: EntryState): number | null {
  if (state.kind !== "downloading" || state.total === null || state.total <= 0) return null;
  return Math.min(100, Math.round((state.bytes / state.total) * 100));
}

export function sizeText(bytes: number): string {
  if (bytes < 1_000_000) return `${Math.ceil(bytes / 1000)} KB`;
  const mb = Math.ceil(bytes / 100_000) / 10;
  return `${String(mb).replace(".", ",")} MB`;
}

export function stateText(state: EntryState): string {
  switch (state.kind) {
    case "pending":
      return "Pendiente";
    case "downloading": {
      const percentage = downloadPercentage(state);
      return percentage === null ? `Descargando ${sizeText(state.bytes)}` : `Descargando ${percentage} %`;
    }
    case "ready":
      return "Listo";
    case "error":
      return state.error.kind === "cancelled" ? "Cancelado" : "Error";
  }
}
