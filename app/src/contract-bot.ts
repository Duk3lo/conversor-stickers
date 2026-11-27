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
  /** Connects with the link the bot shows; Rust checks it and the token never comes back. */
  connectLink(link: string): Promise<BotConnectionUi>;
  /** Android: the link another app opened, kept in Rust until confirmed. Resolves its address, or `null`. */
  receiveLink(): Promise<string | null>;
  /** Saves the link `receiveLink` returned the address of. */
  acceptLink(): Promise<BotConnectionUi>;
  rejectLink(): Promise<void>;
  disable(): Promise<BotConnectionUi>;
  status(): Promise<BotStatusUi>;
  activate(): Promise<BotStatusUi>;
  pause(): Promise<BotStatusUi>;
  /** `phone` (digits only) is the WhatsApp to link by code; `null` uses the bot's configured one. */
  link(method: BotLinkMethod, phone: string | null): Promise<BotStatusUi>;
  unlink(): Promise<BotStatusUi>;
  /** Opens `https://wa.me/<number>` outside the app (desktop and Android). */
  openChat(number: string): Promise<void>;
}

export const botApi: BotApi = {
  connection: () => call("bot_connection"),
  connect: (url, token) => call("bot_connect", { url, token }),
  connectLink: (link) => call("bot_connect_link", { link }),
  receiveLink: () => call("bot_receive_link"),
  acceptLink: () => call("bot_accept_link"),
  rejectLink: () => call("bot_reject_link"),
  disable: () => call("bot_disable"),
  status: () => call("bot_status"),
  activate: () => call("bot_activate"),
  pause: () => call("bot_pause"),
  link: (method, phone) => call("bot_link", { method, phone }),
  unlink: () => call("bot_unlink"),
  openChat: (number) => call("bot_open_chat", { number }),
};
