import assert from "node:assert/strict";
import { test } from "node:test";

import type { MediaUi } from "./contract.ts";
import {
  classifyPasted,
  destinationOfPasted,
  linksOfText,
  PASTED_LIMIT_BYTES,
  pastedMessage,
  type Clipboard,
  pasteableKind,
} from "./paste.ts";

function file(kind: string, bytes: number) {
  return { type: kind, size: bytes };
}

function clipboard(
  files: { type: string; size: number }[],
  texts: Record<string, string> = {},
): Clipboard {
  const elements = [
    ...files.map((blob) => ({ kind: "file", type: blob.type, getAsFile: () => blob })),
    ...Object.keys(texts).map((kind) => ({ kind: "string", type: kind, getAsFile: () => null })),
  ];
  return {
    items: elements,
    types: [...(files.length > 0 ? ["Files"] : []), ...Object.keys(texts)],
    getData: (kind) => texts[kind] ?? "",
  };
}

test("a pasted screenshot is a file that can be sent", () => {
  const capture = file("image/png", 300_000);
  assert.deepEqual(classifyPasted(clipboard([capture])), { kind: "file", file: capture, mime: "image/png" });
});

test("the seven kinds are valid", () => {
  for (const mime of [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "video/mp4",
    "video/webm",
    "video/quicktime",
  ]) {
    assert.equal(classifyPasted(clipboard([file(mime, 10)])).kind, "file", mime);
  }
});

test("the size is checked before reading: 50 MB is fine, one more is not, and empty neither", () => {
  assert.equal(classifyPasted(clipboard([file("video/mp4", PASTED_LIMIT_BYTES)])).kind, "file");
  assert.deepEqual(classifyPasted(clipboard([file("video/mp4", PASTED_LIMIT_BYTES + 1)])), {
    kind: "too-much-large",
    bytes: PASTED_LIMIT_BYTES + 1,
  });
  assert.deepEqual(classifyPasted(clipboard([file("image/png", 0)])), { kind: "nothing" });
});

test("a file of another kind is reported, not ignored", () => {
  assert.deepEqual(classifyPasted(clipboard([file("application/pdf", 10)])), {
    kind: "kind-not-allowed",
    mime: "application/pdf",
  });
});

test("of several files the first valid one is used", () => {
  const video = file("video/webm", 10);
  assert.deepEqual(classifyPasted(clipboard([file("text/plain", 3), video])), {
    kind: "file",
    file: video,
    mime: "video/webm",
  });
});

test("files copied in Dolphin or Nautilus (file:// paths) are not accepted", () => {
  assert.deepEqual(classifyPasted(clipboard([], { "text/uri-list": "file:///home/ana/gato.mp4\r\n" })), {
    kind: "files-of-team",
  });
  assert.deepEqual(
    classifyPasted(
      clipboard([], {
        "x-special/gnome-copied-files": "copy\nfile:///home/ana/gato.mp4",
        "text/plain": "/home/ana/gato.mp4",
      }),
    ),
    { kind: "files-of-team" },
  );
});

test("a link copied from the browser (uri-list with https) goes as a link", () => {
  assert.deepEqual(
    classifyPasted(
      clipboard([], { "text/uri-list": "https://www.tiktok.com/@a/video/1", "text/plain": "https://www.tiktok.com/@a/video/1" }),
    ),
    { kind: "links", text: "https://www.tiktok.com/@a/video/1" },
  );
});

test("a text with links keeps only the links, one per line", () => {
  const text = "mira esto https://youtu.be/abc\ny esto:\n  https://www.instagram.com/reel/x  \nfin";
  assert.deepEqual(classifyPasted(clipboard([], { "text/plain": text })), {
    kind: "links",
    text: "https://youtu.be/abc\nhttps://www.instagram.com/reel/x",
  });
});

test("a text without links or an empty clipboard is nothing", () => {
  assert.deepEqual(classifyPasted(clipboard([], { "text/plain": "hola" })), { kind: "nothing" });
  assert.deepEqual(classifyPasted(clipboard([])), { kind: "nothing" });
});

test("the links of the text: only http and https, without repeats", () => {
  assert.deepEqual(linksOfText("https://a.com/1 http://b.com/2 ftp://c.com https://a.com/1"), [
    "https://a.com/1",
    "http://b.com/2",
  ]);
  assert.deepEqual(linksOfText("nothing"), []);
});

test("the Paste button picks the first valid kind of what navigator.clipboard gives", () => {
  assert.equal(pasteableKind(["text/html", "image/png"]), "image/png");
  assert.equal(pasteableKind(["text/plain"]), null);
});

test("the notices explain what to do", () => {
  assert.equal(
    pastedMessage({ kind: "too-much-large", bytes: 62_000_000 }),
    "Lo pegado pesa 62 MB y el máximo es 50 MB. Guárdalo y elígelo con Imagen o Video.",
  );
  assert.equal(
    pastedMessage({ kind: "kind-not-allowed", mime: "application/pdf" }),
    "No se puede pegar un archivo application/pdf. Vale una imagen PNG, JPEG, WebP o GIF, o un video MP4, WebM o MOV.",
  );
  assert.equal(pastedMessage({ kind: "files-of-team" }), "Para archivos del equipo usa Imagen o Video.");
});

const image: MediaUi = { kind: "image", path: "/t/pegado-1.png", width: 800, height: 600 };
const video: MediaUi = { kind: "video", duration_s: 4, width: 640, height: 360, has_audio: true };

test("what is pasted in Stickers opens the sticker editor, whether image or video", () => {
  assert.deepEqual(destinationOfPasted("stickers", image), { kind: "editor", destination: "sticker" });
  assert.deepEqual(destinationOfPasted("stickers", video), { kind: "editor", destination: "sticker" });
});

test("in GIFs a video opens the editor; a still image goes to Download with a notice", () => {
  assert.deepEqual(destinationOfPasted("gifs", video), { kind: "editor", destination: "gif" });
  assert.deepEqual(destinationOfPasted("gifs", image), {
    kind: "ready",
    notice: "Una imagen fija no sirve para un GIF: la tienes en Descargar para hacer un sticker.",
  });
});

test("in Download everything goes into the list, and so does what failed", () => {
  assert.deepEqual(destinationOfPasted("download", video), { kind: "ready", notice: null });
  assert.deepEqual(destinationOfPasted("download", image), { kind: "ready", notice: null });
  assert.deepEqual(destinationOfPasted("stickers", null), {
    kind: "ready",
    notice: "Lo pegado no se pudo abrir: el motivo está en Descargar.",
  });
});

test("in GIF search what is pasted goes into the Download list", () => {
  assert.deepEqual(destinationOfPasted("search-gif", video), { kind: "ready", notice: null });
});
