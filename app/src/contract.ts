import { Channel, invoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";

export interface SideRangeUi {
  min: number;
  max: number;
  suggested: number[];
}

export interface PresetUi {
  name: string;
  title: string;
  extension: string;
  side: number;
  max_size_bytes: number;
  max_duration_s: number;
  /** Highest fps the destination accepts. */
  max_fps: number;
  /** Output side the person may type; null = exact canvas (e.g. WhatsApp 512 x 512). */
  side_range_px: SideRangeUi | null;
  fps_range: { min: number; max: number };
}

export type Origin = "file" | "link" | "pasted";

export type EntryState =
  | { kind: "pending" }
  | { kind: "downloading"; bytes: number; total: number | null }
  | { kind: "ready" }
  | { kind: "error"; error: UiError };

export type MediaUi =
  | { kind: "video"; duration_s: number; width: number; height: number; has_audio: boolean }
  | { kind: "image"; path: string; width: number; height: number };

export interface EntryUi {
  id: number;
  name: string;
  origin: Origin;
  state: EntryState;
  media: MediaUi | null;
}

export type QueueEvent = { kind: "change"; entry: EntryUi } | { kind: "finished" };

export type SaveVideoEvent = { kind: "downloading"; bytes: number; total: number | null } | { kind: "writing" };

export type MediaKind = "image" | "video";

export type PriorityUi = "sharpness" | "balance" | "smoothness" | "fit_anyway" | "fluid";

export type FramingModeUi = "crop" | "fit";

export type BackgroundUi =
  | { kind: "transparent" }
  | { kind: "color"; rgb: string }
  | { kind: "blur" };

export type ShapeUi = "square" | "rounded" | "circle";

export type AlignUi = "top" | "center" | "bottom";

export interface FramingSpecUi {
  mode: FramingModeUi;
  center_x: number;
  center_y: number;
  zoom: number;
  /** Fit mode only. */
  background: BackgroundUi;
  shape: ShapeUi;
  /** 0.5..1: the drawing takes this fraction of the canvas. */
  scale: number;
  /** Fit mode only: vertical placement. */
  align: AlignUi;
}

export interface CreateRequest {
  id: number;
  preset: string;
  start_s: number;
  seconds: number;
  speed: number;
  priority: PriorityUi;
  /** null = automatic: the priority picks the fps. */
  fps: number | null;
  framing: FramingSpecUi;
  /** Only for presets with `side_range_px`; null = the preset default. */
  side_px: number | null;
}

export interface StaticRequest {
  id: number;
  framing: FramingSpecUi;
}

export interface ImagesGifRequest {
  ids: number[];
  image_s: number;
  repeat: boolean;
}

export interface EstimateUi {
  estimated_size_bytes: number;
  max_size_bytes: number;
  fits: boolean;
  fps: number;
  max_seconds_s: number;
  detail_px: number;
  /** Longest input range (seconds of video) that fits the duration cap and the size. */
  max_range_s: number;
}

export interface LibraryUi {
  categories: CategoryUi[];
  packs: PackUi[];
  notice: string | null;
}

export interface CategoryUi {
  id: number;
  name: string;
}

export interface PackUi {
  id: number;
  name: string;
  author: string;
  category: number | null;
  kind: StickerKind | null;
  stickers: StickerUi[];
  icon: string | null;
  exportable: boolean;
  favorite: boolean;
  modified: number;
  published: number | null;
  origin: OriginUi;
  checks: CheckUi[];
}

export type OriginUi =
  | { kind: "own" }
  | { kind: "community"; id: string; version: number; license: string; original_author: string; withdrawn: string | null };

export interface CheckUi {
  key: "count" | "kind" | "emojis" | "icon" | "texts" | "withdrawn";
  ok: boolean;
  text: string;
}

export type MediaOrigin = "own" | "klipy" | "giphy";

export interface StickerUi {
  id: number;
  path: string;
  kind: StickerKind;
  size_bytes: number;
  emojis: string[];
  favorite: boolean;
  media_origin?: MediaOrigin;
}

export type FavoriteTarget = { kind: "pack"; id: number } | { kind: "sticker"; pack: number; id: number };

export interface CreatedPack {
  library: LibraryUi;
  pack: number;
}

export type StickerKind = "static" | "animated";

export interface PackEdit {
  name: string;
  author: string;
  category: number | null;
}

export interface ImportResult {
  library: LibraryUi;
  pack: number;
  notices: string[];
}

export type ProgressUi =
  | { kind: "decoding" }
  | { kind: "preview" }
  | { kind: "fitting_size"; attempt: number; max: number }
  | { kind: "searching_duration"; attempt: number; max: number };

export interface CreateResult {
  meets_limit: boolean;
  size_bytes: number;
  max_size_bytes: number;
  fps: number | null;
  duration_s: number | null;
  reductions: string[];
  notice: string | null;
}

export type UiErrorKind =
  | "link_not_valid"
  | "yt_dlp_not_found"
  | "failed_download"
  | "ffmpeg_not_found"
  | "invalid_entry"
  | "invalid_options"
  | "does_not_fit_even_sped_up"
  | "timed_out"
  | "cancelled"
  | "busy"
  | "other"
  | "invalid_pack"
  | "bot_unreachable"
  | "bot_unauthorized";

export interface UiError {
  kind: UiErrorKind;
  message: string;
}

export const PREVIEW_WINDOW_S = 120;

const ERROR_KINDS: readonly UiErrorKind[] = [
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
];

export function asErrorUi(failure: unknown): UiError {
  if (typeof failure === "object" && failure !== null && "kind" in failure && "message" in failure) {
    const { kind, message } = failure;
    if (typeof message === "string" && ERROR_KINDS.some((known) => known === kind)) {
      return { kind: kind as UiErrorKind, message };
    }
  }
  const message = failure instanceof Error ? failure.message : String(failure);
  return { kind: "other", message: message || "error desconocido" };
}

export async function call<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  try {
    return await invoke<T>(command, args, options);
  } catch (failure) {
    throw asErrorUi(failure);
  }
}

