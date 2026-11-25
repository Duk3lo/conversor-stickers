import assert from "node:assert/strict";
import { test } from "node:test";

import type { PackUi, StickerUi } from "./contract.ts";
import {
  type BackAction,
  backAction,
  controls,
  whatsappState,
  destinationLabel,
  tabLabel,
  previewMode,
  playableNoVideoReason,
} from "./mobile.ts";
import { openEditor, openPack, goTo, type Navigation, initialNavigation, Roots } from "./navigation.ts";
import { exportableNoReason } from "./packs.ts";

function pack(howMany: number, changes: Partial<PackUi> = {}): PackUi {
  const stickers: StickerUi[] = Array.from({ length: howMany }, (_, i) => ({ id: i + 1, path: "/a.webp", kind: "static", size_bytes: 1, emojis: ["🙂"], favorite: false }));
  return { id: 1, name: "P", author: "", category: null, kind: "static", stickers, icon: "/a.webp", exportable: howMany >= 3, favorite: false, modified: 0, published: null, origin: { kind: "own" }, checks: [], ...changes };
}

function navDe(action: BackAction): Navigation {
  if (action.kind !== "navigate") throw new Error("se esperaba navegar");
  return action.nav;
}

test("the Create tabs", () => {
  assert.deepEqual(tabLabel("download"), { name: "Descargar", detail: "Enlaces y pegar" });
  assert.equal(tabLabel("stickers").name, "Sticker");
  assert.equal(tabLabel("gifs").name, "GIF");
  assert.equal(tabLabel("search-gif").name, "Buscar GIF");
});

test("Play edition: no link box, no save from links, same rest; the tab says only Pegar", () => {
  assert.deepEqual(controls("android", true), {
    links: false,
    saveVideo: false,
    ctrlV: false,
    addToWhatsapp: true,
    ffmpegCredits: true,
  });
  assert.equal(tabLabel("download", true).detail, "Pegar");
  assert.equal(tabLabel("download").detail, "Enlaces y pegar");
});

test("controls per platform: links on both, no Ctrl+V on Android, with WhatsApp and credits", () => {
  assert.deepEqual(controls("desktop"), { links: true, saveVideo: true, ctrlV: true, addToWhatsapp: false, ffmpegCredits: false });
  assert.deepEqual(controls("android"), { links: true, saveVideo: true, ctrlV: false, addToWhatsapp: true, ffmpegCredits: true });
});

test("WhatsApp: hidden on desktop and, on Android, with the Export reason", () => {
  assert.deepEqual(whatsappState("desktop", pack(5)), { visible: false, active: false, reason: null });
  assert.deepEqual(whatsappState("android", pack(5)), { visible: true, active: true, reason: null });
  for (const p of [pack(2), pack(5, { exportable: false, checks: [{ key: "kind", ok: false, text: "Mezcla tipos." }] }), pack(3, { exportable: false, icon: null })]) {
    const state = whatsappState("android", p);
    assert.equal(state.active, false);
    assert.equal(state.reason, exportableNoReason(p));
    assert.ok(state.reason !== null);
  }
});

test("the preview is per file only on Android", () => {
  assert.equal(previewMode("desktop"), "window-webm");
  assert.equal(previewMode("android"), "file");
  assert.ok(playableNoVideoReason("desktop").includes("gst-plugins-good"));
  assert.ok(!playableNoVideoReason("android").includes("gst"));
});

test("back button: closes the editor, then the pack, and at the root it exits", () => {
  const withPack = openPack(initialNavigation(), 4);
  const withEditor = openEditor(withPack, 9, "sticker");

  const first = backAction("android", withEditor);
  assert.deepEqual(first, { kind: "navigate", nav: withPack });
  const second = backAction("android", navDe(first));
  assert.deepEqual(second, { kind: "navigate", nav: { ...withPack, openedPack: null } });
  assert.deepEqual(backAction("android", navDe(second)), { kind: "exit" });
  assert.deepEqual(backAction("android", initialNavigation()), { kind: "exit" });
});

test("back button: from the detail goes back to the previous route and at Home it exits", () => {
  const detail = goTo(initialNavigation(), { kind: "pack", origin: "community", id: "gatos" });
  assert.deepEqual(navDe(backAction("android", detail)).path, { kind: "start" });
  assert.deepEqual(backAction("android", navDe(backAction("android", detail))), { kind: "exit" });
});

test("the labels of the five destinations; on Android the bar is shorter", () => {
  assert.deepEqual(Roots.map((root) => destinationLabel("desktop", root)), ["Inicio", "Comunidad", "Crear", "Favoritos", "Mis paquetes"]);
  assert.equal(destinationLabel("android", "mis-packs"), "Paquetes");
});

test("the back button does not exist on desktop", () => {
  assert.deepEqual(backAction("desktop", openEditor(initialNavigation(), 1, "gif")), { kind: "none" });
});
