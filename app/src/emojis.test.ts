import assert from "node:assert/strict";
import { test } from "node:test";

import { toggleEmoji, searchEmojis, withRecent, Groups, saveRecent, readRecent, RECENT_MAX } from "./emojis.ts";

test("there are about 300 emojis in six groups, all with words", () => {
  assert.deepEqual(Groups.map((group) => group.id), ["caras", "gestos", "animales", "comida", "objetos", "simbolos"]);
  const all = Groups.flatMap((group) => group.emojis);
  assert.ok(all.length >= 300, `solo hay ${all.length}`);
  assert.ok(all.every((entry) => entry.emoji !== "" && entry.words !== ""));
});

test("search by Spanish word, with or without accents and case", () => {
  assert.ok(searchEmojis("gato").some((entry) => entry.emoji === "🐱"));
  assert.ok(searchEmojis("CORAZÓN").some((entry) => entry.emoji === "❤️"));
  assert.ok(searchEmojis("corazon").some((entry) => entry.emoji === "❤️"));
  assert.ok(searchEmojis("dormido").some((entry) => entry.emoji === "😴"));
});

test("several words must all match", () => {
  const result = searchEmojis("corazón rojo").map((entry) => entry.emoji);
  assert.ok(result.includes("❤️"));
  assert.ok(!result.includes("💙"));
});

test("an empty search or one without matches returns nothing, and there are no repeats", () => {
  assert.deepEqual(searchEmojis("   "), []);
  assert.deepEqual(searchEmojis("zzzzqqq"), []);
  const emojis = searchEmojis("luna").map((entry) => entry.emoji);
  assert.equal(new Set(emojis).size, emojis.length);
});

test("the emoji itself can be pasted into the search", () => {
  assert.deepEqual(searchEmojis("🐱").map((entry) => entry.emoji).slice(0, 1), ["🐱"]);
});

test("the recents put the latest first, without repeats and capped at 16", () => {
  assert.deepEqual(withRecent(["🐱", "😂"], "😂"), ["😂", "🐱"]);
  const many = Array.from({ length: 20 }, (_, i) => String(i));
  const list = withRecent(many, "🔥");
  assert.equal(list.length, RECENT_MAX);
  assert.equal(list[0], "🔥");
});

test("the recents are saved and read, and a broken store breaks nothing", () => {
  const data = new Map<string, string>();
  const store = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => void data.set(key, value) };
  assert.deepEqual(readRecent(store), []);
  assert.equal(saveRecent(store, ["🐱", "😂"]), true);
  assert.deepEqual(readRecent(store), ["🐱", "😂"]);
  data.set("emojis-recent", "{esto no es json");
  assert.deepEqual(readRecent(store), []);
  data.set("emojis-recent", JSON.stringify([1, "🐱", null, ""]));
  assert.deepEqual(readRecent(store), ["🐱"]);
  const broken = { getItem: () => { throw new Error("sin acceso"); }, setItem: () => { throw new Error("cuota"); } };
  assert.deepEqual(readRecent(broken), []);
  assert.equal(saveRecent(broken, ["🐱"]), false);
  assert.deepEqual(readRecent(null), []);
  assert.equal(saveRecent(null, ["🐱"]), false);
});

test("a sticker carries 1 to 3 emojis", () => {
  assert.deepEqual(toggleEmoji(["🐱"], "😂"), { emojis: ["🐱", "😂"], notice: null });
  assert.deepEqual(toggleEmoji(["🐱", "😂"], "🐱"), { emojis: ["😂"], notice: null });
  assert.equal(toggleEmoji(["🐱"], "🐱").emojis.length, 1);
  assert.ok(toggleEmoji(["🐱"], "🐱").notice?.includes("al menos"));
  const full = toggleEmoji(["🐱", "😂", "👍"], "🔥");
  assert.deepEqual(full.emojis, ["🐱", "😂", "👍"]);
  assert.ok(full.notice?.includes("como mucho 3"));
});
