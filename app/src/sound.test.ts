import assert from "node:assert/strict";
import { test } from "node:test";

import type { MediaUi } from "./contract.ts";
import { canSaveSound, initialSound, soundRequest, soundText } from "./sound.ts";

const video = (has_audio: boolean): MediaUi => ({ kind: "video", duration_s: 8, width: 640, height: 360, has_audio });
const image: MediaUi = { kind: "image", path: "/tmp/a.png", width: 10, height: 10 };

test("the button shows for a video with audio and for links, whose audio is not known", () => {
  assert.equal(canSaveSound(video(true), "file"), true);
  assert.equal(canSaveSound(video(false), "file"), false);
  assert.equal(canSaveSound(video(false), "pasted"), false);
  assert.equal(canSaveSound(video(false), "link"), true);
  assert.equal(canSaveSound(image, "link"), false);
});

test("the sound starts idle or unavailable", () => {
  assert.deepEqual(initialSound(video(true), "file"), { kind: "idle" });
  assert.deepEqual(initialSound(video(false), "file"), { kind: "unavailable" });
});

test("the request is the clip range capped at ten seconds", () => {
  assert.deepEqual(soundRequest({ start: 2, end: 6.5 }), { startS: 2, seconds: 4.5 });
  assert.deepEqual(soundRequest({ start: 1, end: 30 }), { startS: 1, seconds: 10 });
});

test("the text follows the state", () => {
  assert.equal(soundText({ kind: "idle" }), "");
  assert.equal(soundText({ kind: "saving" }), "Guardando sonido…");
  assert.equal(soundText({ kind: "saved", name: "a.m4a" }), "Sonido guardado como a.m4a");
  assert.equal(soundText({ kind: "failed", message: "Sin audio" }), "Sin audio");
});
