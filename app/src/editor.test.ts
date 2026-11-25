import assert from "node:assert/strict";
import { test } from "node:test";

import type { EstimateUi, CreateRequest, PresetUi } from "./contract.ts";
import {
  ESTIMATE_WAITS_MS,
  createEstimator,
  reasonForCreateNo,
  requestOfEditor,
  fittingText,
  outputDurationText,
  estimateText,
  priorityText,
  Priorities,
  maxSecondsText,
  durationFix,
} from "./editor.ts";
import { DEFAULT_FRAMING } from "./framing.ts";
import type { EstimateState } from "./views.ts";

const estimate = (changes: Partial<EstimateUi> = {}): EstimateUi => ({
  estimated_size_bytes: 380_000,
  max_size_bytes: 500_000,
  fits: true,
  fps: 15,
  max_seconds_s: 6.5,
  detail_px: 512,
  max_range_s: 10,
  ...changes,
});

const sticker: PresetUi = {
  name: "whatsapp-sticker",
  title: "Sticker animado de WhatsApp",
  extension: "webp",
  side: 512,
  max_size_bytes: 500_000,
  max_duration_s: 10,
  max_fps: 30,
  side_range_px: null,
  fps_range: { min: 5, max: 30 },
};

test("each priority has a short title and an explanation", () => {
  assert.equal(priorityText("sharpness").title, "Más calidad");
  assert.equal(priorityText("balance").title, "Equilibrado");
  assert.equal(priorityText("smoothness").title, "Más duración");
  assert.equal(priorityText("fit_anyway").title, "Que quepa");
  assert.equal(priorityText("fluid").title, "Más fluido");
  const texts = Priorities.map((priority) => priorityText(priority).explanation);
  for (const text of texts) assert.ok(text.length > 10);
  assert.equal(new Set(texts).size, Priorities.length);
});

test("the estimate gives the size, the limit and whether it fits", () => {
  assert.equal(estimateText(estimate()), "≈ 380 KB de 500 KB · cabe · 15 fps");
  assert.equal(
    estimateText(estimate({ estimated_size_bytes: 612_400, fits: false })),
    "≈ 613 KB de 500 KB · no cabe · 15 fps",
  );
  assert.equal(
    estimateText(estimate({ estimated_size_bytes: 4_210_000, max_size_bytes: 10_000_000 })),
    "≈ 4,3 MB de 10 MB · cabe · 15 fps",
  );
});

test("the seconds that fit are stated as approximate", () => {
  assert.equal(maxSecondsText(estimate()), "Con esta calidad caben unos 6,5 s del video");
  assert.equal(maxSecondsText(estimate({ max_seconds_s: 10 })), "Con esta calidad caben unos 10 s del video");
  assert.equal(
    maxSecondsText(estimate({ max_seconds_s: 0.04 })),
    "Con esta calidad no cabe ni una décima del video: prueba con «Más duración»",
  );
});

test("the output duration is shown next to the clip range", () => {
  assert.equal(outputDurationText(4, 0.5), "4,0 s del video a 0,5x · dura 8,0 s");
  assert.equal(outputDurationText(6, 1), "6,0 s del video a 1x · dura 6,0 s");
});

test("the size fitting notice says how much to trim", () => {
  assert.equal(fittingText(5), "No entra ni a 2x: acorta el tramo 5 s o sube la velocidad a mano.");
  assert.equal(fittingText(0.1), "No entra ni a 2x: acorta el tramo 0,1 s o sube la velocidad a mano.");
});

test("cannot create without a destination or with a clip range that does not fit at that speed", () => {
  const clipRange = { start: 0, end: 15 };
  assert.equal(reasonForCreateNo({ preset: null, clipRange, speed: 1 }), "Cargando los destinos…");
  assert.equal(reasonForCreateNo({ preset: sticker, clipRange: null, speed: 1 }), "Elige el tramo del video.");
  assert.equal(
    reasonForCreateNo({ preset: sticker, clipRange, speed: 1 }),
    "A 1x, el tramo de 15 s dura 15 s y el máximo es 10 s. Sube la velocidad o pulsa «Ajustar al máximo».",
  );
  assert.equal(reasonForCreateNo({ preset: sticker, clipRange, speed: 1.5 }), null);
  assert.equal(reasonForCreateNo({ preset: sticker, clipRange: { start: 0, end: 11 }, speed: 1.1 }), null);
});

test("the request carries the input clip range duration, not the output one", () => {
  const request = requestOfEditor({
    id: 3,
    preset: sticker,
    clipRange: { start: 2.3, end: 6.3 },
    speed: 0.5,
    priority: "smoothness",
    fps: 24,
    framing: { ...DEFAULT_FRAMING, center_x: 0.4, zoom: 2 },
    sidePx: 320,
  });
  assert.deepEqual(request, {
    id: 3,
    preset: "whatsapp-sticker",
    start_s: 2.3,
    seconds: 4,
    speed: 0.5,
    priority: "smoothness",
    fps: 24,
    framing: { ...DEFAULT_FRAMING, center_x: 0.4, zoom: 2 },
    side_px: 320,
  });
});

