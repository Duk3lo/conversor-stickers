// «Mi bot», only in the `github` edition. Mirrors `src-tauri/src/contract_bot.rs`. The bot's token
// goes to Rust once and never comes back.
import { call } from "./contract.ts";

export interface BotConnectionUi {
  url: string | null;
  has_token: boolean;
  enabled: boolean;
}

export interface BotStatusUi {
  link: "unlinked" | "linking" | "linked";
  connection: "disconnected" | "connecting" | "connected";
  active: boolean;
  /** Pairing code, only while linking. */
  code: string | null;
  /** Text to draw as a QR, only while linking. */
  qr: string | null;
  /** The bot's own words: untrusted, only ever shown as text. */
  notice: string | null;
}

export type BotLinkMethod = "code" | "qr";

export interface BotApi {
  connection(): Promise<BotConnectionUi>;
  /** A `null` token keeps the saved one, which only works for the same address. */
  connect(url: string, token: string | null): Promise<BotConnectionUi>;
  disable(): Promise<BotConnectionUi>;
  status(): Promise<BotStatusUi>;
  activate(): Promise<BotStatusUi>;
  pause(): Promise<BotStatusUi>;
  link(method: BotLinkMethod): Promise<BotStatusUi>;
  unlink(): Promise<BotStatusUi>;
  /** Desktop only: opens `https://wa.me/<number>` in the browser. */
  openChat(number: string): Promise<void>;
}

export const botApi: BotApi = {
  connection: () => call("bot_connection"),
  connect: (url, token) => call("bot_connect", { url, token }),
  disable: () => call("bot_disable"),
  status: () => call("bot_status"),
  activate: () => call("bot_activate"),
  pause: () => call("bot_pause"),
  link: (method) => call("bot_link", { method }),
  unlink: () => call("bot_unlink"),
  openChat: (number) => call("bot_open_chat", { number }),
};
