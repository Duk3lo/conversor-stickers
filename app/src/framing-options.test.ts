import assert from "node:assert/strict";
import { test } from "node:test";

import { DEFAULT_FRAMING } from "./framing.ts";
import {
  SCALES,
  scaleText,
  sideChoices,
  withAlign,
  withBackground,
  withColour,
  withScale,
  withShape,
  isHexColour,
  contentBox,
  fitLayout,
  shapeRadius,
} from "./framing-options.ts";

test("the defaults are crop, transparent, square, full size and centred", () => {
  assert.equal(DEFAULT_FRAMING.background.kind, "transparent");
  assert.equal(DEFAULT_FRAMING.shape, "square");
  assert.equal(DEFAULT_FRAMING.scale, 1);
  assert.equal(DEFAULT_FRAMING.align, "center");
});

test("the size chips go from full size down to half", () => {
  assert.deepEqual(SCALES, [1, 0.85, 0.7, 0.5]);
  assert.equal(scaleText(0.85), "85 %");
  assert.equal(scaleText(1), "100 %");
});

test("choosing a colour background keeps a previous colour or starts on white", () => {
  const first = withBackground(DEFAULT_FRAMING, "color");
  assert.deepEqual(first.background, { kind: "color", rgb: "#ffffff" });
  const red = withColour(first, "#ff0000");
  const blur = withBackground(red, "blur");
  assert.deepEqual(blur.background, { kind: "blur" });
  assert.deepEqual(withBackground(blur, "color").background, { kind: "color", rgb: "#ffffff" });
  assert.deepEqual(withBackground(red, "color").background, { kind: "color", rgb: "#ff0000" });
});

test("a colour is only accepted as #rrggbb", () => {
  assert.equal(isHexColour("#a1B2c3"), true);
  assert.equal(isHexColour("red"), false);
  assert.equal(isHexColour("#fff"), false);
  assert.deepEqual(withColour(DEFAULT_FRAMING, "nope"), DEFAULT_FRAMING);
});

test("the other options change only their own field", () => {
  assert.equal(withShape(DEFAULT_FRAMING, "circle").shape, "circle");
  assert.equal(withAlign(DEFAULT_FRAMING, "top").align, "top");
  assert.equal(withScale(DEFAULT_FRAMING, 0.7).scale, 0.7);
  assert.equal(withScale(DEFAULT_FRAMING, 0.1).scale, 0.5);
  assert.equal(withScale(DEFAULT_FRAMING, 3).scale, 1);
});

test("the GIF side chips offer the default plus each suggested side", () => {
  assert.deepEqual(sideChoices({ side_range_px: null }), []);
  assert.deepEqual(sideChoices({ side_range_px: { min: 64, max: 480, suggested: [240, 480] } }), [
    { value: null, label: "Original" },
    { value: 240, label: "240 px" },
    { value: 480, label: "480 px" },
  ]);
});

test("a wide image fits the canvas width and sits where the alignment says", () => {
  const wide = { ...DEFAULT_FRAMING, mode: "fit" as const };
  assert.deepEqual(fitLayout(wide, 200, 100), { left: 0, top: 0.25, width: 1, height: 0.5 });
  assert.deepEqual(fitLayout({ ...wide, align: "top" }, 200, 100), { left: 0, top: 0, width: 1, height: 0.5 });
  assert.deepEqual(fitLayout({ ...wide, align: "bottom" }, 200, 100), { left: 0, top: 0.5, width: 1, height: 0.5 });
});

test("the scale shrinks the content square around the centre", () => {
  const half = { ...DEFAULT_FRAMING, mode: "fit" as const, scale: 0.5 };
  assert.deepEqual(contentBox(half), { left: 0.25, top: 0.25, size: 0.5 });
  assert.deepEqual(contentBox({ ...half, mode: "crop" }), { left: 0, top: 0, size: 1 });
  assert.deepEqual(fitLayout({ ...half, align: "top" }, 200, 100), { left: 0, top: 0, width: 1, height: 0.5 });
});

test("crop mode shows the whole source, centred", () => {
  assert.deepEqual(fitLayout({ ...DEFAULT_FRAMING, scale: 0.5 }, 100, 200), { left: 0.25, top: 0, width: 0.5, height: 1 });
});

test("the shape maps to a corner radius", () => {
  assert.equal(shapeRadius("square"), "0");
  assert.equal(shapeRadius("rounded"), "14%");
  assert.equal(shapeRadius("circle"), "50%");
});
