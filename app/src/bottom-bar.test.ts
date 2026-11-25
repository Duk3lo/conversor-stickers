import assert from "node:assert/strict";
import { test } from "node:test";

import { bottomAction } from "./bottom-bar.ts";

test("desktop has no bottom actions", () => {
  for (const panel of ["start", "editor", "detail", "mis-packs"] as const) {
    assert.equal(bottomAction("desktop", panel, true), "none");
  }
});

test("Android shows the Create button on the explorer roots", () => {
  for (const panel of ["start", "mis-packs"] as const) {
    assert.equal(bottomAction("android", panel, false), "fab-create");
  }
  assert.equal(bottomAction("android", "community", false), "none");
});

test("Android editor bar depends on whether there is a result", () => {
  assert.equal(bottomAction("android", "editor", false), "editor-create");
  assert.equal(bottomAction("android", "editor", true), "editor-result");
});

test("Android pack detail keeps its action bar and other panels have none", () => {
  assert.equal(bottomAction("android", "detail", false), "pack-bar");
  assert.equal(bottomAction("android", "favorites", false), "none");
  assert.equal(bottomAction("android", "create", false), "none");
  assert.equal(bottomAction("android", "search", false), "none");
});
