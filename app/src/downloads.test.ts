import assert from "node:assert/strict";
import { test } from "node:test";

import type { EntryUi, QueueEvent } from "./contract.ts";
import { createDownloads } from "./downloads.ts";

function turn(): Promise<void> {
  return new Promise((follow) => setTimeout(follow, 0));
}

const downloading: EntryUi = {
  id: 1,
  name: "a.example",
  origin: "link",
  state: { kind: "downloading", bytes: 5, total: null },
  media: null,
};

function bank() {
  const calls: { emit(event: QueueEvent): void; finish(): void; fail(): void }[] = [];
  const steps: string[] = [];
  const downloads = createDownloads({
    download: (emit) =>
      new Promise<void>((bien, bad) => {
        calls.push({
          emit,
          finish: () => bien(),
          fail: () => bad({ kind: "other", message: "se interrumpió" }),
        });
      }),
    onStart: () => steps.push("empieza"),
    onEvent: (event) => steps.push(event.kind),
    onFail: (failure) => steps.push(`falla: ${failure.message}`),
  });
  return { downloads, calls, steps };
}

test("download notifies that it starts, of the changes and that it ends", async () => {
  const b = bank();
  b.downloads.start();
  b.calls[0]?.emit({ kind: "change", entry: downloading });
  b.calls[0]?.emit({ kind: "finished" });
  b.calls[0]?.finish();
  await turn();
  assert.deepEqual(b.steps, ["empieza", "change", "finished"]);
  assert.equal(b.calls.length, 1);
});

test("what is added just as the queue finishes starts", async () => {
  const b = bank();
  b.downloads.start();
  b.downloads.start();
  assert.equal(b.calls.length, 1, "dos colas a la vez");
  b.calls[0]?.finish();
  await turn();
  assert.equal(b.calls.length, 2);
});

test("also if added after finishing and before the command returns", async () => {
  const b = bank();
  b.downloads.start();
  b.calls[0]?.emit({ kind: "finished" });
  b.downloads.start();
  b.calls[0]?.finish();
  await turn();
  assert.equal(b.calls.length, 2);
});

test("what arrives late from an old queue does not close the new one", async () => {
  const b = bank();
  b.downloads.start();
  b.downloads.start();
  b.calls[0]?.finish();
  await turn();
  b.calls[0]?.emit({ kind: "finished" });
  assert.equal(b.steps.includes("terminada"), false);
  b.calls[1]?.finish();
  await turn();
  assert.deepEqual(b.steps, ["empieza", "finished"]);
});

test("cancel does not undo what was added while downloading", async () => {
  const b = bank();
  b.downloads.start();
  b.downloads.start();
  b.downloads.cancel();
  b.calls[0]?.finish();
  await turn();
  assert.equal(b.calls.length, 1);
  assert.equal(b.steps.at(-1), "finished");
});

test("it can be resumed after cancelling", async () => {
  const b = bank();
  b.downloads.start();
  b.downloads.cancel();
  b.calls[0]?.finish();
  await turn();
  b.downloads.start();
  assert.equal(b.calls.length, 2);
});

test("a failure is notified and leaves the queue stopped", async () => {
  const b = bank();
  b.downloads.start();
  b.calls[0]?.fail();
  await turn();
  assert.deepEqual(b.steps, ["empieza", "falla: se interrumpió", "finished"]);
});
