import type { EntryUi, UiError } from "./contract.ts";

const SESSION_ASKS = /\blog ?in\b|\bsign in\b|\bcookies\b|authenticat|private video|is private/i;
const URL_IN_MESSAGE = /\b[a-z][a-z0-9+.-]*:\/\/\S+/gi;

export function errorMessage(error: UiError): string {
  switch (error.kind) {
    case "yt_dlp_not_found":
      return `Falta yt-dlp para descargar enlaces: ${error.message}`;
    case "failed_download":
      return SESSION_ASKS.test(error.message.replace(URL_IN_MESSAGE, ""))
        ? "Este video pide iniciar sesión, y la app no usa tus cookies. Descárgalo a mano y elige el archivo."
        : error.message;
    default:
      return error.message;
  }
}

export function pendingThereIs(entries: readonly Pick<EntryUi, "state">[]): boolean {
  return entries.some((entry) => entry.state.kind === "pending");
}
