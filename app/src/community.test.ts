import assert from "node:assert/strict";
import { test } from "node:test";

import { COMMUNITY_NOTICE, Licenses, proposableNoReason, downloadPercentage, communityErrorText, validateProposal } from "./community.ts";
import type { PackUi, StickerUi } from "./contract.ts";

function sticker(id: number, kind: StickerUi["kind"] = "static"): StickerUi {
  return { id, path: `/s/${id}.webp`, kind, size_bytes: 1, emojis: ["🙂"], favorite: false };
}

function pack(changes: Partial<PackUi> = {}): PackUi {
  return {
    id: 1,
    name: "Gatos",
    author: "Ana",
    category: null,
    kind: "static",
    stickers: [1, 2, 3].map((id) => sticker(id)),
    icon: "/i.png",
    exportable: true,
    favorite: false,
    modified: 0,
    published: null,
    origin: { kind: "own" },
    checks: [],
    ...changes,
  };
}

test("the licenses are exactly the three in the plan", () => {
  assert.deepEqual(Licenses.map((license) => license.id), ["CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0"]);
});

test("the notice says it is uploaded by third parties and how to ask for withdrawal", () => {
  assert.ok(COMMUNITY_NOTICE.includes("otras personas"));
  assert.ok(COMMUNITY_NOTICE.includes("retirada"));
});

test("the download percentage comes from finished files and does not exceed 100", () => {
  assert.equal(downloadPercentage({ kind: "downloading", done: 3, total: 12 }), 25);
  assert.equal(downloadPercentage({ kind: "downloading", done: 13, total: 12 }), 100);
  assert.equal(downloadPercentage({ kind: "downloading", done: 0, total: 0 }), null);
  assert.equal(downloadPercentage({ kind: "verifying" }), null);
  assert.equal(downloadPercentage(null), null);
});

test("a complete own pack can be proposed", () => {
  assert.equal(proposableNoReason(pack()), null);
});

test("what does not meet the limits is not proposed: count, icon, author, mix or community copy", () => {
  assert.ok(proposableNoReason(pack({ stickers: [sticker(1)] }))?.includes("de 3 a 30"));
  assert.ok(proposableNoReason(pack({ stickers: Array.from({ length: 31 }, (_, i) => sticker(i)) }))?.includes("31"));
  assert.ok(proposableNoReason(pack({ icon: null }))?.includes("icono"));
  assert.ok(proposableNoReason(pack({ author: "  " }))?.includes("autor"));
  assert.ok(proposableNoReason(pack({ stickers: [sticker(1), sticker(2), sticker(3, "animated")] }))?.includes("mezcla"));
  const backup = pack({ origin: { kind: "community", id: "x", version: 1, license: "CC0-1.0", original_author: "Z", withdrawn: null } });
  assert.ok(proposableNoReason(backup)?.includes("ya es de la comunidad"));
});

test("a pack with KLIPY or GIPHY stickers is not proposed", () => {
  const foreign: StickerUi = { ...sticker(3), media_origin: "klipy" };
  assert.equal(proposableNoReason(pack({ stickers: [sticker(1), sticker(2), foreign] })), "Contiene stickers de KLIPY.");
});

test("the proposal asks for a license from the list and a category from the catalog", () => {
  const categories = [{ id: "animales" }, { id: "comida" }];
  assert.deepEqual(validateProposal("CC0-1.0", "animales", categories), { valid: true, license: "CC0-1.0", category: "animales" });
  assert.equal(validateProposal("MIT", "animales", categories).valid, false);
  assert.equal(validateProposal("", "animales", categories).valid, false);
  assert.equal(validateProposal("CC-BY-4.0", "otra", categories).valid, false);
  assert.equal(validateProposal("CC-BY-4.0", "animales", []).valid, false);
});

test("the community error is explained without showing the technical detail", () => {
  const text = communityErrorText("El catálogo respondió 404 a https://ejemplo.test/indice.json");
  assert.equal(text.community, "La comunidad aún no está disponible o no hay conexión.");
  assert.equal(text.start, "La comunidad no está disponible ahora.");
  assert.equal(text.detail, "El catálogo respondió 404 a https://ejemplo.test/indice.json");
  assert.ok(!text.community.includes("404"));
  assert.ok(!text.start.includes("404"));
});

test("without technical detail there is nothing to expand", () => {
  assert.equal(communityErrorText("   ").detail, null);
});
