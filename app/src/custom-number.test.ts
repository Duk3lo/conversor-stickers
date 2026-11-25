import assert from "node:assert/strict";
import { test } from "node:test";

import { validateFps, validateSide } from "./custom-number.ts";

const fps = { min: 5, max: 30 };
const side = { min: 64, max: 480 };

test("an empty field is not an error", () => {
  assert.deepEqual(validateFps("  ", fps), { kind: "empty" });
  assert.deepEqual(validateSide("", side), { kind: "empty" });
});

test("fps inside the range are accepted", () => {
  assert.deepEqual(validateFps("17", fps), { kind: "ok", value: 17 });
  assert.deepEqual(validateFps(" 30 ", fps), { kind: "ok", value: 30 });
});

test("fps outside the range or not whole numbers say why", () => {
  assert.deepEqual(validateFps("31", fps), { kind: "error", message: "Los fps deben estar entre 5 y 30." });
  assert.deepEqual(validateFps("4", fps), { kind: "error", message: "Los fps deben estar entre 5 y 30." });
  assert.equal(validateFps("12,5", fps).kind, "error");
  assert.equal(validateFps("abc", fps).kind, "error");
  assert.equal(validateFps("-3", fps).kind, "error");
});

test("the side is rounded to an even number", () => {
  assert.deepEqual(validateSide("301", side), { kind: "ok", value: 302 });
  assert.deepEqual(validateSide("300", side), { kind: "ok", value: 300 });
});

test("the side outside the range is rejected", () => {
  assert.deepEqual(validateSide("32", side), { kind: "error", message: "El tamaño debe estar entre 64 y 480 px." });
  assert.equal(validateSide("600", side).kind, "error");
  assert.equal(validateSide("x", side).kind, "error");
});
