import assert from "node:assert/strict";
import { test } from "node:test";

import { canOpenFolder, folderButtonVisible } from "./saved-folder.ts";

test("only Android can open the folders", () => {
  assert.equal(canOpenFolder("android"), true);
  assert.equal(canOpenFolder("desktop"), false);
});

test("the button shows next to a save that worked, on Android only", () => {
  assert.equal(folderButtonVisible("android", "videos"), true);
  assert.equal(folderButtonVisible("android", "sounds"), true);
  assert.equal(folderButtonVisible("android", null), false);
  assert.equal(folderButtonVisible("desktop", "videos"), false);
});
