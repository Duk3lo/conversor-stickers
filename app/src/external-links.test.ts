import assert from "node:assert/strict";
import { test } from "node:test";

import { externalAddress } from "./external-links.ts";

test("http and https links leave the app; the rest stays in it", () => {
  assert.equal(externalAddress("https://github.com/x"), "https://github.com/x");
  assert.equal(externalAddress("HTTP://example.com"), "HTTP://example.com");
  for (const href of [null, "", "#icon-save", "/ruta", "ruta.html", "mailto:a@b.c", "javascript:alert(1)", "tauri://localhost"]) {
    assert.equal(externalAddress(href), null, String(href));
  }
});
