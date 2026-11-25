import assert from "node:assert/strict";
import { test } from "node:test";

import type { EstimateUi } from "./contract.ts";
import { clipRangeDuration } from "./crop.ts";
import { planFit, fitPlanText, detailText } from "./fit.ts";

const estimate = (changes: Partial<EstimateUi> = {}): EstimateUi => ({
  estimated_size_bytes: 900_000,
  max_size_bytes: 500_000,
  fits: false,
  fps: 5,
  max_seconds_s: 4,
  detail_px: 512,
  max_range_s: 10,
  ...changes,
});

test("a sticker that already fits is left alone", () => {
  const plan = planFit({ estimate: estimate({ fits: true }), clipRange: { start: 1, end: 6 }, speed: 1 });
  assert.equal(plan.kind, "unchanged");
});

test("speed is raised when that is enough", () => {
  const plan = planFit({ estimate: estimate(), clipRange: { start: 1, end: 7 }, speed: 1 });
  assert.equal(plan.kind, "faster");
  if (plan.kind === "faster") {
    assert.equal(plan.speed, 1.5);
    assert.deepEqual(plan.clipRange, { start: 1, end: 7 });
  }
});

test("the range is shortened when not even 2x is enough", () => {
  const plan = planFit({ estimate: estimate(), clipRange: { start: 1, end: 11 }, speed: 1 });
  assert.equal(plan.kind, "shorter");
  if (plan.kind === "shorter") {
    assert.equal(plan.speed, 2);
    assert.deepEqual(plan.clipRange, { start: 1, end: 9 });
  }
});

test("nothing fits when the floor is below a tenth of a second", () => {
  const plan = planFit({ estimate: estimate({ max_seconds_s: 0.02 }), clipRange: { start: 0, end: 5 }, speed: 1 });
  assert.equal(plan.kind, "impossible");
});

test("a range already short enough at this speed is left alone", () => {
  const plan = planFit({ estimate: estimate({ max_seconds_s: 8 }), clipRange: { start: 0, end: 6 }, speed: 2 });
  assert.equal(plan.kind, "unchanged");
});

test("each plan explains what happened", () => {
  assert.equal(fitPlanText({ kind: "faster", speed: 1.5, clipRange: { start: 0, end: 6 } }), "Velocidad subida a 1,5x para que quepa.");
  assert.equal(
    fitPlanText({ kind: "shorter", speed: 2, clipRange: { start: 1, end: 9 } }),
    "Tramo acortado a 8 s y velocidad 2x para que quepa.",
  );
  assert.ok(fitPlanText({ kind: "impossible" })?.startsWith("Ni así cabe"));
  assert.equal(fitPlanText({ kind: "unchanged" }), null);
});

test("the reduced detail is only mentioned below 512 px", () => {
  assert.equal(detailText(estimate({ detail_px: 256 })), "Detalle reducido a 256 px");
  assert.equal(detailText(estimate({ detail_px: 512 })), null);
});

test("planFit never touches the fps: it only changes speed and range", () => {
  const plan = planFit({ estimate: estimate({ fps: 24 }), clipRange: { start: 1, end: 7 }, speed: 1 });
  assert.equal(plan.kind, "faster");
  assert.equal("fps" in plan, false);
});

test("max_seconds_s is in source seconds: at 1.9x a 10.2 s limit is not enough for 18.8 s", () => {
  const plan = planFit({ estimate: estimate({ max_seconds_s: 10.2 }), clipRange: { start: 0, end: 18.8 }, speed: 1.9 });
  assert.ok(plan.kind === "faster" || plan.kind === "shorter");
  if (plan.kind === "faster" || plan.kind === "shorter") {
    assert.ok(clipRangeDuration(plan.clipRange) / plan.speed <= 10.2 / 1.9 + 1e-6);
  }
});
