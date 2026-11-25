import assert from "node:assert/strict";
import { test } from "node:test";

import { PREVIEW_WINDOW_S as Window } from "./contract.ts";
import {
  createPreview,
  fromForClipRange,
  type PreviewState,
  secondInPreview,
} from "./preview.ts";

test("a video that fits in the window is previewed from the start", () => {
  for (const duration of [0.05, 6, Window]) {
    assert.equal(fromForClipRange({ start: 0, end: Math.min(duration, 6) }, duration, null), 0);
  }
});

test("the clip range always stays inside the window, anywhere in the video", () => {
  const duration = 7199.7;
  for (const long of [0.1, 6, 20, 60, 90]) {
    for (const start of [0, 3.2, 59.9, 119.5, 1000, 3601.3, duration - long]) {
      const from = fromForClipRange({ start, end: start + long }, duration, null);
      const scenario = `${long} s desde ${start}: ventana desde ${from}`;
      assert.ok(from >= 0 && from <= duration - Window + 1e-6, scenario);
      assert.ok(from <= start + 1e-6 && from + Window >= start + long - 1e-6, scenario);
    }
  }
});

test("the window does not change while the clip range stays inside", () => {
  assert.equal(fromForClipRange({ start: 60, end: 66 }, 600, 10), 10);
  assert.equal(fromForClipRange({ start: 10, end: 130 }, 600, 10), 10);
  assert.ok(fromForClipRange({ start: 128, end: 134 }, 600, 10) !== 10);
  assert.ok(fromForClipRange({ start: 5, end: 11 }, 600, 10) !== 10);
});

test("the video seconds are converted to preview seconds", () => {
  assert.equal(secondInPreview(65, 60), 5);
  assert.equal(secondInPreview(3, 60), 0);
});

function turn(): Promise<void> {
  return new Promise((follow) => setTimeout(follow, 0));
}

function bank() {
  const requested: string[] = [];
  const responses: { give(): void; fail(message: string): void }[] = [];
  const states: PreviewState[] = [];
  const freed: string[] = [];
  let cancellations = 0;
  const preview = createPreview({
    request(id, fromS) {
      const index = requested.push(`${id}@${fromS}`);
      return new Promise<string>((bien, bad) => {
        responses.push({
          give: () => bien(`blob:${index}`),
          fail: (message) => bad({ kind: "cancelled", message }),
        });
      });
    },
    cancel: async () => {
      cancellations += 1;
    },
    free: (url) => freed.push(url),
    wait: turn,
    onChange: (state) => states.push(state),
  });
  return { preview, requested, responses, states, freed, cancellations: () => cancellations };
}

test("requests the preview and shows it with its start", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 30 });
  assert.deepEqual(b.states.at(-1), { kind: "loading", fromS: 30 });
  await turn();
  await turn();
  b.responses[0]?.give();
  await turn();
  assert.deepEqual(b.requested, ["1@30"]);
  assert.deepEqual(b.states.at(-1), { kind: "ready", url: "blob:1", fromS: 30 });
});

test("asking for the same thing does not generate it again", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 0 });
  await turn();
  await turn();
  b.preview.show({ id: 1, fromS: 0 });
  assert.deepEqual(b.requested, ["1@0"]);
  assert.equal(b.cancellations(), 0);
});

test("changing cancels the running one and, when done, only the last is requested", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 0 });
  await turn();
  await turn();
  b.preview.show({ id: 1, fromS: 100 });
  b.preview.show({ id: 2, fromS: 0 });
  assert.ok(b.cancellations() > 0);
  assert.deepEqual(b.requested, ["1@0"]);
  b.responses[0]?.fail("cancelada");
  await turn();
  await turn();
  assert.deepEqual(b.requested, ["1@0", "2@0"]);
  assert.deepEqual(b.states.at(-1), { kind: "loading", fromS: 0 });
});

test("dragging only generates the window where it stops", async () => {
  const b = bank();
  for (const fromS of [10, 20, 30]) b.preview.show({ id: 1, fromS });
  await turn();
  await turn();
  assert.deepEqual(b.requested, ["1@30"]);
});

test("one that arrives late is not shown and is released", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 0 });
  await turn();
  await turn();
  b.preview.show(null);
  b.responses[0]?.give();
  await turn();
  assert.deepEqual(b.freed, ["blob:1"]);
  assert.deepEqual(b.states.at(-1), { kind: "empty" });
});

test("the one that stops being shown is released", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 0 });
  await turn();
  await turn();
  b.responses[0]?.give();
  await turn();
  b.preview.show({ id: 2, fromS: 0 });
  assert.deepEqual(b.freed, ["blob:1"]);
});

test("a failure is shown with its reason", async () => {
  const b = bank();
  b.preview.show({ id: 1, fromS: 0 });
  await turn();
  await turn();
  b.responses[0]?.fail("ffmpeg no pudo");
  await turn();
  assert.deepEqual(b.states.at(-1), { kind: "failed", fromS: 0, message: "ffmpeg no pudo" });
});
