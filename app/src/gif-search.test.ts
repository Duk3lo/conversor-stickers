import assert from "node:assert/strict";
import { test } from "node:test";

import {
  visibleAttribution,
  classifyError,
  createDelay,
  keyLink,
  providerName,
  queryText,
  joinPage,
  validateKey,
} from "./gif-search.ts";
import type { GifResultUi } from "./contract-gifs.ts";

const gif = (id: string): GifResultUi => ({ id, title: id, thumbnail: `/c/${id}.gif`, width: 10, height: 10 });

test("the search text is trimmed and empty means trending", () => {
  assert.equal(queryText("  gato  "), "gato");
  assert.equal(queryText("   "), null);
  assert.equal(queryText(""), null);
  assert.equal(queryText("a".repeat(150))?.length, 100);
});

test("Rust errors are classified by their message", () => {
  assert.equal(classifyError("Falta la clave de KLIPY"), "without-key");
  assert.equal(classifyError("La clave no es válida"), "key-invalid");
  assert.equal(classifyError("Límite de búsquedas: espera un minuto"), "limit");
  assert.equal(classifyError("Sin conexión"), "without-connection");
  assert.equal(classifyError("algo raro"), "other");
});

test("the key is validated before saving it", () => {
  assert.deepEqual(validateKey("  abc123  "), { ok: true, value: "abc123" });
  assert.equal(validateKey("   ").ok, false);
  assert.equal(validateKey("abc 123").ok, false);
  assert.equal(validateKey("x".repeat(201)).ok, false);
});

test("the attribution is the provider's even if Rust does not send it", () => {
  assert.equal(visibleAttribution("giphy", ""), "Powered by GIPHY");
  assert.equal(visibleAttribution("klipy", ""), "KLIPY");
  assert.equal(visibleAttribution("giphy", "Powered by GIPHY"), "Powered by GIPHY");
  assert.equal(visibleAttribution(null, ""), "");
});

test("each provider shows where to get the key", () => {
  assert.equal(keyLink("klipy"), "https://partner.klipy.com");
  assert.equal(keyLink("giphy"), "https://developers.giphy.com");
  assert.equal(providerName("klipy"), "KLIPY");
  assert.equal(providerName("giphy"), "GIPHY");
});

test("\"Cargar más\" adds the page without repeating ids", () => {
  const joined = joinPage([gif("a"), gif("b")], [gif("b"), gif("c")]);
  assert.deepEqual(joined.map((r) => r.id), ["a", "b", "c"]);
});

test("the delay only runs the last call and can be cancelled", () => {
  const pending = new Map<number, () => void>();
  let next = 1;
  const timer = {
    set: (action: () => void) => {
      pending.set(next, action);
      return next++;
    },
    remove: (id: number) => void pending.delete(id),
  };
  const calls: string[] = [];
  const delay = createDelay<string>((text) => calls.push(text), 300, timer);
  delay.request("g");
  delay.request("ga");
  assert.equal(pending.size, 1);
  [...pending.values()][0]?.();
  pending.clear();
  assert.deepEqual(calls, ["ga"]);
  delay.request("gat");
  delay.cancel();
  assert.equal(pending.size, 0);
});
