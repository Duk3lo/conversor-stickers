import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { PLAY_LOGO_COLORS } from "./edition.ts";

test("the header logo has plain colors and every part has a Play color", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const logo = html.slice(html.indexOf('<svg class="header-logo"'), html.indexOf("</svg>", html.indexOf('<svg class="header-logo"')));
  assert.ok(!logo.includes("var(--"), "the WebView paints CSS variables in gradient stops black");
  const roles = [...logo.matchAll(/data-logo="([^"]+)"/g)].map((match) => match[1] ?? "");
  assert.ok(roles.length > 0);
  for (const role of roles) assert.ok(role in PLAY_LOGO_COLORS, `no Play color for ${role}`);
});
