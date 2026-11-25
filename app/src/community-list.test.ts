import assert from "node:assert/strict";
import { test } from "node:test";

import type { CatalogUi, CommunityPackUi } from "./contract-community.ts";
import {
  communityEntries,
  entryAction,
  featuredEntries,
  filterChips,
  filterEntries,
  packsCountText,
} from "./community-list.ts";

function pack(id: string, changes: Partial<CommunityPackUi> = {}): CommunityPackUi {
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

const catalog: CatalogUi = {
  categories: [
    { id: "animales", name: "Animales" },
    { id: "comida", name: "Comida" },
    { id: "vacia", name: "Vacía" },
  ],
  packs: [
    pack("gatos", { added: "2026-10-01", tags: ["Mininos"] }),
    pack("tacos", { category: "comida", animated: true, added: "2026-11-10", author: "Ánge" }),
    pack("arepas", { category: "comida", downloaded: 7, added: "2026-11-05" }),
    pack("zorros", { downloaded: 8, updatable: true, added: "2026-09-01" }),
  ],
  updated: 1_790_000_000,
  offline: false,
  notice_accepted: true,
};

const entries = communityEntries(catalog, null);
const names = (list: readonly { card: { name: string } }[]) => list.map((entry) => entry.card.name);
const all = { query: "", filter: "all", sort: "recent" } as const;

test("recent sorts by added date descending, A-Z by name", () => {
  assert.deepEqual(names(filterEntries(entries, all)), ["tacos", "arepas", "gatos", "zorros"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, sort: "az" })), ["arepas", "gatos", "tacos", "zorros"]);
});

test("filters by type, downloaded and category", () => {
  assert.deepEqual(names(filterEntries(entries, { ...all, filter: "animated" })), ["tacos"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, filter: "static" })), ["arepas", "gatos", "zorros"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, filter: "downloaded" })), ["arepas", "zorros"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, filter: "category:comida" })), ["tacos", "arepas"]);
});

test("search ignores accents and case and covers author, label and license", () => {
  assert.deepEqual(names(filterEntries(entries, { ...all, query: "ange" })), ["tacos"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, query: "MININOS" })), ["gatos"]);
  assert.equal(filterEntries(entries, { ...all, query: "cc0 luis" }).length, 3);
  assert.deepEqual(filterEntries(entries, { ...all, query: "nada" }), []);
});

test("search and filter combine", () => {
  assert.deepEqual(names(filterEntries(entries, { ...all, filter: "category:comida", query: "are" })), ["arepas"]);
});

test("chips: fixed filters plus only categories that have packs", () => {
  assert.deepEqual(
    filterChips(catalog).map((chip) => chip.key),
    ["all", "static", "animated", "downloaded", "category:animales", "category:comida"],
  );
});

test("featured are the newest and respect the limit", () => {
  assert.deepEqual(names(featuredEntries(entries, 2)), ["tacos", "arepas"]);
});

test("entry action: download, update or open the local copy", () => {
  const [gatos, , arepas, zorros] = entries;
  assert.deepEqual(entryAction(gatos!), { kind: "download", text: "Descargar", path: { kind: "pack", origin: "community", id: "gatos" } });
  assert.equal(entryAction(zorros!).kind, "update");
  assert.deepEqual(entryAction(arepas!).path, { kind: "pack", origin: "local", id: "7" });
  assert.equal(entryAction(arepas!).text, "Abrir");
});

test("withdrawn is detected from the local backup", () => {
  const library = {
    categories: [],
    notice: null,
    packs: [{ id: 7, name: "arepas", author: "", category: null, kind: "static", stickers: [], icon: null, exportable: true, favorite: false, modified: 1, published: null, origin: { kind: "community", original_author: "Luis", withdrawn: "2026-11-20" }, checks: [] }],
  };
  // @ts-expect-error the fixture only fills the fields the logic reads
  const withdrawn = communityEntries(catalog, library).filter((entry) => entry.withdrawn);
  assert.deepEqual(names(withdrawn), ["arepas"]);
});

test("pack count text", () => {
  assert.equal(packsCountText(1), "1 paquete");
  assert.equal(packsCountText(4), "4 paquetes");
});
