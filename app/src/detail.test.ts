import assert from "node:assert/strict";
import { test } from "node:test";

import type { LibraryUi, OriginUi, PackUi, StickerUi } from "./contract.ts";
import type { CommunityPackUi } from "./contract-community.ts";
import {
  communityBar,
  localBar,
  communityModel,
  localModel,
  lockReason,
  moveTo,
  moveOne,
  checkText,
  withdrawnUrl,
} from "./detail.ts";

function sticker(id: number): StickerUi {
  return { id, path: `/s/${id}.webp`, kind: "static", size_bytes: 1, emojis: ["🙂"], favorite: id === 2 };
}

const ok = [{ key: "count" as const, ok: true, text: "5 stickers" }];

function local(changes: Partial<PackUi> = {}): PackUi {
  return {
    id: 1,
    name: "Gatos",
    author: "Ana",
    category: null,
    kind: "static",
    stickers: [1, 2, 3].map(sticker),
    icon: "/i.png",
    exportable: true,
    favorite: false,
    modified: 0,
    published: null,
    origin: { kind: "own" },
    checks: ok,
    ...changes,
  };
}

function ofCommunity(version: number, withdrawn: string | null = null): OriginUi {
  return { kind: "community", id: "gatos", version, license: "CC0-1.0", original_author: "Eva", withdrawn };
}

function remote(changes: Partial<CommunityPackUi> = {}): CommunityPackUi {
  return {
    id: "gatos",
    version: 2,
    name: "Gatos",
    author: "Eva",
    license: "CC0-1.0",
    category: "animales",
    tags: [],
    animated: false,
    added: "2026-11-01",
    stickers: 8,
    icon: null,
    downloaded: null,
    updatable: false,
    ...changes,
  };
}

test("desktop: Export .wastickers, enabled if the pack meets the limits", () => {
  const bar = localBar(local(), "desktop");
  assert.deepEqual(bar.main, { key: "export", text: "Exportar .wastickers", active: true });
  assert.equal(bar.reason, null);
  assert.equal(bar.state, null);
});

test("the first failing check disables the action and gives the reason", () => {
  const failures = [
    { key: "count" as const, ok: false, text: "Faltan 2 stickers" },
    { key: "icon" as const, ok: false, text: "Falta el icono" },
  ];
  const bar = localBar(local({ exportable: false, checks: failures }), "android");
  assert.equal(bar.main.active, false);
  assert.equal(bar.reason, "Faltan 2 stickers");
});

test("Android: \"Añadir a WhatsApp\"; with published it says \"Añadido\"", () => {
  assert.deepEqual(localBar(local(), "android").main, { key: "add-whatsapp", text: "Añadir a WhatsApp", active: true });
  assert.equal(localBar(local({ published: 1 }), "android").state, "Añadido a WhatsApp");
});

test("Android: a community copy with a newer version than the published one asks for \"Actualizar en WhatsApp\"", () => {
  const bar = localBar(local({ origin: ofCommunity(3), published: 2 }), "android");
  assert.equal(bar.main.key, "update-whatsapp");
  assert.equal(bar.state, null);
  assert.equal(localBar(local({ origin: ofCommunity(2), published: 2 }), "android").main.key, "add-whatsapp");
});

test("a withdrawn pack blocks the action with the withdrawal reason, before other failures", () => {
  const pack = local({
    exportable: false,
    origin: ofCommunity(1, "Retirado por derechos de autor"),
    checks: [{ key: "count", ok: false, text: "Faltan stickers" }],
  });
  assert.equal(lockReason(pack), "Retirado por derechos de autor");
  const bar = localBar(pack, "android");
  assert.equal(bar.main.active, false);
  assert.equal(bar.reason, "Retirado por derechos de autor");
});

test("not exportable without checks gives a generic reason, never an enabled button without reason", () => {
  const bar = localBar(local({ exportable: false, checks: [] }), "desktop");
  assert.equal(bar.main.active, false);
  assert.ok(bar.reason !== null);
});

test("community pack not downloaded: \"Descargar\"; in progress, disabled", () => {
  assert.deepEqual(communityBar(remote(), null, false).main, { key: "download", text: "Descargar", active: true });
  const gear = communityBar(remote(), null, true);
  assert.deepEqual(gear.main, { key: "download", text: "Descargando…", active: false });
});

