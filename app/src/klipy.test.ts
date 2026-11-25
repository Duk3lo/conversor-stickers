import assert from "node:assert/strict";
import { test } from "node:test";

import type { PackUi, StickerUi } from "./contract.ts";
import type { KlipyCategoryUi, StickerKlipyUi } from "./contract-gifs.ts";
import {
  KLIPY_ATTRIBUTION,
  CATEGORIES_IN_COVER,
  packDestination,
  originLabel,
  splitTrending,
  visibleRow,
  initialRows,
  foreignOriginReason,
  keyOriginText,
  joinStickers,
} from "./klipy.ts";

function categories(howMany: number): KlipyCategoryUi[] {
  return Array.from({ length: howMany }, (_, i) => ({ id: `c${i}`, name: `Categoría ${i}` }));
}

function sticker(id: string): StickerKlipyUi {
  return { id, title: id, thumbnail: `/${id}.webp`, animated: true };
}

function stickerLocal(id: number, origin?: StickerUi["media_origin"]): StickerUi {
  const base: StickerUi = { id, path: `/s/${id}.webp`, kind: "static", size_bytes: 1, emojis: ["🙂"], favorite: false };
  return origin === undefined ? base : { ...base, media_origin: origin };
}

function pack(id: number, stickers: StickerUi[]): PackUi {
  return {
    id,
    name: `Paquete ${id}`,
    author: "Ana",
    category: null,
    kind: "static",
    stickers,
    icon: "/i.png",
    exportable: true,
    favorite: false,
    modified: 0,
    published: null,
    origin: { kind: "own" },
    checks: [],
  };
}

test("the cover starts with Trending and continues with the first categories", () => {
  const rows = initialRows(categories(CATEGORIES_IN_COVER + 4));
  assert.equal(rows.length, CATEGORIES_IN_COVER + 1);
  assert.deepEqual(rows[0], { key: "trending", title: "Tendencias", category: null, state: { kind: "loading" } });
  assert.deepEqual(rows[1]?.category, "c0");
  assert.equal(rows[1]?.title, "Categoría 0");
});

test("without categories there is still Trending", () => {
  assert.equal(initialRows([]).length, 1);
});

test("a row shows loading or results, and is hidden if it fails or comes back empty", () => {
  const base = { key: "x", title: "X", category: null };
  assert.equal(visibleRow({ ...base, state: { kind: "loading" } }), true);
  const list = { kind: "ready" as const, results: [sticker("a")], next: null, loadingPlus: false, plusError: null };
  assert.equal(visibleRow({ ...base, state: list }), true);
  assert.equal(visibleRow({ ...base, state: { ...list, results: [] } }), false);
  assert.equal(visibleRow({ ...base, state: { kind: "hidden" } }), false);
});

test("\"Ver más\" adds the next page without repeating stickers", () => {
  const joined = joinStickers([sticker("a"), sticker("b")], [sticker("b"), sticker("c")]);
  assert.deepEqual(joined.map((s) => s.id), ["a", "b", "c"]);
});

test("the media origin is named only if it is not own", () => {
  assert.equal(originLabel(undefined), null);
  assert.equal(originLabel("own"), null);
  assert.equal(originLabel("klipy"), "KLIPY");
  assert.equal(originLabel("giphy"), "GIPHY");
});

test("a pack with KLIPY or GIPHY stickers is not proposed and says why", () => {
  assert.equal(foreignOriginReason(pack(1, [stickerLocal(1), stickerLocal(2, "own")])), null);
  assert.equal(foreignOriginReason(pack(1, [stickerLocal(1, "klipy"), stickerLocal(2)])), "Contiene stickers de KLIPY.");
  assert.equal(foreignOriginReason(pack(1, [stickerLocal(1, "giphy"), stickerLocal(2, "klipy")])), "Contiene stickers de KLIPY y GIPHY.");
});

test("add to an existing pack or to a new one with a valid name", () => {
  const packs = [pack(7, [])];
  assert.deepEqual(packDestination("7", "", packs), { ok: true, pack: 7, newName: null });
  assert.deepEqual(packDestination("new", "  Gatos  ", packs), { ok: true, pack: null, newName: "Gatos" });
});

test("the destination rejects an empty name, too long, or a pack that no longer exists", () => {
  const packs = [pack(7, [])];
  assert.equal(packDestination("new", "   ", packs).ok, false);
  assert.equal(packDestination("new", "x".repeat(200), packs).ok, false);
  assert.equal(packDestination("9", "", packs).ok, false);
  assert.equal(packDestination("", "", packs).ok, false);
});

test("where the key comes from is stated in settings", () => {
  assert.equal(keyOriginText("bundled"), "Clave integrada");
  assert.equal(keyOriginText("user"), "Tu clave");
  assert.equal(keyOriginText("proxy"), "Servicio de la app");
  assert.equal(keyOriginText(null), null);
});

test("the KLIPY attribution is fixed", () => {
  assert.equal(KLIPY_ATTRIBUTION, "KLIPY");
});

test("Trending is separated so it can go first, keeping the order of the rest", () => {
  const rows = initialRows([{ id: "a", name: "A" }, { id: "b", name: "B" }] as unknown as KlipyCategoryUi[]);
  const reversed = [...rows].reverse();
  const { first, rest } = splitTrending(reversed);
  assert.deepEqual(first.map((row) => row.key), ["trending"]);
  assert.deepEqual(rest.map((row) => row.key), ["category-b", "category-a"]);
});