function turn(): Promise<void> {
  return new Promise((follow) => setTimeout(follow, 0));
}

function bank() {
  const requested: CreateRequest[] = [];
  const responses: { give(e: EstimateUi): void; fail(kind: string, message: string): void }[] = [];
  const states: EstimateState[] = [];
  const waiters: { ms: number; follow(): void }[] = [];
  const estimator = createEstimator({
    estimate(request) {
      requested.push(request);
      return new Promise<EstimateUi>((bien, bad) => {
        responses.push({ give: bien, fail: (kind, message) => bad({ kind, message }) });
      });
    },
    onChange: (state) => states.push(state),
    wait: (ms) => new Promise<void>((follow) => waiters.push({ ms, follow })),
  });
  return { estimator, requested, responses, states, waiters };
}

const request = (seconds: number): CreateRequest => ({
  id: 1,
  preset: "whatsapp-sticker",
  start_s: 0,
  seconds,
  speed: 1,
  priority: "balance",
  fps: null,
  framing: DEFAULT_FRAMING,
  side_px: null,
});

test("the estimate waits 400 ms without changes before being requested", async () => {
  const { estimator, requested, responses, states, waiters } = bank();
  estimator.request(request(6));
  assert.deepEqual(states, [{ kind: "calculating" }]);
  assert.equal(waiters[0]?.ms, ESTIMATE_WAITS_MS);
  assert.equal(ESTIMATE_WAITS_MS, 400);
  estimator.request(request(7));
  estimator.request(request(8));
  for (const waits of waiters) waits.follow();
  await turn();
  assert.deepEqual(requested, [request(8)]);
  responses[0]?.give(estimate());
  await turn();
  assert.deepEqual(states.at(-1), { kind: "ready", estimate: estimate() });
});

test("a late response does not overwrite the estimate of the current controls", async () => {
  const { estimator, requested, responses, states, waiters } = bank();
  estimator.request(request(6));
  waiters[0]?.follow();
  await turn();
  estimator.request(request(9));
  responses[0]?.give(estimate({ estimated_size_bytes: 1 }));
  await turn();
  assert.deepEqual(states.at(-1), { kind: "calculating" });
  waiters[1]?.follow();
  await turn();
  assert.equal(requested.length, 2);
  responses[1]?.give(estimate());
  await turn();
  assert.deepEqual(states.at(-1), { kind: "ready", estimate: estimate() });
});

test("the estimate cancelled by Rust is ignored and a failure is reported", async () => {
  const { estimator, responses, states, waiters } = bank();
  estimator.request(request(6));
  waiters[0]?.follow();
  await turn();
  estimator.request(request(7));
  responses[0]?.fail("cancelled", "cancelled");
  await turn();
  assert.deepEqual(states.at(-1), { kind: "calculating" });
  waiters[1]?.follow();
  await turn();
  responses[1]?.fail("other", "sin implementar");
  await turn();
  assert.deepEqual(states.at(-1), { kind: "failed", message: "sin implementar" });
});

test("if the last estimate is cancelled, it stays unrequested instead of calculating forever", async () => {
  const { estimator, responses, states, waiters } = bank();
  estimator.request(request(6));
  waiters[0]?.follow();
  await turn();
  responses[0]?.fail("cancelled", "cancelled");
  await turn();
  assert.deepEqual(states.at(-1), { kind: "without-request" });
});

test("asking for nothing leaves the estimate unrequested and discards the one in progress", async () => {
  const { estimator, requested, states, waiters } = bank();
  estimator.request(request(6));
  estimator.request(null);
  assert.deepEqual(states.at(-1), { kind: "without-request" });
  waiters[0]?.follow();
  await turn();
  assert.equal(requested.length, 0);
});

test("a clip range too long for the destination is fixed first by speed, then by trimming", () => {
  assert.equal(durationFix({ preset: sticker, clipRange: { start: 0, end: 8 }, speed: 1 }), null);
  assert.deepEqual(durationFix({ preset: sticker, clipRange: { start: 0, end: 15 }, speed: 1 }), {
    speed: 1.5,
    clipRange: { start: 0, end: 15 },
  });
  assert.deepEqual(durationFix({ preset: sticker, clipRange: { start: 2, end: 30 }, speed: 1 }), {
    speed: 2,
    clipRange: { start: 2, end: 22 },
  });
  assert.equal(durationFix({ preset: null, clipRange: { start: 0, end: 15 }, speed: 1 }), null);
});
