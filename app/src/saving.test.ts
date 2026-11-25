import assert from "node:assert/strict";
import { test } from "node:test";

import { savedNotice, savingPercentage, savingText, startSaving } from "./saving.ts";

test("text and percentage of each stage of saving a video", () => {
  const start = startSaving(3);
  assert.equal(savingText(start), "Preparando…");
  assert.equal(savingPercentage(start), null);

  const known = { ...start, event: { kind: "downloading", bytes: 250, total: 1000 } } as const;
  assert.equal(savingText(known), "Descargando con sonido 25 %");
  assert.equal(savingPercentage(known), 25);

  const unknown = { ...start, event: { kind: "downloading", bytes: 2_500_000, total: null } } as const;
  assert.equal(savingText(unknown), "Descargando con sonido 2,5 MB");
  assert.equal(savingPercentage(unknown), null);

  const over = { ...start, event: { kind: "downloading", bytes: 1200, total: 1000 } } as const;
  assert.equal(savingPercentage(over), 100);

  const writing = { ...start, event: { kind: "writing" } } as const;
  assert.equal(savingText(writing), "Guardando…");
  assert.equal(savingPercentage(writing), 100);
});

test("the notice says where it was saved", () => {
  assert.equal(savedNotice("android", "Gato.mp4"), "Guardado en Vídeos");
  assert.equal(savedNotice("desktop", "Gato.mp4"), "Guardado como Gato.mp4");
});
