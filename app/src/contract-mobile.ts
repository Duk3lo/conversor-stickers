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

/** Shares the last saved clip to the bot's WhatsApp chat; `caption` is the settings text it reads. */
export function sendToBot(number: string, caption: string | null): Promise<void> {
  return call("send_to_bot", { number, caption });
}

/** Opens a web address in the system browser. Rust only accepts `https` addresses of known sites. */
export function openExternal(url: string): Promise<void> {
  return call("open_external", { url });
}

export function openWhatsapp(): Promise<void> {
  return call("open_whatsapp");
}
