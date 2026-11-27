import assert from "node:assert/strict";
import { test } from "node:test";

import type { BotStatusUi } from "./contract-bot.ts";
import { type BotSendSteps, desktopDelivered, runBotSend } from "./bot-send.ts";

const paused: BotStatusUi = { link: "linked", connection: "connected", active: false, code: null, qr: null, notice: null };

function steps(overrides: Partial<BotSendSteps> = {}) {
  const done: string[] = [];
  const all: BotSendSteps = {
    status: async () => (done.push("status"), paused),
    activate: async () => (done.push("activate"), { ...paused, active: true }),
    save: async () => (done.push("save"), "clip.mp4"),
    deliver: async (name) => (done.push(`deliver ${name}`), "Listo"),
    abandoned: () => false,
    progress: () => undefined,
    ...overrides,
  };
  return { all, done };
}

test("saves, then asks, activates a paused bot and delivers", async () => {
  const { all, done } = steps();
  assert.deepEqual(await runBotSend(all), { sent: true, message: "Listo" });
  assert.deepEqual(done, ["save", "status", "activate", "deliver clip.mp4"]);
});

test("an active bot is not activated again", async () => {
  const { all, done } = steps();
  all.status = async () => (done.push("status"), { ...paused, active: true });
  assert.equal((await runBotSend(all)).sent, true);
  assert.deepEqual(done, ["save", "status", "deliver clip.mp4"]);
});

test("without linking nothing is activated, saved or sent", async () => {
  const { all, done } = steps({ status: async () => ({ ...paused, link: "unlinked" }) });
  const outcome = await runBotSend(all);
  assert.equal(outcome.sent, false);
  assert.match(outcome.message, /No se ha enviado nada.*vincula/);
  assert.deepEqual(done, ["save"]);
});

test("a refused activation delivers nothing and says why", async () => {
  const { all, done } = steps({ activate: () => Promise.reject({ kind: "other", message: "el bot responde: sin WhatsApp" }) });
  const outcome = await runBotSend(all);
  assert.deepEqual(outcome, { sent: false, message: "No se ha enviado nada. No se pudo activar el bot: el bot responde: sin WhatsApp." });
  assert.deepEqual(done, ["save", "status"]);

  const refused = steps({ activate: async () => paused });
  assert.match((await runBotSend(refused.all)).message, /no se ha activado/);
});

test("an activation that gets no answer warns that the bot may have stayed active", async () => {
  const { all } = steps({ activate: () => Promise.reject({ kind: "bot_unreachable", message: "sin respuesta" }) });
  assert.match((await runBotSend(all)).message, /puede haber quedado activo/);
});

test("cancelling the save or leaving the editor never activates the bot", async () => {
  const cancelled = steps({ save: async () => null });
  const outcome = await runBotSend(cancelled.all);
  assert.equal(outcome.sent, false);
  assert.match(outcome.message, /canceló/);
  assert.deepEqual(cancelled.done, []);

  let left = false;
  const closed = steps({ abandoned: () => left, status: async () => ((left = true), paused) });
  assert.equal((await runBotSend(closed.all)).sent, false);
  assert.deepEqual(closed.done, ["save"]);
});

test("leaving the editor during the activation says the bot stayed active", async () => {
  let left = false;
  const { all, done } = steps({ abandoned: () => left, activate: async () => ((left = true), { ...paused, active: true }) });
  assert.match((await runBotSend(all)).message, /El bot quedó activo/);
  assert.ok(!done.some((step) => step.startsWith("deliver")));
});

test("a failed save or delivery is explained", async () => {
  const save = steps({ save: () => Promise.reject({ kind: "other", message: "disco lleno" }) });
  assert.match((await runBotSend(save.all)).message, /No se pudo guardar el clip: disco lleno/);
  assert.ok(!save.done.some((step) => step.startsWith("deliver")));
  const deliver = steps({ deliver: () => Promise.reject({ kind: "other", message: "No se encontró WhatsApp instalado" }) });
  assert.match((await runBotSend(deliver.all)).message, /se guardó.*No se encontró WhatsApp/);
});

test("on desktop the message says to attach the video and paste the caption", () => {
  assert.match(desktopDelivered("a.mp4", "sonido=10", true), /Adjunta ese video, pega como pie el texto ya copiado: sonido=10/);
  assert.match(desktopDelivered("a.mp4", "sonido=10", false), /Copiar pie.*sonido=10/);
  assert.doesNotMatch(desktopDelivered("a.mp4", null, false), /pie/);
});
