import assert from "node:assert/strict";
import { test } from "node:test";

import {
  barFraction,
  clipBarLabels,
  durationFits,
  editorRecommendedS,
  fitToTime,
  maxRangeS,
  moveRange,
  recommendedDurationS,
  secondAt,
  shadedRegion,
  snapMarginS,
  snapToOriginal,
  zoomSpan,
} from "./clipbar.ts";

test("moving the range keeps its length", () => {
  assert.deepEqual(moveRange({ start: 1, end: 3.5 }, 4, 20), { start: 4, end: 6.5 });
});

test("moving the range stops at both ends of the video", () => {
  assert.deepEqual(moveRange({ start: 1, end: 3.5 }, -2, 20), { start: 0, end: 2.5 });
  assert.deepEqual(moveRange({ start: 1, end: 3.5 }, 19, 20), { start: 17.5, end: 20 });
});

test("moving avoids float noise", () => {
  assert.deepEqual(moveRange({ start: 0, end: 0.3 }, 0.1, 5), { start: 0.1, end: 0.4 });
});

test("fit to time keeps the start when there is room", () => {
  assert.deepEqual(fitToTime({ start: 2, end: 9 }, 4.5, 20), { start: 2, end: 6.5 });
});

test("fit to time moves the start back when the end would pass the video", () => {
  assert.deepEqual(fitToTime({ start: 18, end: 19 }, 4, 20), { start: 16, end: 20 });
});

test("fit to time never exceeds the video nor drops below a tenth", () => {
  assert.deepEqual(fitToTime({ start: 0, end: 3 }, 50, 10), { start: 0, end: 10 });
  assert.deepEqual(fitToTime({ start: 1, end: 3 }, 0, 10), { start: 1, end: 1.1 });
});

test("the longest range is the smaller of the cap and the estimate", () => {
  const preset = { max_duration_s: 10 };
  assert.equal(maxRangeS(preset, 1, null), 10);
  assert.equal(maxRangeS(preset, 2, null), 20);
  assert.equal(maxRangeS(preset, 1, 4.5), 4.5);
  assert.equal(maxRangeS(preset, 1, 40), 10);
  assert.equal(maxRangeS(null, 1, null), null);
});

test("the shaded part starts where the range stops fitting", () => {
  assert.deepEqual(shadedRegion({ start: 2, end: 10 }, 5, 20), { left: 0.35, width: 0.15 });
});

test("nothing is shaded when the range fits", () => {
  assert.equal(shadedRegion({ start: 2, end: 6 }, 5, 20), null);
  assert.equal(shadedRegion({ start: 2, end: 7 }, 5, 20), null);
});

test("the labels name start, end, length and output length", () => {
  assert.deepEqual(clipBarLabels({ start: 0.4, end: 1.9 }, 2), {
    start: "0,4 s",
    end: "1,9 s",
    length: "1,5 s",
    output: "0,8 s",
  });
});

const recommendation = {
  durationS: 60,
  preset: { max_duration_s: 10 },
  speed: 1,
  estimatedS: null,
  withSound: false,
};

test("the recommended length is the shortest of every limit", () => {
  assert.equal(recommendedDurationS(recommendation), 10);
  assert.equal(recommendedDurationS({ ...recommendation, durationS: 4.25 }), 4.2);
  assert.equal(recommendedDurationS({ ...recommendation, speed: 2 }), 20);
  assert.equal(recommendedDurationS({ ...recommendation, speed: 2, estimatedS: 12.38 }), 12.3);
  assert.equal(recommendedDurationS({ ...recommendation, speed: 2, withSound: true }), 10);
  assert.equal(recommendedDurationS({ ...recommendation, estimatedS: 6.5, withSound: true }), 6.5);
});

test("the recommended length is truncated without float noise", () => {
  assert.equal(recommendedDurationS({ ...recommendation, preset: { max_duration_s: 3 }, speed: 1.1 }), 3.3);
});

test("the recommended length is at least a tenth and needs a destination", () => {
  assert.equal(recommendedDurationS({ ...recommendation, estimatedS: 0 }), 0.1);
  assert.equal(recommendedDurationS({ ...recommendation, preset: null }), null);
});

