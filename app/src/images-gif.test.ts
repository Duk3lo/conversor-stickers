import assert from "node:assert/strict";
import { test } from "node:test";

import { clampImageSeconds, imageSecondsText, imagesGifRequest, MAX_GIF_IMAGES, imagesChosenText } from "./images-gif.ts";

test("the time of each image stays between 0,1 and 5 s in steps of 0,1", () => {
  assert.equal(clampImageSeconds(0.04), 0.1);
  assert.equal(clampImageSeconds(7), 5);
  assert.equal(clampImageSeconds(0.54), 0.5);
  assert.equal(clampImageSeconds(Number.NaN), 0.5);
});

test("the time is written in Spanish with a decimal comma", () => {
  assert.equal(imageSecondsText(0.5), "0,5 s");
  assert.equal(imageSecondsText(2), "2 s");
});

test("the request keeps the order of selection", () => {
  assert.deepEqual(imagesGifRequest([4, 2, 9], 0.5, true), { ids: [4, 2, 9], image_s: 0.5, repeat: true });
});

test("the text says whether the GIF is still or animated", () => {
  assert.equal(MAX_GIF_IMAGES, 50);
  assert.equal(imagesChosenText(1), "1 imagen: GIF sin movimiento");
  assert.equal(imagesChosenText(3), "3 imágenes: GIF animado");
});
