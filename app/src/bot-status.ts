// What the app says and offers about the bot, from the saved connection and its last answer.
import type { BotConnectionUi, BotLinkMethod, BotStatusUi } from "./contract-bot.ts";
import type { MediaUi, UiError } from "./contract.ts";
import type { InstalledExtension } from "./extensions.ts";
import type { Platform } from "./mobile.ts";
import type { Destination } from "./navigation.ts";

/** The extension whose chat the bot connection belongs to. */
export const BOT_EXTENSION_ID = "bot-stickers-sonido";

const LINKING_POLL_MS = 2_000;
const POLL_MS = 5_000;
// Slow enough to stay under the bot's limit of failed requests per minute.
const FAILED_POLL_MS = 10_000;

export interface BotState {
  /** `null` until the saved connection is read, or if reading it failed. */
  readonly connection: BotConnectionUi | null;
  /** The last answer of the bot; `null` before the first one and after a failure. */
  readonly status: BotStatusUi | null;
  readonly error: UiError | null;
}

export const INITIAL_BOT_STATE: BotState = { connection: null, status: null, error: null };

export type BotKind =
  | "off"
  | "checking"
  | "unreachable"
  | "unauthorized"
  | "error"
  | "unlinked"
  | "linking"
  | "offline"
  | "paused"
  | "active";

export interface BotDescription {
  readonly kind: BotKind;
  readonly text: string;
}

export function botEnabled(state: BotState): boolean {
  return state.connection?.enabled === true;
}

export function describeBot(state: BotState): BotDescription {
  const say = (kind: BotKind, text: string): BotDescription => ({ kind, text });
  const { status, error } = state;
  if (state.connection === null && error !== null) return say("error", error.message);
  if (!botEnabled(state)) return say("off", "Sin conexión con el bot.");
  if (error !== null) {
    if (error.kind === "bot_unreachable") {
      return say("unreachable", "No se puede llegar al bot. ¿Está Tailscale activo en los dos equipos? ¿Está el bot arrancado?");
    }
    if (error.kind === "bot_unauthorized") {
      return say(
        "unauthorized",
        "Token incorrecto. Pulsa «Deshabilitar», pega el contenido del archivo control-token del bot y vuelve a conectar.",
      );
    }
    return say("error", error.message);
  }
  if (status === null) return say("checking", "Consultando el bot…");
  switch (status.link) {
    case "unlinked":
      return say("unlinked", "El bot responde, pero su WhatsApp está sin vincular.");
    case "linking":
      return say("linking", "Vinculando el WhatsApp del bot…");
    case "linked":
      if (status.connection !== "connected") {
        const detail = status.connection === "connecting" ? "conectando con WhatsApp…" : "sin conexión con WhatsApp.";
        return say("offline", `Bot vinculado, ${detail}`);
      }
      return status.active
        ? say("active", "Bot conectado y activo: procesa lo que le llegue.")
        : say("paused", "Bot conectado y en pausa: no procesa ni responde.");
  }
}

/** The bot's own explanation, labelled as such. */
export function botNotice(state: BotState): string | null {
  const notice = state.status?.notice ?? null;
  return botEnabled(state) && notice !== null ? `Aviso del bot: ${notice}` : null;
}

export interface BotActions {
  readonly activate: boolean;
  readonly pause: boolean;
  readonly link: boolean;
  readonly unlink: boolean;
}

export function botActions(state: BotState): BotActions {
  const status = botEnabled(state) ? state.status : null;
  const online = status?.link === "linked" && status.connection === "connected";
  return {
    activate: online && status?.active === false,
    pause: status?.active === true,
    link: status?.link === "unlinked",
    // The bot refuses to unlink without a connection to WhatsApp.
    unlink: online,
  };
}

/** How long until the next question to the bot; `null` stops asking. */
export function pollDelayMs(state: BotState): number | null {
  if (!botEnabled(state)) return null;
  if (state.error !== null) {
    // Asking again with a rejected token only gets the app blocked: it waits for a new one.
    return state.error.kind === "bot_unauthorized" ? null : FAILED_POLL_MS;
  }
  const status = state.status;
  const changing = status === null || status.link === "linking" || status.connection === "connecting";
  return changing ? LINKING_POLL_MS : POLL_MS;
}

/** The bot extension, once its notice is accepted. */
export function acceptedBot(list: readonly InstalledExtension[]): InstalledExtension | null {
  return list.find((installed) => installed.extension.id === BOT_EXTENSION_ID && installed.accepted) ?? null;
}

export interface BotBox {
  readonly line: string;
  readonly panelLabel: string;
  /** The single «Enviar al bot» button; `null` without an enabled connection, when sending stays manual. */
  readonly send: { readonly reason: string | null } | null;
}

