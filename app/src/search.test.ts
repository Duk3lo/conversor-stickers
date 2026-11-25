import assert from "node:assert/strict";
import { test } from "node:test";

import {
  search,
  withSearch,
  saveHistory,
  readHistory,
  HISTORY_MAX,
  favoritesModel,
  withoutSearch,
} from "./search.ts";
import type { LibraryUi, PackUi, StickerUi } from "./contract.ts";
import type { CatalogUi, CommunityPackUi } from "./contract-community.ts";

function sticker(id: number, emojis: string[], favorite = false): StickerUi {
  return { id, path: `/s/${id}.webp`, kind: "static", size_bytes: 1, emojis, favorite };
}

function local(id: number, name: string, stickers: StickerUi[], changes: Partial<PackUi> = {}): PackUi {
  return {
    id,
    name,
    author: "Ana",
    category: null,
    kind: "static",
    stickers,
    icon: null,
    exportable: true,
    favorite: false,
    modified: 0,
    published: null,
    origin: { kind: "own" },
    checks: [],
    ...changes,
  };
}

function remote(id: string, name: string, changes: Partial<CommunityPackUi> = {}): CommunityPackUi {
  return {
    id,
    version: 1,
    name,
    author: "Luis",
    license: "CC0-1.0",
    category: "animales",
    tags: [],
    animated: false,
    added: "2026-11-01",
    stickers: 5,
    icon: null,
    downloaded: null,
    updatable: false,
    ...changes,
  };
}

const library: LibraryUi = {
  categories: [{ id: 1, name: "Memes" }],
  packs: [
    local(1, "Gatos que saltan", [sticker(1, ["🐱"]), sticker(2, ["😂"], true)], { category: 1 }),
    local(2, "Reacciones", [sticker(1, ["👍"]), sticker(2, ["🐱", "🔥"], true)], { author: "Marta", favorite: true }),
  ],
  notice: null,
};

const catalog: CatalogUi = {
  categories: [{ id: "animales", name: "Animales" }],
  packs: [
    remote("perros-pixel", "Perros de píxel", { tags: ["pixel", "cachorro"] }),
    remote("tacos", "Tacos felices", { author: "Inés", category: "comida" }),
  ],
  updated: 0,
  offline: false,
  notice_accepted: true,
};

test("search by name, ignoring accents and case", () => {
  assert.deepEqual(search("PIXEL", library, catalog).packs.map((t) => t.id), ["perros-pixel"]);
  assert.deepEqual(search("gatos", library, catalog).packs.map((t) => t.id), ["1"]);
});

test("search by author, by tag and by category", () => {
  assert.deepEqual(search("marta", library, catalog).packs.map((t) => t.id), ["2"]);
  assert.deepEqual(search("cachorro", library, catalog).packs.map((t) => t.id), ["perros-pixel"]);
  assert.deepEqual(search("memes", library, catalog).packs.map((t) => t.id), ["1"]);
});

test("search by emoji: packs and loose stickers", () => {
  const result = search("🐱", library, catalog);
  assert.deepEqual(result.packs.map((t) => t.id).sort(), ["1", "2"]);
  assert.deepEqual(result.stickers.map((s) => `${s.pack}/${s.id}`), ["1/1", "2/2"]);
});

test("the word \"gato\" finds the stickers with 🐱 by their Spanish name", () => {
  const result = search("gato", library, catalog);
  assert.ok(result.stickers.some((s) => s.emojis.includes("🐱")));
  assert.ok(result.packs.some((t) => t.id === "2"));
});

test("several words must all match; an empty one returns nothing", () => {
  assert.deepEqual(search("marta gatos", library, catalog).packs, []);
  assert.deepEqual(search("   ", library, catalog), { packs: [], stickers: [] });
  assert.deepEqual(search("zzzqqq", library, catalog), { packs: [], stickers: [] });
});

test("the name weighs more than the author: it comes first", () => {
  const b: LibraryUi = { ...library, packs: [local(1, "Otro", [sticker(1, ["🙂"])], { author: "Luna" }), local(2, "Luna llena", [sticker(1, ["🙂"])])] };
  assert.deepEqual(search("luna", b, null).packs.map((t) => t.id), ["2", "1"]);
});

test("search without catalog or library does not fail", () => {
  assert.deepEqual(search("gato", null, null), { packs: [], stickers: [] });
});

test("the history puts the latest first, without repeats ignoring case, and capped", () => {
  assert.deepEqual(withSearch(["perro", "gato"], "Gato"), ["Gato", "perro"]);
  assert.deepEqual(withSearch(["a"], "   "), ["a"]);
  assert.deepEqual(withSearch([], "  muchos   espacios "), ["muchos espacios"]);
  const full = Array.from({ length: HISTORY_MAX }, (_, i) => `b${i}`);
  const fresh = withSearch(full, "new");
  assert.equal(fresh.length, HISTORY_MAX);
  assert.equal(fresh[0], "new");
  assert.deepEqual(withoutSearch(["a", "b"], "a"), ["b"]);
});

test("the history is saved and read, and a broken store breaks nothing", () => {
  const data = new Map<string, string>();
  const store = { getItem: (c: string) => data.get(c) ?? null, setItem: (c: string, v: string) => void data.set(c, v) };
  assert.equal(saveHistory(store, ["gato", "perro"]), true);
  assert.deepEqual(readHistory(store), ["gato", "perro"]);
  data.set("searches-recent", "no es json");
  assert.deepEqual(readHistory(store), []);
  const broken = { getItem: () => { throw new Error("x"); }, setItem: () => { throw new Error("x"); } };
  assert.deepEqual(readHistory(broken), []);
  assert.equal(saveHistory(broken, ["a"]), false);
  assert.deepEqual(readHistory(null), []);
});

test("Favorites: marked packs and stickers, with the note of how many packs would result", () => {
  const model = favoritesModel(library, catalog);
  assert.deepEqual(model.packs.map((t) => t.id), ["2"]);
  assert.deepEqual(model.stickers.map((s) => `${s.pack}/${s.id}`), ["1/2", "2/2"]);
  assert.equal(model.neededPacks, 1);
  assert.ok(model.note?.includes("menos de 3"));
});

test("Favorites: 35 stickers split into 2 packs and 0 creates none", () => {
  const many = local(1, "Todos", Array.from({ length: 35 }, (_, i) => sticker(i + 1, ["🙂"], true)));
  const match = favoritesModel({ ...library, packs: [many] }, null);
  assert.equal(match.neededPacks, 2);
  assert.ok(match.note?.includes("2 paquetes"));
  const none = favoritesModel({ ...library, packs: [local(1, "Nada", [sticker(1, ["🙂"])])] }, null);
  assert.equal(none.neededPacks, 0);
  assert.equal(none.note, null);
});
