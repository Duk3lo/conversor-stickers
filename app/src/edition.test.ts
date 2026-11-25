import { test } from "node:test";
import assert from "node:assert/strict";
import { editionBadge, isGithubEdition } from "./edition.ts";

test("only the play edition turns off extensions and downloads", () => {
  assert.equal(isGithubEdition(undefined), true);
  assert.equal(isGithubEdition("github"), true);
  assert.equal(isGithubEdition("play"), false);
});

test("only the github edition shows a badge", () => {
  assert.equal(editionBadge(undefined), "GitHub");
  assert.equal(editionBadge("github"), "GitHub");
  assert.equal(editionBadge("play"), null);
});
