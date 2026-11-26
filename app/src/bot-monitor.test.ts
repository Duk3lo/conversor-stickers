import assert from "node:assert/strict";
import { test } from "node:test";

import type { BotApi, BotStatusUi } from "./contract-bot.ts";
import { createBotMonitor } from "./bot-monitor.ts";

const paused: BotStatusUi = { link: "linked", connection: "connected", active: false, code: null, qr: null, notice: null };
const connection = { url: "http://127.0.0.1:8787", has_token: true, enabled: true };

// A macrotask: every pending promise step has run by then.
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

function setup(overrides: Partial<BotApi> = {}) {
  const calls: string[] = [];
  const timers: { run: () => void; delay: number; id: number }[] = [];
  let nextId = 0;
  const answers: ((status: BotStatusUi) => void)[] = [];
  const api: BotApi = {
    connection: async () => connection,
    connect: async () => connection,
    disable: async () => ({ ...connection, enabled: false }),
    status: () => {
      calls.push("status");
      return new Promise((resolve) => answers.push(resolve));
    },
    activate: async () => {
      calls.push("activate");
      return { ...paused, active: true };
    },
    pause: async () => paused,
    link: async () => paused,
    unlink: async () => paused,
    openChat: async () => undefined,
    ...overrides,
  };
  const monitor = createBotMonitor(api, {
    set(run, delay) {
      timers.push({ run, delay, id: ++nextId });
      return nextId;
    },
    clear(id) {
      const at = timers.findIndex((timer) => timer.id === id);
      if (at >= 0) timers.splice(at, 1);
    },
  });
  const tick = () => timers.shift()?.run();
  const answer = async (status: BotStatusUi = paused) => {
    answers.shift()?.(status);
    await settle();
  };
  return { monitor, calls, timers, answers, tick, answer };
}

test("nothing is asked until a view is on screen, and hiding it stops", async () => {
  const { monitor, calls, timers, answer } = setup();
  await monitor.load();
  await settle();
  assert.deepEqual(calls, []);

  monitor.watch("panel", true);
  await settle();
  await answer();
  assert.deepEqual(calls, ["status"]);
  assert.deepEqual(timers.map((timer) => timer.delay), [5000]);

  monitor.watch("panel", false);
  assert.equal(timers.length, 0);
});

test("requests never overlap, even with two views and a slow bot", async () => {
  const { monitor, calls, timers, answers, answer } = setup();
  await monitor.load();
  monitor.watch("panel", true);
  await settle();
  monitor.watch("editor", true);
  monitor.setForeground(false);
  monitor.setForeground(true);
  const activation = monitor.run((api) => api.activate());
  await settle();
  assert.deepEqual(calls, ["status"]);
  assert.equal(answers.length, 1);
  assert.equal(timers.length, 0);

  await answer();
  assert.equal((await activation).active, true);
  assert.deepEqual(calls, ["status", "activate"]);
  assert.equal(timers.length, 1);
});

test("the background stops polling and coming back asks at once", async () => {
  const { monitor, calls, timers, answer } = setup();
  await monitor.load();
  monitor.watch("panel", true);
  await settle();
  await answer({ ...paused, link: "linking" });
  assert.deepEqual(timers.map((timer) => timer.delay), [2000]);

  monitor.setForeground(false);
  assert.equal(timers.length, 0);
  monitor.setForeground(true);
  await settle();
  assert.deepEqual(calls, ["status", "status"]);
});

test("disabling stops asking and drops the answer that was on its way", async () => {
  const { monitor, calls, timers, answer } = setup();
  await monitor.load();
  monitor.watch("panel", true);
  await settle();

  await monitor.disable();
  await answer();

  assert.equal(monitor.state().status, null);
  assert.equal(monitor.state().connection?.enabled, false);
  assert.equal(timers.length, 0);
  assert.deepEqual(calls, ["status"]);
  await assert.rejects(monitor.run((api) => api.activate()));
  assert.deepEqual(calls, ["status"]);
});

test("a failed operation is thrown and the status is read again", async () => {
  let asked = 0;
  const { monitor } = setup({
    status: async () => {
      asked += 1;
      return paused;
    },
    activate: () => Promise.reject({ kind: "other", message: "el bot responde: sin conexión" }),
  });
  await monitor.load();

  await assert.rejects(monitor.run((api) => api.activate()), { message: "el bot responde: sin conexión" });

  assert.equal(asked, 1);
  assert.deepEqual(monitor.state().status, paused);
});

test("a wrong token is kept as the state and not asked again", async () => {
  let asked = 0;
  const { monitor, timers } = setup({
    status: () => {
      asked += 1;
      return Promise.reject({ kind: "bot_unauthorized", message: "el bot rechazó el token" });
    },
  });
  await monitor.load();
  monitor.watch("panel", true);
  await settle();
  monitor.watch("panel", false);
  monitor.watch("panel", true);
  await settle();

  assert.equal(monitor.state().error?.kind, "bot_unauthorized");
  assert.equal(asked, 1);
  assert.equal(timers.length, 0);
});

test("a connection that cannot be read is an error, not silence", async () => {
  const { monitor } = setup({ connection: () => Promise.reject({ kind: "other", message: "archivo dañado" }) });
  await monitor.load();
  assert.equal(monitor.state().error?.message, "archivo dañado");
});
