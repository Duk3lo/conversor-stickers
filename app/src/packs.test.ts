import assert from "node:assert/strict";
import { test } from "node:test";

import type { LibraryUi, PackUi, StickerUi, StickerKind } from "./contract.ts";
import {
  categoryForNew,
  filterPacks,
  currentFilter,
  exportableNoReason,
  compatiblePacks,
  countText,
  kindText,
  validateAuthor,
  validateCategoryName,
  validatePackName,
} from "./packs.ts";

function stickers(howMany: number, kind: StickerKind = "animated"): StickerUi[] {
  return Array.from({ length: howMany }, (_, index) => ({
    id: index + 1,
    path: `/b/${index}.webp`,
    kind,
    size_bytes: 1000,
    emojis: ["🙂"],
    favorite: false,
  }));
}

function pack(id: number, changes: Partial<PackUi> = {}): PackUi {
  return {
    id,
    name: `Paquete ${id}`,
    author: "",
    category: null,
    kind: null,
    stickers: [],
    icon: null,
    exportable: false,
    favorite: false,
    modified: 0,
    published: null,
    origin: { kind: "own" },
    checks: [],
    ...changes,
  };
}

const library: LibraryUi = {
  categories: [
    { id: 1, name: "Anime" },
    { id: 2, name: "Memes" },
  ],
  packs: [
    pack(10, { category: 1 }),
    pack(11, { category: 2 }),
    pack(12),
    pack(13, { category: 1 }),
  ],
  notice: null,
};

const ids = (packs: readonly PackUi[]) => packs.map((p) => p.id);

test("filter by category: all, without category or a specific one", () => {
  assert.deepEqual(ids(filterPacks(library, { kind: "all" })), [10, 11, 12, 13]);
  assert.deepEqual(ids(filterPacks(library, { kind: "without-category" })), [12]);
  assert.deepEqual(ids(filterPacks(library, { kind: "category", id: 1 })), [10, 13]);
  assert.deepEqual(ids(filterPacks(library, { kind: "category", id: 99 })), []);
});

test("a filter of a deleted category goes back to All", () => {
  assert.deepEqual(currentFilter({ kind: "category", id: 2 }, library), { kind: "category", id: 2 });
  assert.deepEqual(currentFilter({ kind: "category", id: 7 }, library), { kind: "all" });
  assert.deepEqual(currentFilter({ kind: "without-category" }, library), { kind: "without-category" });
});

test("a new pack goes into the category being viewed", () => {
  assert.equal(categoryForNew({ kind: "category", id: 2 }), 2);
  assert.equal(categoryForNew({ kind: "all" }), null);
  assert.equal(categoryForNew({ kind: "without-category" }), null);
});

test("the count is written out of 30", () => {
  assert.equal(countText(pack(1)), "0/30");
  assert.equal(countText(pack(1, { stickers: stickers(12) })), "12/30");
});

test("the pack kind is named; empty if it has no stickers yet", () => {
  assert.equal(kindText(null), "Vacío");
  assert.equal(kindText("animated"), "Animado");
  assert.equal(kindText("static"), "Estático");
});

test("reason not to export: Rust's if given, otherwise the missing one", () => {
  assert.equal(exportableNoReason(pack(1, { exportable: true, stickers: stickers(3) })), null);
  assert.equal(
    exportableNoReason(pack(1, { checks: [{ key: "count", ok: true, text: "3 stickers" }, { key: "icon", ok: false, text: "Falta el icono de la bandeja" }] })),
    "Falta el icono de la bandeja",
  );
  assert.equal(exportableNoReason(pack(1, { stickers: stickers(1) })), "Faltan 2 stickers: un paquete lleva de 3 a 30.");
  assert.equal(exportableNoReason(pack(1, { stickers: stickers(2) })), "Falta 1 sticker: un paquete lleva de 3 a 30.");
  assert.equal(exportableNoReason(pack(1, { stickers: stickers(5) })), "Falta el icono de la bandeja.");
});

test("adding a sticker only offers packs of its kind or empty ones, and with room", () => {
  const withKinds: LibraryUi = {
    categories: [],
    packs: [
      pack(1, { kind: "animated", stickers: stickers(4) }),
      pack(2, { kind: "static", stickers: stickers(4, "static") }),
      pack(3),
      pack(4, { kind: "animated", stickers: stickers(30) }),
    ],
    notice: null,
  };
  assert.deepEqual(compatiblePacks(withKinds, "animated"), [
    { id: 1, name: "Paquete 1" },
    { id: 3, name: "Paquete 3" },
  ]);
  assert.deepEqual(
    compatiblePacks(withKinds, "static").map((p) => p.id),
    [2, 3],
  );
});

test("pack name: trimmed, 1 to 128 characters and without control characters", () => {
  assert.deepEqual(validatePackName("  Gatos  "), { valid: true, value: "Gatos" });
  assert.deepEqual(validatePackName("   "), { valid: false, reason: "Escribe un nombre." });
  assert.deepEqual(validatePackName("a".repeat(128)), { valid: true, value: "a".repeat(128) });
  assert.deepEqual(validatePackName("a".repeat(129)), {
    valid: false,
    reason: "El nombre puede tener como mucho 128 caracteres.",
  });
  assert.equal(validatePackName("😺".repeat(128)).valid, true);
  assert.deepEqual(validatePackName("Ga\u0007tos"), {
    valid: false,
    reason: "El nombre no puede llevar caracteres de control.",
  });
});

test("author: may be empty, at most 128 characters", () => {
  assert.deepEqual(validateAuthor(" "), { valid: true, value: "" });
  assert.deepEqual(validateAuthor("Ana"), { valid: true, value: "Ana" });
  assert.equal(validateAuthor("a".repeat(129)).valid, false);
  assert.equal(validateAuthor("A\nna").valid, false);
});

test("category: 1 to 40 characters, unique ignoring case and at most 50", () => {
  assert.deepEqual(validateCategoryName(" Gatos ", library.categories, null), { valid: true, value: "Gatos" });
  assert.deepEqual(validateCategoryName("anime", library.categories, null), {
    valid: false,
    reason: "Ya hay una categoría «Anime».",
  });
  assert.equal(validateCategoryName("ANIME", library.categories, 1).valid, true);
  assert.equal(validateCategoryName("a".repeat(41), library.categories, null).valid, false);
  assert.equal(validateCategoryName("", library.categories, null).valid, false);
  const full = Array.from({ length: 50 }, (_, index) => ({ id: index, name: `c${index}` }));
  assert.deepEqual(validateCategoryName("Nueva", full, null), {
    valid: false,
    reason: "Ya tienes 50 categorías, el máximo.",
  });
  assert.equal(validateCategoryName("Otro nombre", full, 3).valid, true);
});
