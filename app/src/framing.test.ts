import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DEFAULT_FRAMING,
  MAX_ZOOM,
  clampFraming,
  cropSquare,
  dragFraming,
  withZoom,
  framingResetText,
  pinchFraming,
} from "./framing.ts";

test("the default framing is a centred crop at 1x", () => {
  assert.deepEqual(DEFAULT_FRAMING, {
    mode: "crop",
    center_x: 0.5,
    center_y: 0.5,
    zoom: 1,
    background: { kind: "transparent" },
    shape: "square",
    scale: 1,
    align: "center",
  });
});

test("at 1x the square covers the short side of a landscape frame", () => {
  const square = cropSquare(DEFAULT_FRAMING, 1920, 1080);
  assert.ok(Math.abs(square.width - 1080 / 1920) < 1e-9);
  assert.equal(square.height, 1);
  assert.ok(Math.abs(square.left - (1 - square.width) / 2) < 1e-9);
  assert.equal(square.top, 0);
});

test("zooming shrinks the square", () => {
  const square = cropSquare({ ...DEFAULT_FRAMING, zoom: 2 }, 1000, 1000);
  assert.equal(square.width, 0.5);
  assert.equal(square.height, 0.5);
});

test("the centre is clamped so the square stays inside the frame", () => {
  const moved = clampFraming({ ...DEFAULT_FRAMING, center_x: 0, center_y: 2, zoom: 2 }, 1000, 1000);
  assert.equal(moved.center_x, 0.25);
  assert.equal(moved.center_y, 0.75);
});

test("zoom is limited to 1..4 and bad numbers fall back", () => {
  assert.equal(clampFraming({ ...DEFAULT_FRAMING, zoom: 9 }, 100, 100).zoom, MAX_ZOOM);
  assert.equal(clampFraming({ ...DEFAULT_FRAMING, zoom: 0.2 }, 100, 100).zoom, 1);
  assert.deepEqual(clampFraming({ ...DEFAULT_FRAMING, center_x: Number.NaN, zoom: Number.NaN }, 100, 100), DEFAULT_FRAMING);
});

test("dragging moves the centre by the fraction of the displayed frame", () => {
  const start = { ...DEFAULT_FRAMING, zoom: 2 };
  const moved = dragFraming(start, 50, -100, 200, 200, 1000, 1000);
  assert.equal(moved.center_x, 0.75);
  assert.equal(moved.center_y, 0.25);
});

test("dragging never leaves the frame", () => {
  const moved = dragFraming({ ...DEFAULT_FRAMING, zoom: 2 }, 5000, 5000, 200, 200, 1000, 1000);
  assert.equal(moved.center_x, 0.75);
  assert.equal(moved.center_y, 0.75);
});

test("changing zoom keeps the square inside the frame", () => {
  const edge = { ...DEFAULT_FRAMING, center_x: 0.75, zoom: 2 };
  assert.equal(withZoom(edge, 1, 1000, 1000).center_x, 0.5);
});

test("the reset text depends on the mode", () => {
  assert.equal(framingResetText({ ...DEFAULT_FRAMING, zoom: 2 }), "Centrar y quitar el zoom");
  assert.equal(framingResetText(DEFAULT_FRAMING), null);
});

test("pinching zooms by how far the fingers moved apart, within the limits", () => {
  assert.equal(pinchFraming(DEFAULT_FRAMING, 100, 200, 640, 360).zoom, 2);
  assert.equal(pinchFraming({ ...DEFAULT_FRAMING, zoom: 2 }, 200, 100, 640, 360).zoom, 1);
  assert.equal(pinchFraming(DEFAULT_FRAMING, 10, 900, 640, 360).zoom, MAX_ZOOM);
  assert.equal(pinchFraming({ ...DEFAULT_FRAMING, zoom: 2 }, 0, 100, 640, 360).zoom, 2);
});
