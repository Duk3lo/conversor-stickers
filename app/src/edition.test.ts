import { test } from "node:test";
import assert from "node:assert/strict";
import { isGithubEdition } from "./edition.ts";

test("only the play edition turns off extensions and downloads", () => {
  assert.equal(isGithubEdition(undefined), true);
  assert.equal(isGithubEdition("github"), true);
  assert.equal(isGithubEdition("play"), false);
});
