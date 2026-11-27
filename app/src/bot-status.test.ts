import assert from "node:assert/strict";
import { test } from "node:test";

import type { BotStatusUi } from "./contract-bot.ts";
import type { MediaUi, UiError } from "./contract.ts";
import { builtinExtension, type InstalledExtension } from "./extensions.ts";
import { type BotState, botActions, botBox, botNotice, describeBot, manualTargets, numberHeld, offerReceivedLink, oneAtATime, pairingPhone, phoneFieldVisible, pollDelayMs, qrNoteText, receivedLinkQuestion, shouldConnectLocal, submitLink } from "./bot-status.ts";

const connection = { url: "https://equipo.red.ts.net", has_token: true, enabled: true };
const paused: BotStatusUi = { link: "linked", connection: "connected", active: false, code: null, qr: null, notice: null };
const enabled = (status: Partial<BotStatusUi> | null, error: UiError | null = null): BotState => ({
  connection,
  status: status === null ? null : { ...paused, ...status },
  error,
});
const off: BotState = { connection: { ...connection, enabled: false }, status: null, error: null };
const bot: InstalledExtension = { extension: builtinExtension, accepted: true, number: "34612345678" };
const other: InstalledExtension = { ...bot, extension: { ...builtinExtension, id: "otra" } };
const video: MediaUi = { kind: "video", duration_s: 20, width: 640, height: 360, has_audio: true };

test("each situation of the bot has its own description", () => {
  const cases: [BotState, string, RegExp][] = [
    [off, "off", /Sin conexión/],
    [enabled(null), "checking", /Consultando/],
    [enabled(null, { kind: "bot_unreachable", message: "x" }), "unreachable", /Tailscale.*arrancado/],
    [enabled(null, { kind: "bot_unauthorized", message: "x" }), "unauthorized", /Token incorrecto/],
    [enabled(null, { kind: "other", message: "versión 2" }), "error", /versión 2/],
    [enabled({ link: "unlinked", connection: "disconnected" }), "unlinked", /sin vincular/],
    [enabled({ link: "linking", connection: "connecting" }), "linking", /Vinculando/],
    [enabled({ connection: "connecting" }), "offline", /conectando/],
    [enabled({ connection: "disconnected" }), "offline", /sin conexión con WhatsApp/],
    [enabled({}), "paused", /en pausa/],
    [enabled({ active: true }), "active", /activo/],
    [{ connection: null, status: null, error: { kind: "other", message: "archivo dañado" } }, "error", /dañado/],
  ];
  for (const [state, kind, text] of cases) {
    assert.equal(describeBot(state).kind, kind);
    assert.match(describeBot(state).text, text);
  }
});

test("the bot's notice is labelled and only shown while connected to it", () => {
  assert.equal(botNotice(enabled({ notice: "Sesión cerrada" })), "Aviso del bot: Sesión cerrada");
  assert.equal(botNotice(enabled({})), null);
  assert.equal(botNotice({ ...enabled({ notice: "a" }), connection: off.connection }), null);
});

test("only the actions the bot accepts are offered", () => {
  const none = { activate: false, pause: false, link: false, unlink: false };
  assert.deepEqual(botActions(off), none);
  assert.deepEqual(botActions(enabled(null)), none);
  assert.deepEqual(botActions(enabled({ link: "unlinked", connection: "disconnected" })), { ...none, link: true });
  assert.deepEqual(botActions(enabled({ link: "linking", connection: "connecting" })), none);
  assert.deepEqual(botActions(enabled({ connection: "disconnected" })), none);
  assert.deepEqual(botActions(enabled({})), { ...none, activate: true, unlink: true });
  assert.deepEqual(botActions(enabled({ active: true })), { ...none, pause: true, unlink: true });
});

test("polling is faster while linking, slower after a failure and stops with a wrong token", () => {
  assert.equal(pollDelayMs(off), null);
  assert.equal(pollDelayMs(enabled({ link: "linking" })), 2000);
  assert.equal(pollDelayMs(enabled({ connection: "connecting" })), 2000);
  assert.equal(pollDelayMs(enabled({})), 5000);
  assert.equal(pollDelayMs(enabled(null, { kind: "bot_unreachable", message: "x" })), 10000);
  assert.equal(pollDelayMs(enabled(null, { kind: "bot_unauthorized", message: "x" })), null);
});

test("the editor shows the bot only for a video sticker with the extension accepted", () => {
  assert.equal(botBox("gif", video, [bot], enabled({})), null);
  assert.equal(botBox("sticker", { kind: "image", path: "a", width: 1, height: 1 }, [bot], enabled({})), null);
  assert.equal(botBox("sticker", video, [{ ...bot, accepted: false }], enabled({})), null);
  assert.equal(botBox("sticker", video, [other], enabled({})), null);
  assert.equal(botBox("sticker", video, [bot], null), null);
});

test("the send button needs an enabled connection and a saved number", () => {
  assert.equal(botBox("sticker", video, [bot], off)?.send, null);
  assert.deepEqual(botBox("sticker", video, [bot], enabled({}))?.send, { reason: null });
  assert.match(botBox("sticker", video, [{ ...bot, number: null }], enabled({}))?.send?.reason ?? "", /número del bot/);
  assert.equal(botBox("sticker", video, [bot], enabled({ link: "unlinked" }))?.panelLabel, "Vincular el bot");
  assert.match(botBox("sticker", video, [bot], enabled({ notice: "Reinicia" }))?.line ?? "", /Aviso del bot: Reinicia/);
});

test("the manual send of the bot is replaced only while its connection is enabled", () => {
  assert.deepEqual(manualTargets([bot, other], enabled({})), [other]);
  assert.deepEqual(manualTargets([bot, other], off), [bot, other]);
  assert.deepEqual(manualTargets([bot, other], null), [bot, other]);
});

