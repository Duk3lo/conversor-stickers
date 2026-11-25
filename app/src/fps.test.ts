import assert from "node:assert/strict";
import { test } from "node:test";

import type { PresetUi } from "./contract.ts";
import { fitPriority, fpsAllowed, fpsChoices, fpsLabel } from "./fps.ts";

const preset = (max_fps: number): PresetUi => ({
  name: "x",
  title: "X",
  extension: "gif",
  side: 480,
  max_size_bytes: 1,
  max_duration_s: 10,
  max_fps,
  side_range_px: null,
  fps_range: { min: 5, max: max_fps },
});

test("the chips are automatic plus the values the destination allows", () => {
  assert.deepEqual(fpsChoices(preset(30)), [null, 15, 20, 24, 30]);
  assert.deepEqual(fpsChoices(preset(24)), [null, 15, 20, 24]);
  assert.deepEqual(fpsChoices(preset(16)), [null, 15]);
  assert.deepEqual(fpsChoices(null), [null]);
});

test("a chosen fps is only kept if the destination allows it", () => {
  assert.equal(fpsAllowed(preset(30), 30), true);
  assert.equal(fpsAllowed(preset(20), 30), false);
  assert.equal(fpsAllowed(preset(20), null), true);
  assert.equal(fpsAllowed(preset(30), 17), true);
  assert.equal(fpsAllowed(preset(30), 4), false);
});

test("labels and the fitting priority follow the chosen fps", () => {
  assert.equal(fpsLabel(null), "Auto");
  assert.equal(fpsLabel(24), "24");
  assert.equal(fitPriority(null), "fit_anyway");
  assert.equal(fitPriority(24), "fluid");
});
