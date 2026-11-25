import assert from "node:assert/strict";
import { test } from "node:test";

import {
  boundSpeed,
  fitMax,
  fitClipRange,
  barScope,
  outputDuration,
  clipRangeDuration,
  maxOfClipRange,
  moveEnd,
  moveStart,
  shortSecondsText,
  clipRangeText,
  speedText,
  fitsClipRange,
  clipRangeByDefault,
} from "./crop.ts";

test("default clip range: min(6, cap, duration) from the start", () => {
  const cases: [number, number, number][] = [
    [60, 10, 6],
    [60, 5, 5],
    [3.2, 10, 3.2],
    [3.27, 10, 3.2],
    [6, 30, 6],
  ];
  for (const [duration, cap, end] of cases) {
    assert.deepEqual(clipRangeByDefault(duration, cap), { start: 0, end }, `${duration} s, tope ${cap}`);
  }
});

test("a video shorter than a step uses its whole duration", () => {
  assert.deepEqual(clipRangeByDefault(0.05, 10), { start: 0, end: 0.05 });
});

test("switching from discord-gif (30 s) to discord-sticker (5 s) trims the clip range", () => {
  const clipRange = { start: 4, end: 24 };
  assert.deepEqual(fitClipRange(clipRange, 60, 5), { start: 4, end: 9 });
});

test("switching to a shorter video fits the clip range inside the video", () => {
  assert.deepEqual(fitClipRange({ start: 8, end: 14 }, 10, 10), { start: 8, end: 10 });
  assert.deepEqual(fitClipRange({ start: 12, end: 14 }, 10, 10), { start: 9.9, end: 10 });
});

test("removing size fitting trims the clip range back to the cap", () => {
  const withFit = moveEnd({ start: 0, end: 6 }, 18, 60, 20);
  assert.deepEqual(withFit, { start: 0, end: 18 });
  assert.deepEqual(fitClipRange(withFit, 60, 10), { start: 0, end: 10 });
});

test("the handles move in 0.1 s steps", () => {
  assert.deepEqual(moveStart({ start: 0, end: 6 }, 1.234, 60, 10), { start: 1.2, end: 6 });
  assert.deepEqual(moveEnd({ start: 0, end: 6 }, 5.06, 60, 10), { start: 0, end: 5.1 });
  assert.deepEqual(moveStart({ start: 0.3, end: 6 }, 0.3 - 0.1, 60, 10), { start: 0.2, end: 6 });
});

test("the handles do not cross: they leave at least 0.1 s", () => {
  assert.deepEqual(moveStart({ start: 2, end: 6 }, 9, 60, 10), { start: 5.9, end: 6 });
  assert.deepEqual(moveEnd({ start: 2, end: 6 }, 1, 60, 10), { start: 2, end: 2.1 });
});

test("going past the maximum drags the other handle", () => {
  assert.deepEqual(moveStart({ start: 20, end: 26 }, 10, 60, 10), { start: 10, end: 20 });
  assert.deepEqual(moveEnd({ start: 2, end: 6 }, 15, 60, 10), { start: 5, end: 15 });
});

test("the handles do not leave the video", () => {
  assert.deepEqual(moveStart({ start: 2, end: 6 }, -3, 60, 10), { start: 0, end: 6 });
  assert.deepEqual(moveEnd({ start: 2, end: 6 }, 99, 7.25, 10), { start: 2, end: 7.2 });
});

test("the clip range text uses a decimal comma and one digit", () => {
  assert.equal(clipRangeText({ start: 3.2, end: 9.2 }), "3,2 s → 9,2 s · 6,0 s");
  assert.equal(clipRangeText({ start: 0, end: 10 }), "0,0 s → 10,0 s · 10,0 s");
  assert.equal(clipRangeDuration({ start: 0.1, end: 0.3 }), 0.2);
});

test("the valid maximum of the clip range is the cap times the speed; the bar reaches 3x", () => {
  assert.equal(maxOfClipRange(10, 1), 10);
  assert.equal(maxOfClipRange(10, 0.5), 5);
  assert.equal(maxOfClipRange(10, 1.5), 15);
  assert.equal(barScope(10), 30);
  assert.equal(barScope(5), 15);
});

test("the clip range fits if at that speed it does not exceed the cap, with floating point tolerance", () => {
  assert.equal(fitsClipRange(10, 10, 1), true);
  assert.equal(fitsClipRange(10.1, 10, 1), false);
  assert.equal(fitsClipRange(4, 10, 0.5), true);
  assert.equal(fitsClipRange(5.1, 10, 0.5), false);
  assert.equal(fitsClipRange(11, 10, 1.1), true);
  assert.equal(fitsClipRange(30, 10, 3), true);
});

test("the output duration is the clip range divided by the speed: 4 s at 0.5x last 8 s", () => {
  assert.equal(outputDuration(4, 0.5), 8);
  assert.equal(outputDuration(15, 1.5), 10);
  assert.equal(outputDuration(6, 1), 6);
});

test("the speed is bounded to 0.5-3 in 0.05 steps", () => {
  const cases: [number, number][] = [
    [1, 1],
    [0.2, 0.5],
    [7, 3],
    [1.23, 1.25],
    [1.22, 1.2],
    [Number.NaN, 1],
  ];
  for (const [value, expected] of cases) assert.equal(boundSpeed(value), expected, String(value));
});

test("size fitting speeds up just enough, in 0.05 steps upwards, between 1x and 2x", () => {
  const cases: [number, number, number][] = [
    [15, 10, 1.5],
    [11, 10, 1.1],
    [10.3, 10, 1.05],
    [20, 10, 2],
    [6, 10, 1],
    [2, 10, 1],
    [7, 5, 1.4],
    [12.1, 10, 1.25],
  ];
  for (const [clipRange, cap, speed] of cases) {
    const fitting = fitMax(clipRange, cap);
    assert.deepEqual(fitting, { speed, cropS: null }, `${clipRange} s con tope ${cap}`);
    assert.ok(fitsClipRange(clipRange, cap, fitting.speed), `${clipRange} s cabe a ${fitting.speed}x`);
  }
});

test("if it does not fit even at 2x, size fitting says how much to trim", () => {
  assert.deepEqual(fitMax(25, 10), { speed: 2, cropS: 5 });
  assert.deepEqual(fitMax(20.05, 10), { speed: 2, cropS: 0.1 });
  assert.deepEqual(fitMax(12, 5), { speed: 2, cropS: 2 });
});

test("the speed is written with a comma and without extra zeros", () => {
  const cases: [number, string][] = [
    [1, "1x"],
    [1.5, "1,5x"],
    [0.5, "0,5x"],
    [1.25, "1,25x"],
    [1.05, "1,05x"],
    [2, "2x"],
    [3, "3x"],
  ];
  for (const [value, text] of cases) assert.equal(speedText(value), text);
});

test("short seconds carry no extra decimals", () => {
  assert.equal(shortSecondsText(10), "10 s");
  assert.equal(shortSecondsText(6.5), "6,5 s");
  assert.equal(shortSecondsText(6.54), "6,5 s");
  assert.equal(shortSecondsText(6.96), "7 s");
});
