import { type EntryUi, call } from "./contract.ts";
import type { SavedFolder } from "./saved-folder.ts";

export type AddResult = "added" | "cancelled";

export interface WhatsappOutcome {
  result: AddResult;
  notice: string | null;
}

export function addToWhatsapp(pack: number): Promise<WhatsappOutcome> {
  return call("add_to_whatsapp", { pack });
}

export function sendResultToWhatsapp(id: number): Promise<WhatsappOutcome> {
  return call("send_result_to_whatsapp", { id });
}

export function sendStickerToWhatsapp(pack: number, sticker: number): Promise<WhatsappOutcome> {
  return call("send_sticker_to_whatsapp", { pack, sticker });
}

export function pasteFromClipboard(): Promise<EntryUi> {
  return call("paste_from_clipboard");
}

export function previewPath(id: number): Promise<string> {
  return call("preview_path", { id });
}

export function shareSticker(pack: number, sticker: number): Promise<void> {
  return call("share_sticker", { pack, sticker });
}

export function shareResult(id: number): Promise<void> {
  return call("share_result", { id });
}

export function receiveShared(): Promise<EntryUi[]> {
  return call("receive_shared");
}

export function openSavedFolder(kind: SavedFolder): Promise<void> {
  return call("open_saved_folder", { kind });
}