test("the editor limits to the sound length only for stickers that can save sound", () => {
  const editor = {
    duration: 60,
    preset: { max_duration_s: 10 },
    speed: 2,
    estimate: { kind: "calculating" },
    destination: "sticker",
    sound: { kind: "idle" },
  };
  assert.equal(editorRecommendedS(editor), 10);
  assert.equal(editorRecommendedS({ ...editor, destination: "gif" }), 20);
  assert.equal(editorRecommendedS({ ...editor, sound: { kind: "unavailable" } }), 20);
  assert.equal(editorRecommendedS({ ...editor, estimate: { kind: "ready", estimate: { max_range_s: 7.5 } } }), 7.5);
  assert.equal(editorRecommendedS({ ...editor, duration: null }), null);
});

test("a length fits when both the video and the bar allow it", () => {
  assert.equal(durationFits(5, 5, 30), true);
  assert.equal(durationFits(10, 9.9, 30), false);
  assert.equal(durationFits(10, 60, 9), false);
});

test("short videos have no zoomed bar", () => {
  assert.equal(zoomSpan({ start: 2, end: 8 }, 30), null);
});

test("the zoomed bar spans thirty seconds centred on the range", () => {
  assert.deepEqual(zoomSpan({ start: 47, end: 53 }, 100), { from: 35, to: 65 });
});

test("the zoomed bar stays inside the video", () => {
  assert.deepEqual(zoomSpan({ start: 1, end: 7 }, 100), { from: 0, to: 30 });
  assert.deepEqual(zoomSpan({ start: 95, end: 99 }, 100), { from: 70, to: 100 });
});

test("the zoomed bar grows to leave room around a long range", () => {
  assert.deepEqual(zoomSpan({ start: 40, end: 70 }, 200), { from: 25, to: 85 });
  assert.deepEqual(zoomSpan({ start: 0, end: 30 }, 40), { from: 0, to: 40 });
});

test("positions on a bar are fractions of its span, clamped to its ends", () => {
  const span = { from: 20, to: 60 };
  assert.equal(barFraction(30, span), 0.25);
  assert.equal(barFraction(10, span), 0);
  assert.equal(barFraction(70, span), 1);
  assert.equal(secondAt(0.25, span), 30);
  assert.equal(secondAt(-1, span), 20);
  assert.equal(secondAt(2, span), 60);
});

test("the zoomed bar snaps within three tenths", () => {
  assert.equal(snapMarginS(null), 0.3);
});

test("the general bar snaps within four pixels, between three tenths and a second", () => {
  assert.equal(snapMarginS({ durationS: 10, widthPx: 400 }), 0.3);
  assert.equal(snapMarginS({ durationS: 50, widthPx: 400 }), 0.5);
  assert.equal(snapMarginS({ durationS: 600, widthPx: 400 }), 1);
  assert.equal(snapMarginS({ durationS: 600, widthPx: 0 }), 0.3);
});

const original = { start: 0, end: 6 };

test("a drag that ends close to the original range goes back to it", () => {
  assert.deepEqual(snapToOriginal(original, { start: 4, end: 10 }, { start: 0.3, end: 6.3 }, 0.3), original);
  assert.deepEqual(
    snapToOriginal({ start: 0.4, end: 6.4 }, { start: 4, end: 10 }, { start: 0.1, end: 6.7 }, 0.3),
    { start: 0.4, end: 6.4 },
  );
});

test("a drag does not snap unless both ends are close", () => {
  assert.equal(snapToOriginal(original, { start: 4, end: 10 }, { start: 0.2, end: 6.4 }, 0.3), null);
  assert.equal(snapToOriginal(original, { start: 4, end: 10 }, { start: 0.4, end: 6 }, 0.3), null);
});

test("a drag that did not change the range never snaps", () => {
  assert.equal(snapToOriginal(original, { start: 0.2, end: 6.2 }, { start: 0.2, end: 6.2 }, 0.3), null);
});

test("a drag that starts inside the margin is a fine adjustment and does not snap", () => {
  assert.equal(snapToOriginal(original, original, { start: 0.2, end: 6.2 }, 0.3), null);
  assert.equal(snapToOriginal(original, { start: 0.2, end: 6.2 }, { start: 0.1, end: 6.1 }, 0.3), null);
});
