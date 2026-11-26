import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  type LibraryUi,
  asErrorUi,
  type PackEdit,
  type FramingModeUi,
  type EntryUi,
  type EstimateUi,
  type UiError,
  type QueueEvent,
  type SaveVideoEvent,
  type FavoriteTarget,
  type Origin,
  type CreatedPack,
  type CreateRequest,
  type StaticRequest,
  type PresetUi,
  type PriorityUi,
  type ProgressUi,
  type CreateResult,
  type ImportResult,
  type UiErrorKind,
  type MediaKind,
  type StickerKind,
  PREVIEW_WINDOW_S,
} from "./contract.ts";

const errorUi: UiError = { kind: "failed_download", message: "yt-dlp falló (código 1)" };

const listEntry: EntryUi = {
  id: 1,
  name: "gato.mp4",
  origin: "file",
  state: { kind: "ready" },
  media: { kind: "video", duration_s: 12.5, width: 1920, height: 1080, has_audio: true },
};

const examples: {
  preview_window_s: number;
  preset_ui: PresetUi;
  preset_ui_free_side: PresetUi;
  ready_entry: EntryUi;
  image_entry: EntryUi;
  downloading_entry: EntryUi;
  pending_entry: EntryUi;
  error_entry: EntryUi;
  change_event: QueueEvent;
  finished_event: QueueEvent;
  save_video_downloading_event: SaveVideoEvent;
  save_video_writing_event: SaveVideoEvent;
  create_request: CreateRequest;
  origins: Origin[];
  static_request: StaticRequest;
  framings: FramingModeUi[];
  estimate_ui: EstimateUi;
  library_ui: LibraryUi;
  sticker_kinds: StickerKind[];
  favorite_targets: FavoriteTarget[];
  created_pack: CreatedPack;
  pack_edit: PackEdit;
  pack_edit_without_category: PackEdit;
  import_result: ImportResult;
  media_kinds: MediaKind[];
  priorities: PriorityUi[];
  progresses: ProgressUi[];
  create_result: CreateResult;
  static_result: CreateResult;
  error_ui: UiError;
  error_kinds: UiErrorKind[];
} = {
  preview_window_s: PREVIEW_WINDOW_S,
  preset_ui: {name: "whatsapp-sticker", title: "Sticker animado de WhatsApp", extension: "webp", side: 512, max_size_bytes: 500000, max_duration_s: 10.0, max_fps: 30, fps_range: {min: 5, max: 30}, side_range_px: null},
  preset_ui_free_side: {name: "discord-gif", title: "GIF para Discord", extension: "gif", side: 480, max_size_bytes: 10000000, max_duration_s: 30.0, max_fps: 30, fps_range: {min: 5, max: 30}, side_range_px: {min: 64, max: 480, suggested: [240, 320, 480]}},
  ready_entry: listEntry,
  image_entry: {
    id: 5,
    name: "gato.png",
    origin: "file",
    state: { kind: "ready" },
    media: { kind: "image", path: "/home/ana/Imágenes/gato.png", width: 800, height: 600 },
  },
  downloading_entry: {
    id: 2,
    name: "www.tiktok.com",
    origin: "link",
    state: { kind: "downloading", bytes: 1024, total: null },
    media: null,
  },
  pending_entry: {
    id: 3,
    name: "youtu.be",
    origin: "link",
    state: { kind: "pending" },
    media: null,
  },
  error_entry: {
    id: 4,
    name: "roto.mp4",
    origin: "file",
    state: { kind: "error", error: errorUi },
    media: null,
  },
  change_event: { kind: "change", entry: listEntry },
  finished_event: { kind: "finished" },
  save_video_downloading_event: { kind: "downloading", bytes: 1024, total: null },
  save_video_writing_event: { kind: "writing" },
  create_request: {id: 1, preset: "discord-gif", start_s: 3.2, seconds: 6.0, speed: 1.5, priority: "smoothness", fps: 24, framing: {mode: "fit", center_x: 0.25, center_y: 0.5, zoom: 2.0, background: {kind: "blur"}, shape: "circle", scale: 0.8, align: "top"}, side_px: 320},
  origins: ["file", "link", "pasted"],
  static_request: {id: 5, framing: {mode: "fit", center_x: 0.5, center_y: 0.5, zoom: 1.0, background: {kind: "color", rgb: "#ffcc00"}, shape: "rounded", scale: 1.0, align: "bottom"}},
  framings: ["crop", "fit"],
  estimate_ui: {estimated_size_bytes: 380000, max_size_bytes: 500000, fits: true, fps: 15, max_seconds_s: 6.5, detail_px: 384, max_range_s: 6.5},
  library_ui: {
    categories: [
      {
        id: 1,
        name: "Animales"
      }
    ],
    packs: [
      {
        id: 3,
        name: "Gatos",
        author: "Ana",
        category: 1,
        kind: "animated",
        stickers: [
          {
            id: 5,
            path: "/datos/packs/3/5.webp",
            kind: "animated",
            size_bytes: 412000,
            emojis: [
              "🐱"
            ],
            favorite: true,
            media_origin: "klipy"
          }
        ],
        icon: "/datos/packs/3/tray.png",
        exportable: false,
        checks: [
          {
            key: "count",
            ok: false,
            text: "necesita de 3 a 30 stickers"
          }
        ],
        favorite: true,
        modified: 1790000000,
        published: 2,
        origin: {
          kind: "community",
          id: "gatos-garabato",
          version: 2,
          license: "CC-BY-4.0",
          original_author: "Ana",
          withdrawn: null
        }
      },
      {
        id: 4,
        name: "Vacío",
        author: "",
        category: null,
        kind: null,
        stickers: [],
        icon: null,
        exportable: false,
        checks: [
          {
            key: "count",
            ok: false,
            text: "necesita de 3 a 30 stickers"
          }
        ],
        favorite: false,
        modified: 1790000100,
        published: null,
        origin: {
          kind: "own"
        }
      }
    ],
    notice: "se quitó un sticker sin archivo"
  },
  sticker_kinds: ["static", "animated"],
  favorite_targets: [
    { kind: "pack", id: 3 },
    { kind: "sticker", pack: 3, id: 5 },
  ],
  created_pack: {
    library: { categories: [], packs: [], notice: null },
    pack: 9,
  },
  pack_edit: { name: "Gatos", author: "Ana", category: 1 },
  pack_edit_without_category: { name: "Gatos", author: "", category: null },
  import_result: {
    library: { categories: [], packs: [], notice: null },
    pack: 7,
    notices: ["se omitió 1 sticker estático"],
  },
  media_kinds: ["image", "video"],
  priorities: ["sharpness", "balance", "smoothness", "fit_anyway", "fluid"],
  progresses: [
    { kind: "decoding" },
    { kind: "preview" },
    { kind: "fitting_size", attempt: 2, max: 8 },
    { kind: "searching_duration", attempt: 1, max: 4 },
  ],
  create_result: {
    meets_limit: false,
    size_bytes: 612000,
    max_size_bytes: 500000,
    fps: 10,
    duration_s: 6,
    reductions: ["fps 15→10"],
    notice: "Prueba con un tramo de 4,5 s",
  },
  static_result: {
    meets_limit: true,
    size_bytes: 84000,
    max_size_bytes: 100000,
    fps: null,
    duration_s: null,
    reductions: [],
    notice: null,
  },
  error_ui: errorUi,
  error_kinds: [
    "link_not_valid",
    "yt_dlp_not_found",
    "failed_download",
    "ffmpeg_not_found",
    "invalid_entry",
    "invalid_options",
    "does_not_fit_even_sped_up",
    "timed_out",
    "cancelled",
    "busy",
    "other",
    "invalid_pack",
    "bot_unreachable",
    "bot_unauthorized",
  ],
};

test("the typed examples match the JSON that Rust checks", () => {
  const json = JSON.parse(readFileSync(new URL("./contract.examples.json", import.meta.url), "utf8")) as Record<string, unknown>;
  assert.deepEqual(examples, json);
});

test("asErrorUi lets an UiError from Rust through", () => {
  assert.deepEqual(asErrorUi({ kind: "busy", message: "ya hay una conversión en curso" }), {
    kind: "busy",
    message: "ya hay una conversión en curso",
  });
});

test("asErrorUi knows all the error kinds", () => {
  for (const kind of examples.error_kinds) {
    assert.deepEqual(asErrorUi({ kind, message: "x" }), { kind, message: "x" });
  }
});

test("asErrorUi turns any other failure into one of kind other", () => {
  assert.deepEqual(asErrorUi(new Error("IPC caído")), { kind: "other", message: "IPC caído" });
  assert.deepEqual(asErrorUi("texto suelto"), { kind: "other", message: "texto suelto" });
  assert.deepEqual(asErrorUi({ kind: "inventado", message: "x" }).kind, "other");
  assert.deepEqual(asErrorUi(""), { kind: "other", message: "error desconocido" });
});
