import assert from "node:assert/strict";
import { test } from "node:test";

import type { LibraryUi, PackUi, StickerUi } from "./contract.ts";
import type { CatalogUi, CommunityPackUi } from "./contract-community.ts";
import {
  currentChip,
  keyByPath,
  rowsWithChip,
  rowsByCategory,
  startModel,
  normalize,
  followEditing,
  communityCard,
  localCard,
  cards,
  cardsOfCategory,
} from "./catalog.ts";

function sticker(id: number): StickerUi {
  return { id, path: `/s/${id}.webp`, kind: "static", size_bytes: 1000, emojis: ["🙂"], favorite: false };
}

function local(id: number, changes: Partial<PackUi> = {}): PackUi {
  return {
    id,
    name: `Paquete ${id}`,
    author: "Ana",
    category: null,
    kind: "static",
    stickers: [1, 2, 3, 4, 5, 6].map(sticker),
    icon: "/i.png",
    exportable: true,
    favorite: false,
    modified: id,
    published: null,
    origin: { kind: "own" },
    checks: [],
    ...changes,
  };
}

function remote(id: string, changes: Partial<CommunityPackUi> = {}): CommunityPackUi {
  return {
    id,
    version: 1,
    name: id,
    author: "Luis",
    license: "CC0-1.0",
    category: "animales",
    tags: [],
    animated: false,
    added: "2026-11-01",
    stickers: 10,
    icon: null,
    downloaded: null,
    updatable: false,
    ...changes,
  };
}

const library: LibraryUi = {
  categories: [
    { id: 1, name: "Memes" },
    { id: 2, name: "Animales" },
  ],
  packs: [local(1, { category: 1 }), local(2, { category: 2 }), local(3)],
  notice: null,
};

const catalog: CatalogUi = {
  categories: [
    { id: "animales", name: "Animales" },
    { id: "comida", name: "Comida" },
  ],
  packs: [remote("gatos"), remote("tacos", { category: "comida" })],
  updated: 1_790_000_000,
  offline: false,
  notice_accepted: true,
};

test("normalize removes accents and case", () => {
  assert.equal(normalize("  Música Árabe "), "musica arabe");
});

test("an own card carries up to four samples and no marks", () => {
  const card = localCard(local(1, { category: 1 }), library.categories);
  assert.equal(card.samples.length, 4);
  assert.deepEqual(card.marks, []);
  assert.equal(card.category, "Memes");
  assert.equal(card.key, "local/1");
});

test("a withdrawn community copy says so and shows the original author", () => {
  const withdrawn = local(9, {
    origin: { kind: "community", id: "x", version: 1, license: "CC0-1.0", original_author: "Eva", withdrawn: "Derechos" },
  });
  const card = localCard(withdrawn, []);
  assert.deepEqual(card.marks, ["Comunidad", "Retirado"]);
  assert.equal(card.author, "Eva");
});

test("a community card marks downloaded and update", () => {
  const card = communityCard(remote("gatos", { downloaded: 4, updatable: true }), catalog.categories, null);
  assert.deepEqual(card.marks, ["Comunidad", "Descargado", "Actualización"]);
});

test("a downloaded pack appears once, like the catalog one, with its favorite", () => {
  const downloaded = local(7, { category: 2, favorite: true, origin: { kind: "community", id: "gatos", version: 1, license: "CC0-1.0", original_author: "Luis", withdrawn: null } });
  const all = cards(
    { ...library, packs: [...library.packs, downloaded] },
    { ...catalog, packs: [remote("gatos", { downloaded: 7 }), remote("tacos", { category: "comida" })] },
  );
  assert.deepEqual(all.map((t) => t.key), ["local/1", "local/2", "local/3", "community/gatos", "community/tacos"]);
  assert.equal(all.find((t) => t.key === "community/gatos")?.favorite, true);
});