export function presets(): Promise<PresetUi[]> {
  return call("presets");
}

export function pickFromGallery(kind: MediaKind): Promise<EntryUi[]> {
  return call("pick_from_gallery", { kind });
}

export function paste(bytes: Uint8Array, mimeKind: string): Promise<EntryUi> {
  return call("paste", bytes, { headers: { "x-kind": mimeKind } });
}

export function addLinks(text: string): Promise<EntryUi[]> {
  return call("add_links", { text });
}

export function downloadQueue(channel: Channel<QueueEvent>): Promise<void> {
  return call("download_queue", { channel });
}

export function cancelDownloads(): Promise<void> {
  return call("cancel_downloads");
}

/** Saves the video with its audio; `null` if the person cancelled it. Else the name it was saved as. */
export function saveVideo(id: number, channel: Channel<SaveVideoEvent>): Promise<string | null> {
  return call("save_video", { id, channel });
}

/** Saves the audio of the clip as a sound for stickers; same answer as `saveVideo`. */
export function saveSound(
  id: number,
  startS: number,
  seconds: number,
  channel: Channel<SaveVideoEvent>,
): Promise<string | null> {
  return call("save_sound", { id, startS, seconds, channel });
}

/** Saves the clip, with its audio, as a video for WhatsApp to make a sticker from; same answer as `saveVideo`. */
export function saveClipVideo(
  id: number,
  startS: number,
  seconds: number,
  channel: Channel<SaveVideoEvent>,
): Promise<string | null> {
  return call("save_clip_video", { id, startS, seconds, channel });
}

export function removeEntry(id: number): Promise<void> {
  return call("remove_entry", { id });
}

export function preview(id: number, fromS: number): Promise<ArrayBuffer> {
  return call("preview", { id, fromS });
}