test("an unreachable bot or a rejected token leaves the manual send available", () => {
  for (const kind of ["bot_unreachable", "bot_unauthorized"] as const) {
    const failed = enabled(null, { kind, message: "x" });
    assert.deepEqual(manualTargets([bot, other], failed), [bot, other]);
    assert.equal(botBox("sticker", video, [bot], failed)?.send, null);
  }
});

test("the bot number is held while its connection is enabled", () => {
  assert.equal(numberHeld(bot, enabled({})), true);
  assert.equal(numberHeld(bot, off), false);
  assert.equal(numberHeld(bot, null), false);
  assert.equal(numberHeld({ ...bot, accepted: false }, enabled({})), true);
  assert.equal(numberHeld(other, enabled({})), false);
});

test("on the phone the QR comes with how to scan it", () => {
  assert.match(qrNoteText("android", "2@abc") ?? "", /captura.*otra pantalla.*código/s);
  assert.equal(qrNoteText("desktop", "2@abc"), null);
  assert.equal(qrNoteText("android", null), null);
});

test("the number to link by code is cleaned, optional and validated", () => {
  assert.deepEqual(pairingPhone(" +34 600-111 (222) "), { ok: true, phone: "34600111222" });
  assert.deepEqual(pairingPhone("  "), { ok: true, phone: null });
  for (const typed of ["1234567", "1234567890123456", "34600abc222", "+", "34.600.111.222"]) {
    assert.deepEqual(pairingPhone(typed), { ok: false }, typed);
  }
});

test("the QR never sends a number, even if the field is filled in", () => {
  const sent: (string | null)[] = [];
  let invalid = 0;
  submitLink("qr", "+34 600 111 222", (_, phone) => sent.push(phone), () => invalid++);
  submitLink("qr", "abc", (_, phone) => sent.push(phone), () => invalid++);
  assert.deepEqual(sent, [null, null]);
  assert.equal(invalid, 0);
});

test("a number that is not valid never calls link", () => {
  const calls: string[] = [];
  let invalid = 0;
  for (const typed of ["1234567", "1234567890123456", "34600abc222"]) {
    submitLink("code", typed, (method) => calls.push(method), () => invalid++);
  }
  assert.deepEqual(calls, []);
  assert.equal(invalid, 3);
  submitLink("code", "+34 600 111 222", (method, phone) => calls.push(`${method}:${phone}`), () => invalid++);
  submitLink("code", "", (method, phone) => calls.push(`${method}:${phone}`), () => invalid++);
  assert.deepEqual(calls, ["code:34600111222", "code:null"]);
  assert.equal(invalid, 3);
});

test("the number field is only there while the bot can be linked", () => {
  assert.equal(phoneFieldVisible(enabled({ link: "unlinked", connection: "disconnected" })), true);
  for (const state of [off, enabled(null), enabled({}), enabled({ link: "linking", connection: "connecting" }), enabled(null, { kind: "bot_unreachable", message: "x" })]) {
    assert.equal(phoneFieldVisible(state), false);
  }
});

test("only desktop, with the panel visible and nothing saved, looks for the bot of this computer", () => {
  const nothing: BotState = { connection: { url: null, has_token: false, enabled: false }, status: null, error: null };
  assert.equal(shouldConnectLocal("desktop", nothing, true), true);
  assert.equal(shouldConnectLocal("android", nothing, true), false);
  assert.equal(shouldConnectLocal("desktop", nothing, false), false);
  assert.equal(shouldConnectLocal("desktop", { ...nothing, connection: null }, true), false);
  const saved = { ...nothing, connection: { url: "http://127.0.0.1:8787", has_token: true, enabled: false } };
  assert.equal(shouldConnectLocal("desktop", saved, true), false);
});

function received(url: string | null, answer: boolean, failure?: Error) {
  const calls: string[] = [];
  const fails: unknown[] = [];
  const done = offerReceivedLink({
    receive: async () => {
      if (failure !== undefined) throw failure;
      return url;
    },
    ask: async (question) => {
      calls.push(question);
      return answer;
    },
    accept: async () => void calls.push("accept"),
    reject: async () => void calls.push("reject"),
    fail: (error) => void fails.push(error),
  });
  return { calls, fails, done };
}

test("a received link is saved only after the person confirms its address", async () => {
  const yes = received("https://equipo.red.ts.net", true);
  await yes.done;
  assert.deepEqual(yes.calls, ["¿Conectar con el bot en https://equipo.red.ts.net?", "accept"]);
  assert.equal(receivedLinkQuestion("http://127.0.0.1:8787"), "¿Conectar con el bot en http://127.0.0.1:8787?");
});

test("a refused link is dropped, no link asks nothing and a failure is reported", async () => {
  const no = received("https://equipo.red.ts.net", false);
  await no.done;
  assert.deepEqual(no.calls.slice(1), ["reject"]);
  const none = received(null, true);
  await none.done;
  assert.deepEqual(none.calls, []);
  const broken = received(null, true, new Error("enlace no válido"));
  await broken.done;
  assert.equal(broken.fails.length, 1);
  assert.deepEqual(broken.calls, []);
});

test("a call during a run makes it run once more, and several calls only once", async () => {
  let runs = 0;
  let release = (): void => {};
  const run = oneAtATime(async () => {
    runs += 1;
    if (runs === 1) await new Promise<void>((resolve) => (release = resolve));
  });
  const first = run();
  await run();
  await run();
  assert.equal(runs, 1);
  release();
  await first;
  assert.equal(runs, 2);
  await run();
  assert.equal(runs, 3);
});
