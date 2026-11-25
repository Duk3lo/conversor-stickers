import assert from "node:assert/strict";
import { test } from "node:test";

import type { CreateResult } from "./contract.ts";
import { resultDetail, progressFraction, resultSizeText, progressText } from "./progress.ts";

test("text of each conversion stage", () => {
  assert.equal(progressText(null), "Preparando…");
  assert.equal(progressText({ kind: "decoding" }), "Leyendo el video…");
  assert.equal(progressText({ kind: "preview" }), "Preparando la vista previa…");
  assert.equal(
    progressText({ kind: "fitting_size", attempt: 2, max: 8 }),
    "Ajustando el peso · intento 2 de 8",
  );
  assert.equal(
    progressText({ kind: "searching_duration", attempt: 1, max: 4 }),
    "No cabe; buscando una duración que quepa · 1 de 4",
  );
});

test("the bar only advances when it is known how much remains", () => {
  assert.equal(progressFraction(null), null);
  assert.equal(progressFraction({ kind: "decoding" }), null);
  assert.equal(progressFraction({ kind: "fitting_size", attempt: 2, max: 8 }), 0.25);
  assert.equal(progressFraction({ kind: "searching_duration", attempt: 4, max: 4 }), 1);
  assert.equal(progressFraction({ kind: "fitting_size", attempt: 1, max: 0 }), null);
});

const result: CreateResult = {
  meets_limit: true,
  size_bytes: 411_200,
  max_size_bytes: 500_000,
  fps: 12,
  duration_s: 6,
  reductions: ["fps 15→12", "calidad 90→70"],
  notice: null,
};

test("result size against the maximum", () => {
  assert.equal(resultSizeText(result), "412 KB de 500 KB");
  assert.equal(
    resultSizeText({ ...result, size_bytes: 9_400_000, max_size_bytes: 10_000_000 }),
    "9,4 MB de 10 MB",
  );
});

test("detail with fps, duration and reductions", () => {
  assert.equal(resultDetail(result), "12 fps · 6,0 s · Reducido: fps 15→12, calidad 90→70");
  assert.equal(resultDetail({ ...result, reductions: [] }), "12 fps · 6,0 s · Sin reducciones");
});

test("a static sticker shows neither fps nor duration", () => {
  assert.equal(resultDetail({ ...result, fps: null, duration_s: null, reductions: [] }), "Sin reducciones");
});
