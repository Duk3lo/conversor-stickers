import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { canSendToWhatsapp, canShareResult, sentToWhatsappNotice, SHARE_AS_IMAGE_HINT, savedStickerNotice, stickerOptions } from "./share.ts";

test("sharing needs Android and a sticker in the library", () => {
  assert.deepEqual(stickerOptions("android", 1, 2), { share: true, save: true, whatsapp: true });
  assert.deepEqual(stickerOptions("desktop", 1, 2), { share: false, save: true, whatsapp: false });
  assert.deepEqual(stickerOptions("android", null, 2), { share: false, save: false, whatsapp: false });
  assert.deepEqual(stickerOptions("android", 1, null), { share: false, save: false, whatsapp: false });
});

test("only a sticker that fits can be shared from the editor, and only on Android", () => {
  assert.equal(canShareResult("android", "sticker", true), true);
  assert.equal(canShareResult("android", "sticker", false), false);
  assert.equal(canShareResult("android", "gif", true), false);
  assert.equal(canShareResult("desktop", "sticker", true), false);
});

test("the notice names the saved file, and there is none if the dialog was cancelled", () => {
  assert.equal(savedStickerNotice("Gatos-2.webp"), "Sticker guardado como Gatos-2.webp.");
  assert.equal(savedStickerNotice(null), null);
});

test("the share hint warns that WhatsApp receives a photo and the index.html button carries it", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.ok(html.includes(`id="share" class="button-secondary" title="${SHARE_AS_IMAGE_HINT}"`));
  assert.ok(html.includes("Compartir como imagen"));
});

test("the quick WhatsApp button follows the same rule as sharing the result", () => {
  assert.equal(canSendToWhatsapp("android", "sticker", true), true);
  assert.equal(canSendToWhatsapp("android", "sticker", false), false);
  assert.equal(canSendToWhatsapp("android", "gif", true), false);
  assert.equal(canSendToWhatsapp("desktop", "sticker", true), false);
});

test("the quick send notice says where to find the pack, or relays the cancellation", () => {
  assert.ok((sentToWhatsappNotice("added", null) ?? "").includes("Mis stickers rápidos"));
  assert.equal(sentToWhatsappNotice("cancelled", null), null);
  assert.equal(sentToWhatsappNotice("cancelled", "aviso"), "aviso");
});
