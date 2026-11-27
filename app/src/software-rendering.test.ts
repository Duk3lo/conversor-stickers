import assert from "node:assert/strict";
import { test } from "node:test";

import { isSoftwareRenderer } from "./software-rendering.ts";

test("software renderers are recognised, whatever the case or the wrapper", () => {
  assert.equal(isSoftwareRenderer("ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero)), SwiftShader driver)"), true);
  assert.equal(isSoftwareRenderer("Mesa llvmpipe (LLVM 15.0.7, 256 bits)"), true);
  assert.equal(isSoftwareRenderer("Gallium 0.4 on softpipe"), true);
  assert.equal(isSoftwareRenderer("Microsoft Basic Render Driver"), true);
});

test("hardware renderers and a missing renderer do not count", () => {
  assert.equal(isSoftwareRenderer("ANGLE (Qualcomm, Adreno (TM) 730, OpenGL ES 3.2)"), false);
  assert.equal(isSoftwareRenderer("Mali-G78"), false);
  assert.equal(isSoftwareRenderer(""), false);
  assert.equal(isSoftwareRenderer(null), false);
});
