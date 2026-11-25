import assert from "node:assert/strict";
import { test } from "node:test";

import type { EntryUi, EntryState } from "./contract.ts";
import {
  addEntries,
  applyEvent,
  emptyQueue,
  startDownloads,
  interruptDownloads,
  downloadPercentage,
  removeFromQueue,
  stateText,
  sizeText,
} from "./queue.ts";

function entry(id: number, state: EntryState): EntryUi {
  return { id, name: `video-${id}`, origin: "link", state, media: null };
}

const pending: EntryState = { kind: "pending" };

test("adding entries puts them at the end in order", () => {
  const queue = addEntries(emptyQueue(), [entry(1, pending), entry(2, pending)]);
  assert.deepEqual(
    queue.entries.map((e) => e.id),
    [1, 2],
  );
});

test("a change replaces the entry with the same id", () => {
  let queue = addEntries(emptyQueue(), [entry(1, pending), entry(2, pending)]);
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(2, { kind: "downloading", bytes: 10, total: 100 }),
  });
  assert.deepEqual(queue.entries[1]?.state, { kind: "downloading", bytes: 10, total: 100 });
  assert.deepEqual(queue.entries[0]?.state, pending);
});

test("the events of a removed entry are ignored", () => {
  let queue = addEntries(emptyQueue(), [entry(1, pending)]);
  queue = removeFromQueue(queue, 1);
  queue = applyEvent(queue, { kind: "change", entry: entry(1, { kind: "ready" }) });
  assert.deepEqual(queue.entries, []);
});

test("a change of an unknown, non-removed entry is added", () => {
  const queue = applyEvent(emptyQueue(), { kind: "change", entry: entry(7, pending) });
  assert.deepEqual(
    queue.entries.map((e) => e.id),
    [7],
  );
});

test("a progress that arrives late does not undo a final state", () => {
  let queue = addEntries(emptyQueue(), [entry(1, pending)]);
  queue = applyEvent(queue, { kind: "change", entry: entry(1, { kind: "ready" }) });
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(1, { kind: "downloading", bytes: 50, total: 100 }),
  });
  assert.deepEqual(queue.entries[0]?.state, { kind: "ready" });
});

test("the second track without total starts over and does not freeze", () => {
  let queue = addEntries(emptyQueue(), [entry(1, pending)]);
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(1, { kind: "downloading", bytes: 900, total: null }),
  });
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(1, { kind: "downloading", bytes: 20, total: null }),
  });
  assert.deepEqual(queue.entries[0]?.state, { kind: "downloading", bytes: 20, total: null });
});

test("finished marks that it is no longer downloading", () => {
  let queue = startDownloads(addEntries(emptyQueue(), [entry(1, pending)]));
  assert.equal(queue.downloading, true);
  queue = applyEvent(queue, { kind: "finished" });
  assert.equal(queue.downloading, false);
});

test("the functions do not modify the queue they receive", () => {
  const original = addEntries(emptyQueue(), [entry(1, pending)]);
  removeFromQueue(original, 1);
  applyEvent(original, { kind: "change", entry: entry(1, { kind: "ready" }) });
  assert.deepEqual(original.entries[0]?.state, pending);
});

test("download percentage only with the known total", () => {
  assert.equal(downloadPercentage({ kind: "downloading", bytes: 25, total: 100 }), 25);
  assert.equal(downloadPercentage({ kind: "downloading", bytes: 150, total: 100 }), 100);
  assert.equal(downloadPercentage({ kind: "downloading", bytes: 25, total: null }), null);
  assert.equal(downloadPercentage({ kind: "downloading", bytes: 0, total: 0 }), null);
  assert.equal(downloadPercentage({ kind: "ready" }), null);
});

test("text of each row state", () => {
  assert.equal(stateText(pending), "Pendiente");
  assert.equal(stateText({ kind: "downloading", bytes: 42, total: 100 }), "Descargando 42 %");
  assert.equal(stateText({ kind: "downloading", bytes: 2_500_000, total: null }), "Descargando 2,5 MB");
  assert.equal(stateText({ kind: "ready" }), "Listo");
  assert.equal(
    stateText({ kind: "error", error: { kind: "cancelled", message: "descarga cancelada" } }),
    "Cancelado",
  );
  assert.equal(
    stateText({ kind: "error", error: { kind: "failed_download", message: "x" } }),
    "Error",
  );
});

test("sizes in KB or MB base 10, rounded up as in the core", () => {
  assert.equal(sizeText(0), "0 KB");
  assert.equal(sizeText(500_000), "500 KB");
  assert.equal(sizeText(612_001), "613 KB");
  assert.equal(sizeText(10_000_000), "10 MB");
  assert.equal(sizeText(1_520_000), "1,6 MB");
});

test("the second track of a download (audio) starts over from zero", () => {
  let queue = addEntries(emptyQueue(), [entry(1, pending)]);
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(1, { kind: "downloading", bytes: 900, total: 900 }),
  });
  queue = applyEvent(queue, {
    kind: "change",
    entry: entry(1, { kind: "downloading", bytes: 20, total: 100 }),
  });
  assert.deepEqual(queue.entries[0]?.state, { kind: "downloading", bytes: 20, total: 100 });
});

test("if the queue is interrupted, what was downloading becomes an error", () => {
  const error = { kind: "other", message: "la tarea se interrumpió" } as const;
  let queue = addEntries(emptyQueue(), [
    entry(1, { kind: "downloading", bytes: 5, total: null }),
    entry(2, pending),
  ]);
  queue = interruptDownloads(queue, error);
  assert.deepEqual(
    queue.entries.map((e) => e.state),
    [{ kind: "error", error }, pending],
  );
});

