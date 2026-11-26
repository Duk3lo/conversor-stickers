import assert from "node:assert/strict";
import { test } from "node:test";

import type { BotStatusUi } from "./contract-bot.ts";
import type { MediaUi, UiError } from "./contract.ts";
import { builtinExtension, type InstalledExtension } from "./extensions.ts";
import { type BotState, botActions, botBox, botNotice, describeBot, manualTargets, numberHeld, pollDelayMs, qrElsewhereText } from "./bot-status.ts";

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

test("a QR started on the computer is not asked to be scanned on the phone", () => {
  assert.match(qrElsewhereText("android", "2@abc") ?? "", /en la PC/);
  assert.equal(qrElsewhereText("desktop", "2@abc"), null);
  assert.equal(qrElsewhereText("android", null), null);
});
