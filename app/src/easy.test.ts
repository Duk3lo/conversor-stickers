import assert from "node:assert/strict";
import { test } from "node:test";

import type { EstimateUi } from "./contract.ts";
import { DEFAULT_FRAMING } from "./framing.ts";
import { EASY_DEFAULTS, easyReset, easyStep, easySummary, loadMode, saveMode } from "./easy.ts";

const estimate = (changes: Partial<EstimateUi> = {}): EstimateUi => ({
  estimated_size_bytes: 400_000,
  max_size_bytes: 500_000,
  fits: true,
  fps: 15,
  max_seconds_s: 6,
  detail_px: 512,
  max_range_s: 6,
  ...changes,
});

const base = {
  state: { kind: "ready", estimate: estimate() } as const,
  reasonNo: null,
  fitted: false,
  fitPending: false,
  impossible: false,
};

test("the mode defaults to easy and survives a missing or broken store", () => {
  assert.equal(loadMode(null), "easy");
  assert.equal(loadMode({ getItem: () => "rubbish", setItem: () => {} }), "easy");
  assert.equal(loadMode({ getItem: () => "advanced", setItem: () => {} }), "advanced");
  assert.equal(
    loadMode({
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {},
    }),
    "easy",
  );
});

test("saving the mode never throws", () => {
  const saved: string[] = [];
  saveMode({ getItem: () => null, setItem: (_key, value) => void saved.push(value) }, "advanced");
  assert.deepEqual(saved, ["advanced"]);
  saveMode(null, "easy");
  saveMode(
    {
      getItem: () => null,
      setItem: () => {
        throw new Error("full");
      },
    },
    "easy",
  );
});

test("resetting to easy reports nothing when the settings are already the defaults", () => {
  assert.equal(easyReset(EASY_DEFAULTS).changed, false);
});

test("resetting to easy restores the defaults and says it changed", () => {
  const reset = easyReset({ speed: 2, priority: "sharpness", fps: 24, framing: DEFAULT_FRAMING, sidePx: 256 });
  assert.equal(reset.changed, true);
  assert.deepEqual(reset.settings, EASY_DEFAULTS);
});

test("it creates straight away when the estimate fits", () => {
  assert.equal(easyStep(base).kind, "create");
});

test("it waits while the estimate is being calculated or a fit is pending", () => {
  assert.equal(easyStep({ ...base, state: { kind: "calculating" } }).kind, "wait");
  assert.equal(easyStep({ ...base, state: { kind: "without-request" } }).kind, "wait");
  assert.equal(easyStep({ ...base, fitPending: true }).kind, "wait");
});

test("it runs the fit once when the estimate says it does not fit", () => {
  const state = { kind: "ready", estimate: estimate({ fits: false }) } as const;
  assert.equal(easyStep({ ...base, state }).kind, "fit");
  assert.equal(easyStep({ ...base, state, fitted: true }).kind, "create");
});

test("it runs the fit when the range is too long for the speed", () => {
  const step = easyStep({ ...base, state: { kind: "without-request" }, reasonNo: "demasiado largo" });
  assert.equal(step.kind, "fit");
  assert.equal(easyStep({ ...base, state: { kind: "without-request" }, reasonNo: "x", fitted: true }).kind, "stop");
});

test("it stops when even the fit cannot make it fit", () => {
  const state = { kind: "ready", estimate: estimate({ fits: false }) } as const;
  assert.equal(easyStep({ ...base, state, fitted: true, impossible: true }).kind, "stop");
});

test("it creates anyway when the estimate fails", () => {
  assert.equal(easyStep({ ...base, state: { kind: "failed", message: "x" } }).kind, "create");
});

test("the summary says what was changed", () => {
  const before = { speed: 1, clipRange: { start: 0, end: 6 } };
  assert.equal(
    easySummary(before, { speed: 1.6, clipRange: { start: 0, end: 6 } }, true),
    "Se aceleró a 1,6x y se bajó la calidad para que quepa.",
  );
  assert.equal(
    easySummary(before, { speed: 2, clipRange: { start: 0, end: 4.5 } }, true),
    "Se acortó el tramo a 4,5 s, se aceleró a 2x y se bajó la calidad para que quepa.",
  );
  assert.equal(
    easySummary(before, { speed: 1, clipRange: { start: 0, end: 6 } }, true),
    "Se bajó la calidad para que quepa.",
  );
  assert.equal(easySummary(before, before, false), null);
});

test("going back to easy keeps how the video is framed", () => {
  const framing = { ...DEFAULT_FRAMING, mode: "fit" as const, zoom: 2, center_x: 0.4, shape: "circle" as const };
  const reset = easyReset({ ...EASY_DEFAULTS, framing });
  assert.deepEqual(reset.settings.framing, { ...DEFAULT_FRAMING, mode: "fit", zoom: 2, center_x: 0.4 });
});
