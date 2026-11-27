import { DEFAULT_FRAMING } from "./framing.ts";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import type { ExtensionUi } from "./contract.ts";
import {
  buildNumber,
  canRemoveExtension,
  builtinExtension,
  changeExtension,
  countryOptions,
  defaultCountry,
  extensionCaption,
  installExtension,
  loadExtensions,
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

function digits(text: string, country: Parameters<typeof buildNumber>[1]): string {
  const result = buildNumber(text, country);
  assert.equal(result.kind, "number", `${country} ${text}: ${JSON.stringify(result)}`);
  return result.kind === "number" ? result.digits : "";
}

test("an empty chat number is no number", () => {
  assert.deepEqual(buildNumber("  ", "ES"), { kind: "none" });
});

test("spaces, hyphens and parentheses are accepted", () => {
  assert.equal(digits(" (612) 34-56-78 ", "ES"), "34612345678");
  assert.equal(digits("+34 (612) 345-678", null), "34612345678");
});

test("any other character is an error, and + only goes first", () => {
  for (const text of ["612.345.678", "612345678a", "612/345678", "(+34) 612345678", "34+612345678", "++34612345678", "612345678 ext. 2"]) {
    assert.equal(buildNumber(text, "ES").kind, "invalid", text);
  }
});

test("a leading + is international and wins over the chosen country", () => {
  assert.equal(digits("+44 7400 123456", "ES"), "447400123456");
  assert.equal(digits("+44 (0)7400 123456", null), "447400123456");
  assert.equal(digits("+54 9 11 2345-6789", "MX"), "5491123456789");
  // WhatsApp's old Mexican form: the library calls it impossible, but what the user wrote with + is kept.
  assert.equal(digits("+52 1 55 1234 5678", null), "5215512345678");
});

test("an unknown country code or a length outside 8 to 15 digits is an error", () => {
  for (const text of ["+999 123456789", "+", "+34", "+34 12", "+34 12345678901234"]) {
    assert.equal(buildNumber(text, null).kind, "invalid", text);
  }
});

test("without + a country is needed", () => {
  const result = buildNumber("612 345 678", null);
  assert.equal(result.kind, "invalid");
  assert.ok(result.kind === "invalid" && result.message.startsWith("Elige un país"));
});

test("the national leading zero depends on the country", () => {
  assert.equal(digits("612 345 678", "ES"), "34612345678");
  assert.equal(digits("06 12345678", "IT"), "390612345678");
  assert.equal(digits("312 345 6789", "IT"), "393123456789");
  assert.equal(digits("07400 123456", "GB"), "447400123456");
});

test("Argentina: the 0 and the 15 become the mobile 9; without the 15 no 9 is guessed", () => {
  assert.equal(digits("011 15-2345-6789", "AR"), "5491123456789");
  assert.equal(digits("0351 15 234 5678", "AR"), "5493512345678");
  assert.equal(digits("9 11 2345 6789", "AR"), "5491123456789");
  assert.equal(digits("11 2345-6789", "AR"), "541123456789");
});

test("Mexico: ten digits work; the retired 044, 045, 01 and 1 prefixes are errors", () => {
  assert.equal(digits("55 1234 5678", "MX"), "525512345678");
  for (const text of ["044 55 1234 5678", "045 55 1234 5678", "01 55 1234 5678", "1 55 1234 5678"]) {
    assert.equal(buildNumber(text, "MX").kind, "invalid", text);
  }
});

test("countries are listed with Spanish names, sorted", () => {
  const options = countryOptions();
  assert.ok(options.some((option) => option.code === "ES" && option.label === "España (+34)"));
  assert.ok(options.some((option) => option.code === "GB" && option.label === "Reino Unido (+44)"));
  const labels = options.map((option) => option.label);
  assert.deepEqual(labels, [...labels].sort(new Intl.Collator("es").compare));
});

test("the default country comes only from an explicit region of the language", () => {
  assert.equal(defaultCountry("es-ES"), "ES");
  assert.equal(defaultCountry("es-AR"), "AR");
  for (const language of ["es", "es-419", "", "no válido"]) {
    assert.equal(defaultCountry(language), null, language);
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

test("removing a third-party extension leaves the others", () => {
  const other = { ...bot, id: "otra" };
  const list = installExtension(installExtension([], bot), other);
  assert.deepEqual(removeExtension(list, other.id).map((installed) => installed.extension.id), [bot.id]);
  assert.equal(canRemoveExtension(other.id), true);
});

test("the built-in bot extension cannot be removed", () => {
  const other = { ...bot, id: "otra" };
  const list = installExtension(installExtension([], builtinExtension), other);
  assert.equal(canRemoveExtension(builtinExtension.id), false);
  assert.deepEqual(removeExtension(list, builtinExtension.id), list);
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
  const onlyBuiltin = [{ extension: builtinExtension, accepted: false, number: null }];
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": "no es json" })), onlyBuiltin);
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": '{"a":1}' })), onlyBuiltin);
  assert.deepEqual(loadExtensions(memoryStore({ "conversor.extensions": '[{"extension":{"id":1}}]' })), onlyBuiltin);
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

test("the built-in extension starts unaccepted and without number", () => {
  const store = memoryStore();
  const initial = loadExtensions(store);
  assert.deepEqual(initial, [{ extension: builtinExtension, accepted: false, number: null }]);
  assert.deepEqual(sendTargets(initial, "android", saved), []);
  // A list saved without it (removed with an older version) does not hide it.
  saveExtensions(store, []);
  assert.deepEqual(loadExtensions(store), initial);
  const other = { ...bot, id: "otra" };
  saveExtensions(store, installExtension([], other));
  assert.deepEqual(loadExtensions(store).map((installed) => installed.extension.id), [builtinExtension.id, "otra"]);
  const kept = changeExtension(initial, builtinExtension.id, { accepted: true, number: "34612345678" });
  saveExtensions(store, kept);
  assert.deepEqual(loadExtensions(store), kept);
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

test("Argentina: a mobile written with 15 but without the area code is rejected", () => {
  assert.equal(buildNumber("15 2345-6789", "AR").kind, "invalid");
  assert.equal(buildNumber("015 2345-6789", "AR").kind, "invalid");
});

test("the caption tells the built-in bot how the editor frames the video", () => {
  const bot = builtinExtension;
  assert.equal(extensionCaption(bot, 5, DEFAULT_FRAMING), "sonido=5 encuadre=recortar");
  assert.equal(
    extensionCaption(bot, 5, { ...DEFAULT_FRAMING, zoom: 1.5, center_x: 1 / 3, center_y: 0.5 }),
    "sonido=5 encuadre=recortar zoom=1.5 x=0.333",
  );
  const longest = extensionCaption(bot, 10, { ...DEFAULT_FRAMING, zoom: 3.95, center_x: 0.1234, center_y: 0.8766 }) ?? "";
  assert.ok(longest.length <= 80 && /^[a-z0-9=. ]+$/.test(longest), longest);
  // The whole video is what the bot does by default, so an older bot still understands the caption.
  assert.equal(extensionCaption(bot, 5, { ...DEFAULT_FRAMING, mode: "fit" }), "sonido=5");
  assert.equal(extensionCaption({ ...bot, id: "otra" }, 5, DEFAULT_FRAMING), "sonido=5");
});
