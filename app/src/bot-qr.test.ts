import assert from "node:assert/strict";
import { test } from "node:test";

import { qrDrawing } from "./bot-qr.ts";

test("the QR is a square of modules inside a quiet zone of four", () => {
  const { side, path } = qrDrawing("2@abcdefghijklmnop,qrstuvwxyz==,ABCDEF");
  // Every QR version has 17 + 4·version modules per side.
  assert.equal((side - 8 - 17) % 4, 0);
  // The top-left finder pattern starts right after the quiet zone.
  assert.ok(path.startsWith("M4 4h1v1h-1z"));
  const coordinates = [...path.matchAll(/M(\d+) (\d+)/g)].flatMap((match) => [Number(match[1]), Number(match[2])]);
  assert.ok(Math.min(...coordinates) === 4 && Math.max(...coordinates) === side - 5);
});

test("the longest QR text the app accepts can be drawn", () => {
  assert.ok(qrDrawing("a".repeat(1024)).side > 100);
});