/** The bot lines of the editor, only for a video that becomes a sticker; `null` hides them. */
export function botBox(
  destination: Destination,
  media: MediaUi,
  extensions: readonly InstalledExtension[],
  state: BotState | null,
): BotBox | null {
  const bot = acceptedBot(extensions);
  if (state === null || bot === null || destination !== "sticker" || media.kind !== "video") return null;
  if (!botEnabled(state)) {
    return { line: describeBot(state).text, panelLabel: "Conectar en «Mi bot»", send: null };
  }
  // A bot that cannot be reached must not take away the manual send, which only needs WhatsApp.
  const reason = bot.number === null ? "Para enviar, guarda el número del bot en «Extensiones»." : null;
  const description = describeBot(state);
  const notice = botNotice(state);
  return {
    line: notice === null ? description.text : `${description.text} ${notice}`,
    panelLabel: description.kind === "unlinked" || description.kind === "linking" ? "Vincular el bot" : "Abrir «Mi bot»",
    send: replacesManualSend(state) ? { reason } : null,
  };
}

function replacesManualSend(state: BotState): boolean {
  return botEnabled(state) && state.error === null;
}

/** With the bot connected, «Enviar al bot» replaces the manual «Enviar a …» of its extension. */
export function manualTargets(targets: readonly InstalledExtension[], state: BotState | null): InstalledExtension[] {
  const replaced = state !== null && replacesManualSend(state);
  return targets.filter((installed) => !(replaced && installed.extension.id === BOT_EXTENSION_ID));
}

/** With an enabled connection the saved number cannot be changed. */
export function numberHeld(installed: InstalledExtension, state: BotState | null): boolean {
  return installed.extension.id === BOT_EXTENSION_ID && state !== null && botEnabled(state);
}

/** A phone cannot scan its own screen, so the QR it draws comes with what to do with it. */
export function qrNoteText(platform: Platform, qr: string | null): string | null {
  return platform === "android" && qr !== null
    ? "WhatsApp no puede escanear con la cámara la pantalla de este mismo celular: haz una captura y escanéala desde otra pantalla (el código QR caduca pronto), o vincula con código."
    : null;
}

export type PairingPhone = { ok: true; phone: string | null } | { ok: false };

/** The number to link by code, as typed. Empty leaves it to the bot's own configuration. */
export function pairingPhone(typed: string): PairingPhone {
  if (typed.trim() === "") return { ok: true, phone: null };
  const digits = typed.replace(/[\s+\-()]/g, "");
  return /^[0-9]{8,15}$/.test(digits) ? { ok: true, phone: digits } : { ok: false };
}

/**
 * On desktop, the first time the panel is on screen with nothing saved, the app looks for the bot
 * of this same computer instead of asking for a link. The saved connection must have been read.
 */
export function shouldConnectLocal(platform: Platform, state: BotState, panelVisible: boolean): boolean {
  return platform === "desktop" && panelVisible && state.connection !== null && state.connection.url === null;
}

/** The number field only makes sense while the bot can be linked. */
export function phoneFieldVisible(state: BotState): boolean {
  return botActions(state).link;
}

/**
 * Starts linking only with a valid request: the QR never sends a number, whatever is typed, and a
 * number that is not valid never reaches `link`.
 */
export function submitLink(
  method: BotLinkMethod,
  typedPhone: string,
  link: (method: BotLinkMethod, phone: string | null) => void,
  invalidPhone: () => void,
): void {
  const typed = pairingPhone(method === "code" ? typedPhone : "");
  if (typed.ok) link(method, typed.phone);
  else invalidPhone();
}

/** The question before saving a connection another app asked for. */
export function receivedLinkQuestion(url: string): string {
  return `¿Conectar con el bot en ${url}?`;
}

export interface ReceivedLink {
  /** The address of the link another app opened, or `null` if there is none. */
  receive(): Promise<string | null>;
  ask(question: string): Promise<boolean>;
  accept(): Promise<void>;
  reject(): Promise<void>;
  fail(error: unknown): void;
}

/** Nothing is saved unless the person confirms the address; a refusal drops the link. */
export async function offerReceivedLink(received: ReceivedLink): Promise<void> {
  try {
    const url = await received.receive();
    if (url === null) return;
    if (await received.ask(receivedLinkQuestion(url))) await received.accept();
    else await received.reject();
  } catch (error) {
    received.fail(error);
  }
}

/** Runs `task` one at a time; a call that arrives meanwhile makes it run once more afterwards. */
export function oneAtATime(task: () => Promise<void>): () => Promise<void> {
  let running = false;
  let again = false;
  return async () => {
    if (running) {
      again = true;
      return;
    }
    running = true;
    try {
      do {
        again = false;
        await task();
      } while (again);
    } finally {
      running = false;
    }
  };
}
