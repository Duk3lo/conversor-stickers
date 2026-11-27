import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import type { BotConnectionUi, BotStatusUi } from "./contract-bot.ts";

const examples = JSON.parse(readFileSync(new URL("./contract-bot.examples.json", import.meta.url), "utf8")) as {
  connection: BotConnectionUi;
  without_connection: BotConnectionUi;
  linking: BotStatusUi;
  active: BotStatusUi;
};

test("the shared examples have the fields of the interface types and no token", () => {
  for (const connection of [examples.connection, examples.without_connection]) {
    assert.deepEqual(Object.keys(connection), ["url", "has_token", "enabled"]);
  }
  for (const status of [examples.linking, examples.active]) {
    assert.deepEqual(Object.keys(status), ["link", "connection", "active", "code", "qr", "notice"]);
  }
});