test("without a catalog only those of the library are shown, including downloaded ones", () => {
  const downloaded = local(7, { origin: { kind: "community", id: "gatos", version: 1, license: "CC0-1.0", original_author: "Luis", withdrawn: null } });
  const all = cards({ ...library, packs: [downloaded] }, null);
  assert.deepEqual(all.map((t) => t.key), ["local/7"]);
});

test("the rows mix own and community packs with the same category name", () => {
  const rows = rowsByCategory(cards(library, catalog), ["memes", "animales"]);
  assert.deepEqual(rows.map((f) => f.key), ["memes", "animales", "comida", "without-category"]);
  const animals = rows.find((f) => f.key === "animales");
  assert.deepEqual(animals?.cards.map((t) => t.key), ["local/2", "community/gatos"]);
  assert.deepEqual(animals?.path, { kind: "category", origin: "local", id: "2" });
  assert.deepEqual(rows.at(-1)?.path, { kind: "category", origin: "local", id: "sin" });
});

test("without category always goes last and there are no empty rows", () => {
  const rows = rowsByCategory(cards({ ...library, packs: [local(3), local(1, { category: 1 })] }, null));
  assert.deepEqual(rows.map((f) => f.key), ["memes", "without-category"]);
  assert.deepEqual(rowsByCategory([]), []);
});

test("\"Seguir editando\": only own, last modified first, capped", () => {
  const many: LibraryUi = {
    ...library,
    packs: [
      ...Array.from({ length: 10 }, (_, i) => local(i + 1)),
      local(50, { modified: 9999, origin: { kind: "community", id: "x", version: 1, license: "CC0-1.0", original_author: "Z", withdrawn: null } }),
    ],
  };
  const list = followEditing(many);
  assert.equal(list.length, 8);
  assert.equal(list[0]?.id, "10");
  assert.ok(!list.some((t) => t.id === "50"));
  assert.deepEqual(followEditing(null), []);
});

test("\"Seguir editando\" breaks ties by name", () => {
  const b: LibraryUi = { ...library, packs: [local(1, { modified: 5, name: "Zeta" }), local(2, { modified: 5, name: "Alfa" })] };
  assert.deepEqual(followEditing(b).map((t) => t.name), ["Alfa", "Zeta"]);
});

test("the chip picks a row and one that no longer exists goes back to All", () => {
  const rows = rowsByCategory(cards(library, catalog));
  assert.equal(rowsWithChip(rows, null), rows);
  assert.deepEqual(rowsWithChip(rows, "comida").map((f) => f.key), ["comida"]);
  assert.equal(currentChip("comida", rows), "comida");
  assert.equal(currentChip("borrada", rows), null);
  assert.equal(currentChip(null, rows), null);
});

test("the category page resolves the name from any origin and gathers its cards", () => {
  assert.deepEqual(keyByPath("local", "2", library, catalog), { key: "animales", name: "Animales" });
  assert.deepEqual(keyByPath("community", "comida", library, catalog), { key: "comida", name: "Comida" });
  assert.deepEqual(keyByPath("local", "sin", library, null), { key: "without-category", name: "Sin categoría" });
  assert.equal(keyByPath("local", "99", library, catalog), null);
  const all = cards(library, catalog);
  assert.deepEqual(cardsOfCategory(all, "animales").map((t) => t.key), ["local/2", "community/gatos"]);
});

test("Home: \"Seguir editando\", chips and rows; the chip filters the rows", () => {
  const model = startModel(library, catalog, null);
  assert.equal(model.follow.length, 3);
  assert.deepEqual(model.chips.map((c) => c.key), ["memes", "animales", "comida", "without-category"]);
  assert.equal(model.rows.length, 4);
  const filtered = startModel(library, catalog, "comida");
  assert.deepEqual(filtered.rows.map((f) => f.key), ["comida"]);
  assert.equal(startModel(library, catalog, "borrada").chip, null);
});

test("Home without catalog and without packs has no rows", () => {
  const model = startModel({ categories: [], packs: [], notice: null }, null, null);
  assert.deepEqual([model.follow, model.rows, model.chips], [[], [], []]);
});