export function cancelPreview(): Promise<void> {
  return call("cancel_preview");
}

export function create(request: CreateRequest, channel: Channel<ProgressUi>): Promise<CreateResult> {
  return call("create", { request, channel });
}

export function cancelCreation(): Promise<void> {
  return call("cancel_creation");
}

/** Local paths of small frames evenly spaced over the whole video. */
export function timelineThumbnails(id: number, count: number): Promise<string[]> {
  return call("timeline_thumbnails", { id, count });
}

/** Local path of the frame at `atS` with the video's own aspect ratio (longer side up to 640 px). */
export function videoStill(id: number, atS: number): Promise<string> {
  return call("video_still", { id, atS });
}

export function estimate(request: CreateRequest): Promise<EstimateUi> {
  return call("estimate", { request });
}

export function createStatic(request: StaticRequest): Promise<CreateResult> {
  return call("create_static", { request });
}

/** Returns the size of the GIF; its bytes are read with `result(ids[0])`. */
export function createImagesGif(request: ImagesGifRequest): Promise<number> {
  return call("create_images_gif", { request });
}

export function result(id: number): Promise<ArrayBuffer> {
  return call("result", { id });
}

export function save(id: number): Promise<string | null> {
  return call("save", { id });
}

export function library(): Promise<LibraryUi> {
  return call("library");
}

export function createCategory(name: string): Promise<LibraryUi> {
  return call("create_category", { name });
}

export function renameCategory(id: number, name: string): Promise<LibraryUi> {
  return call("rename_category", { id, name });
}

export function deleteCategory(id: number): Promise<LibraryUi> {
  return call("delete_category", { id });
}

export function createPack(name: string, category: number | null): Promise<LibraryUi> {
  return call("create_pack", { name, category });
}

export function editPack(id: number, edit: PackEdit): Promise<LibraryUi> {
  return call("edit_pack", { id, edit });
}

export function deletePack(id: number): Promise<LibraryUi> {
  return call("delete_pack", { id });
}

export function addSticker(pack: number, entry: number): Promise<LibraryUi> {
  return call("add_sticker", { pack, entry });
}

export function removeSticker(pack: number, sticker: number): Promise<LibraryUi> {
  return call("remove_sticker", { pack, sticker });
}

export function useAsIcon(pack: number, sticker: number): Promise<LibraryUi> {
  return call("use_as_icon", { pack, sticker });
}

export function exportPack(id: number): Promise<string | null> {
  return call("export_pack", { id });
}

export function exportSticker(pack: number, sticker: number): Promise<string | null> {
  return call("export_sticker", { pack, sticker });
}

export function importWastickers(): Promise<ImportResult | null> {
  return call("import_wastickers");
}

export function setEmojis(pack: number, sticker: number, emojis: string[]): Promise<LibraryUi> {
  return call("set_emojis", { pack, sticker, emojis });
}

export function markFavorite(target: FavoriteTarget, favorite: boolean): Promise<LibraryUi> {
  return call("mark_favorite", { target, favorite });
}

export function reorderStickers(pack: number, order: number[]): Promise<LibraryUi> {
  return call("reorder_stickers", { pack, order });
}

export function createPackFromFavorites(name: string): Promise<CreatedPack> {
  return call("create_pack_from_favorites", { name });
}

export function exportBackup(): Promise<string | null> {
  return call("export_backup");
}

export function restoreBackup(): Promise<LibraryUi | null> {
  return call("restore_backup");
}

/** An installed extension, as Rust validated it from its manifest. */
export interface ExtensionUi {
  id: string;
  name: string;
  description: string;
  notice: string;
  url: string;
  /** Whether the caption carries `sonido=<seconds>`. */
  sound: boolean;
  max_sound_s: number | null;
}

/** Asks for a manifest file and answers it validated; `null` if the user cancels. */
export function installExtension(): Promise<ExtensionUi | null> {
  return call("install_extension");
}
