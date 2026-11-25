import assert from "node:assert/strict";
import { test } from "node:test";

import { canSaveClipVideo, clipVideoText } from "./clip-video.ts";
import type { MediaUi } from "./contract.ts";

const video: MediaUi = { kind: "video", duration_s: 8, width: 640, height: 360, has_audio: true };
const image: MediaUi = { kind: "image", path: "/tmp/a.png", width: 10, height: 10 };

test("the button is for videos in the sticker editor on Android", () => {
  assert.equal(canSaveClipVideo("android", "sticker", video), true);
  assert.equal(canSaveClipVideo("android", "sticker", image), false);
  assert.equal(canSaveClipVideo("android", "gif", video), false);
  assert.equal(canSaveClipVideo("desktop", "sticker", video), false);
});

test("the text tells what to do in WhatsApp, or why it failed", () => {
  assert.equal(clipVideoText({ kind: "idle" }), "");
  assert.equal(clipVideoText({ kind: "saving" }), "Guardando vídeo…");
  assert.equal(
    clipVideoText({ kind: "saved", name: "a-sticker.mp4" }),
    "Vídeo guardado. En WhatsApp: Stickers → Crear sticker → elige este vídeo. Si tu WhatsApp ya tiene stickers con sonido, lo incluirá.",
  );
  assert.equal(clipVideoText({ kind: "failed", message: "HTTP 403" }), "HTTP 403");
});