test("downloaded community pack: \"Abrir mi copia\"; if there is a new version, \"Actualizar (v2)\" and the separate copy", () => {
  const open = communityBar(remote({ downloaded: 4 }), local({ id: 4 }), false);
  assert.equal(open.main.key, "open-backup");
  assert.equal(open.state, "Descargado");
  assert.equal(open.secondary, null);
  const update = communityBar(remote({ downloaded: 4, updatable: true }), local({ id: 4 }), false);
  assert.deepEqual(update.main, { key: "update", text: "Actualizar (v2)", active: true });
  assert.equal(update.secondary?.key, "open-backup");
});

test("downloaded and withdrawn community pack: only open the copy, with the reason", () => {
  const bar = communityBar(remote({ downloaded: 4, updatable: true }), local({ id: 4, origin: ofCommunity(1, "Derechos") }), false);
  assert.equal(bar.main.key, "open-backup");
  assert.equal(bar.reason, "Derechos");
  assert.equal(bar.state, "Retirado del catálogo");
});

test("local model: original author and license of a community copy", () => {
  const model = localModel(local({ origin: ofCommunity(2) }), "desktop");
  assert.equal(model.author, "Eva");
  assert.equal(model.license, "CC0-1.0");
  assert.equal(model.version, "Versión 2");
  assert.equal(model.communityId, "gatos");
  assert.equal(model.stickers[1]?.favorite, true);
  const own = localModel(local(), "desktop");
  assert.deepEqual([own.license, own.version, own.communityId], [null, null, null]);
});

test("local model: the media origin goes to the sticker only if it is not own", () => {
  const model = localModel(local({ stickers: [sticker(1), { ...sticker(2), media_origin: "klipy" }] }), "desktop");
  assert.equal(model.stickers[0]?.mediaOrigin, undefined);
  assert.equal(model.stickers[1]?.mediaOrigin, "klipy");
});

test("not downloaded community model: no favorite or checks, stickers without id", () => {
  const model = communityModel({ pack: remote(), stickers: [{ path: "/a", emojis: ["🐱"] }] }, null, false);
  assert.equal(model.favorite, null);
  assert.deepEqual(model.checks, []);
  assert.equal(model.stickers[0]?.id, null);
  assert.equal(model.count, 8);
  assert.equal(model.version, "Versión 2");
});

test("downloaded community model: favorite of the copy and update notice", () => {
  const library: LibraryUi = { categories: [], packs: [local({ id: 4, favorite: true })], notice: null };
  const model = communityModel({ pack: remote({ downloaded: 4, updatable: true }), stickers: [] }, library, false);
  assert.equal(model.favorite, true);
  assert.equal(model.pack, 4);
  assert.equal(model.version, "Versión 2: hay una actualización");
});

test("checks are read with their state in words, not only with a symbol", () => {
  assert.equal(checkText({ key: "emojis", ok: true, text: "Todos tienen emoji" }), "Correcto: Todos tienen emoji");
  assert.equal(checkText({ key: "emojis", ok: false, text: "Hay stickers sin emoji" }), "Falta: Hay stickers sin emoji");
});

test("move a sticker one position; at the edge it does nothing", () => {
  assert.deepEqual(moveOne([1, 2, 3, 4], 1, -1), [2, 1, 3, 4]);
  assert.deepEqual(moveOne([1, 2, 3, 4], 1, 1), [1, 3, 2, 4]);
  assert.equal(moveOne([1, 2, 3], 0, -1), null);
  assert.equal(moveOne([1, 2, 3], 2, 1), null);
  assert.equal(moveOne([], 0, 1), null);
});

test("dragging leaves an exact permutation of the same ids", () => {
  const result = moveTo([10, 20, 30, 40], 0, 2);
  assert.deepEqual(result, [20, 30, 10, 40]);
  assert.deepEqual([...(result ?? [])].sort(), [10, 20, 30, 40]);
  assert.deepEqual(moveTo([10, 20, 30, 40], 3, 0), [40, 10, 20, 30]);
  assert.equal(moveTo([10, 20], 1, 1), null);
  assert.equal(moveTo([10, 20], 0, 5), null);
});

test("the withdrawal link encodes the pack id", () => {
  const url = withdrawnUrl("gatos-garabato");
  assert.ok(url.startsWith("https://github.com/"));
  assert.ok(url.includes("template=retirada.yml"));
  assert.ok(url.endsWith(encodeURIComponent("Retirada: gatos-garabato")));
});
