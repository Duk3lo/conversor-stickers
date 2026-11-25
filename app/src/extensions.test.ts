import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import type { ExtensionUi } from "./contract.ts";
import {
  builtinExtension,
  changeExtension,
  extensionCaption,
  installExtension,
  loadExtensions,
  parseNumber,
  removeExtension,
  saveExtensions,
  sendTargets,
  soundDurationNotice,
  stillImageNotice,
} from "./extensions.ts";

function memoryStore(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  };
}

const bot: ExtensionUi = {
  id: "bot-stickers-sonido",
  name: "Bot de stickers",
  description: "Envía el clip a un bot.",
  notice: "Cliente no oficial.",
  url: "https://example.com",
  sound: true,
  max_sound_s: 10,
};
const saved = { kind: "saved", name: "a" } as const;

test("the chat number is only digits, 8 to 15 of them, or empty", () => {
  assert.deepEqual(parseNumber("34612345678"), { kind: "number", digits: "34612345678" });
  assert.deepEqual(parseNumber(" 12345678 "), { kind: "number", digits: "12345678" });
  assert.deepEqual(parseNumber("  "), { kind: "none" });
  for (const text of ["1234567", "1234567890123456", "+34612345678", "34 612 345 678"]) {
    assert.equal(parseNumber(text).kind, "invalid", text);
  }
});

test("a new extension starts unaccepted and without number; reinstalling keeps the number", () => {
  let list = installExtension([], bot);
  assert.deepEqual(list, [{ extension: bot, accepted: false, number: null }]);
  list = changeExtension(list, bot.id, { accepted: true, number: "34612345678" });
  list = installExtension(list, { ...bot, notice: "Otro aviso." });
  assert.equal(list.length, 1);
  assert.deepEqual([list[0]?.extension.notice, list[0]?.accepted, list[0]?.number], ["Otro aviso.", false, "34612345678"]);
});

test("removing an extension leaves the others", () => {
  const other = { ...bot, id: "otra" };
  const list = installExtension(installExtension([], bot), other);
  assert.deepEqual(removeExtension(list, bot.id).map((installed) => installed.extension.id), ["otra"]);
});

test("the state survives saving and loading", () => {
  const store = memoryStore();
  assert.deepEqual(loadExtensions(store, []), []);
  const list = changeExtension(installExtension([], bot), bot.id, { accepted: true, number: "34612345678" });
  saveExtensions(store, list);
  assert.deepEqual(loadExtensions(store), list);
  assert.deepEqual(loadExtensions(null, []), []);
});

test("stored garbage is dropped and a bad number is forgotten", () => {
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": "no es json" })), []);
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": '{"a":1}' })), []);
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": '[{"extension":{"id":1}}]' })), []);
  const stored = JSON.stringify([{ extension: bot, accepted: true, number: "12" }]);
  assert.equal(loadExtensions(memoryStore({ "conversor.extensions": stored }))[0]?.number, null);
});

test("«Enviar a» needs Android, a saved clip, the accepted notice and a number", () => {
  const ready = changeExtension(installExtension([], bot), bot.id, { accepted: true, number: "34612345678" });
  assert.equal(sendTargets(ready, "android", saved).length, 1);
  assert.equal(sendTargets(ready, "desktop", saved).length, 0);
  assert.equal(sendTargets(ready, "android", { kind: "idle" }).length, 0);
  assert.equal(sendTargets(changeExtension(ready, bot.id, { accepted: false }), "android", saved).length, 0);
  assert.equal(sendTargets(changeExtension(ready, bot.id, { number: null }), "android", saved).length, 0);
});

test("the caption carries the whole seconds of sound only if the extension asks for it", () => {
  assert.equal(extensionCaption(bot, 5.9), "sonido=5");
  assert.equal(extensionCaption(bot, 0.4), null);
  assert.equal(extensionCaption(bot, 9999), "sonido=600");
  assert.equal(extensionCaption({ ...bot, sound: false }, 5), null);
});

test("the duration notice appears only above the declared limit", () => {
  assert.equal(soundDurationNotice(bot, 10), null);
  assert.equal(soundDurationNotice(bot, 10.9), null);
  assert.equal(soundDurationNotice(bot, 12.3), "Se enviarán 12 s de sonido a Bot de stickers, que declara un máximo de 10 s.");
  assert.equal(soundDurationNotice({ ...bot, max_sound_s: null }, 100), null);
  assert.equal(soundDurationNotice({ ...bot, sound: false }, 100), null);
});

test("the still image notice appears when the sound outlasts the 10 s sticker image", () => {
  assert.equal(stillImageNotice(bot, 10), null);
  assert.equal(stillImageNotice(bot, 10.9), null);
  assert.equal(
    stillImageNotice(bot, 25.4),
    "La imagen del sticker dura 10 s; el resto del sonido (15 s) sonará con la imagen quieta.",
  );
  assert.equal(stillImageNotice({ ...bot, sound: false }, 100), null);
});

test("the built-in extension starts unaccepted and without number, and removing it sticks", () => {
  const store = memoryStore();
  const initial = loadExtensions(store);
  assert.deepEqual(initial, [{ extension: builtinExtension, accepted: false, number: null }]);
  assert.deepEqual(sendTargets(initial, "android", saved), []);
  saveExtensions(store, removeExtension(initial, builtinExtension.id));
  assert.deepEqual(loadExtensions(store), []);
  assert.deepEqual(loadExtensions(memoryStore(), []), []);
});

test("the built-in extension mirrors its bundled manifest", () => {
  const manifest = JSON.parse(readFileSync(new URL("./builtin-extensions/bot-stickers-sonido.json", import.meta.url), "utf8")) as Record<string, any>;
  assert.deepEqual(builtinExtension, {
    id: manifest["id"],
    name: manifest["nombre"],
    description: manifest["descripcion"],
    notice: manifest["aviso"],
    url: manifest["url"],
    sound: manifest["pie"].sonido,
    max_sound_s: manifest["limites"].sonidoMaxSegundos,
  });
});
