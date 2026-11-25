import assert from "node:assert/strict";
import { test } from "node:test";

import { type CommunityPackUi, downloadFromCommunity } from "./contract-community.ts";

interface Call {
  command: string;
  args: Record<string, unknown>;
}

// The window has no `core:event:allow-listen`, so progress must travel through a channel passed to
// the command, never through event listeners.
test("downloadFromCommunity reports progress through a channel and never listens to events", async () => {
  const calls: Call[] = [];
  const callbacks = new Map<number, (message: unknown) => void>();
  let next = 1;
  Object.assign(globalThis, {
    window: {
      __TAURI_INTERNALS__: {
        transformCallback(callback: (message: unknown) => void): number {
          callbacks.set(next, callback);
          return next++;
        },
        async invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
          calls.push({ command, args });
          if (command === "plugin:event|listen") throw "event.listen not allowed";
          const channel = args.channel as { toJSON(): string };
          const id = Number(channel.toJSON().replace("__CHANNEL__:", ""));
          callbacks.get(id)?.({ index: 0, message: { kind: "verifying" } });
          return { library: { packs: [] }, pack: 7 };
        },
      },
    },
  });

  const seen: unknown[] = [];
  const created = await downloadFromCommunity("gatos", (progress) => seen.push(progress));

  assert.equal(created.pack, 7);
  assert.deepEqual(seen, [{ kind: "verifying" }]);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["download_from_community"],
  );
  assert.equal(calls[0]?.args.id, "gatos");
});

test("the typed community pack matches the JSON example that Rust checks", async () => {
  const { readFileSync } = await import("node:fs");
  const json = JSON.parse(readFileSync(new URL("./contract-community.examples.json", import.meta.url), "utf8")) as Record<
    string,
    unknown
  >;
  const pack: CommunityPackUi = {
    id: "gatos-garabato",
    version: 2,
    name: "Gatos garabato",
    author: "Ana",
    license: "CC-BY-4.0",
    category: "animales",
    tags: ["gato", "garabato"],
    animated: false,
    added: "2026-11-22",
    stickers: 3,
    icon: "/cache/comunidad/archivos/ab12.png",
    downloaded: 7,
    updatable: true,
  };
  assert.deepEqual(pack, json["community_pack_ui"]);
});
