import assert from "node:assert/strict";
import { test } from "node:test";

import { clipBarLabels, fitToTime, maxRangeS, moveRange, shadedRegion } from "./clipbar.ts";

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
