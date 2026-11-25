import assert from "node:assert/strict";
import { test } from "node:test";

import { pendingThereIs, errorMessage } from "./messages.ts";

test("without yt-dlp it says what is missing besides the core message", () => {
  assert.equal(
    errorMessage({ kind: "yt_dlp_not_found", message: "no se encontró `yt-dlp`; instálalo con pipx install yt-dlp" }),
    "Falta yt-dlp para descargar enlaces: no se encontró `yt-dlp`; instálalo con pipx install yt-dlp",
  );
});

test("if yt-dlp asks to sign in, what to do is explained", () => {
  const cases = [
    "ERROR: [Instagram] abc: Requested content is not available, rate-limit reached or login required",
    "ERROR: [youtube] x: Sign in to confirm your age",
    "ERROR: Use --cookies-from-browser or --cookies for the authentication",
    "ERROR: This video is private",
  ];
  for (const message of cases) {
    assert.equal(
      errorMessage({ kind: "failed_download", message }),
      "Este video pide iniciar sesión, y la app no usa tus cookies. Descárgalo a mano y elige el archivo.",
      message,
    );
  }
});

test("other errors are shown as they are", () => {
  assert.equal(errorMessage({ kind: "failed_download", message: "HTTP Error 404" }), "HTTP Error 404");
  assert.equal(errorMessage({ kind: "other", message: "sin implementar" }), "sin implementar");
});

test("there are pending entries if any entry waits for download", () => {
  assert.equal(pendingThereIs([{ state: { kind: "ready" } }, { state: { kind: "pending" } }]), true);
  assert.equal(pendingThereIs([{ state: { kind: "ready" } }]), false);
});

test("any yt-dlp failure is not confused with a session request", () => {
  const cases = [
    "yt-dlp no pudo descargar el video (código 1): ERROR: [generic] Unable to download webpage: HTTP Error 404",
    "yt-dlp no pudo descargar el video (código 1): ERROR: Unsupported URL: https://blogin.example/v",
    "yt-dlp no pudo descargar el video (código 1): ERROR: unable to download video. Si el sitio pide iniciar sesión, descárgalo a mano",
  ];
  for (const message of cases) {
    assert.equal(errorMessage({ kind: "failed_download", message }), message);
  }
});

test("login or cookies inside a URL are not a session request", () => {
  const cases = [
    "yt-dlp no pudo descargar el video (código 1): ERROR: Unsupported URL: https://sitio.example/login?next=/v/1",
    "yt-dlp no pudo descargar el video (código 1): ERROR: Unsupported URL: https://sitio.example/recetas/cookies",
  ];
  for (const message of cases) {
    assert.equal(errorMessage({ kind: "failed_download", message }), message);
  }
});

test("if it asks for a session outside the URL it is still explained", () => {
  const cases = [
    "ERROR: [generic] https://sitio.example/v/1: login required",
    "ERROR: https://sitio.example/v/1 needs --cookies",
  ];
  for (const message of cases) {
    assert.ok(errorMessage({ kind: "failed_download", message }).includes("pide iniciar sesión"), message);
  }
});
