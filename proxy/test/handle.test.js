import assert from "node:assert/strict";
import test from "node:test";

import { handle } from "../src/index.js";

const KEY = "clave-secreta-de-prueba";
const BODY = JSON.stringify({ result: true, data: { data: [], current_page: 1, has_next: false } });

function setup({ status = 200, limiter } = {}) {
  const calls = [];
  const store = new Map();
  const deps = {
    fetch: async (url) => {
      calls.push(url);
      return new Response(BODY, { status });
    },
    cache: {
      match: async (request) => store.get(request.url)?.clone(),
      put: async (request, response) => void store.set(request.url, response),
    },
  };
  const env = { KLIPY_KEY: KEY, ...(limiter ? { RATE_LIMITER: limiter } : {}) };
  const get = (path, method = "GET") =>
    handle(new Request(`https://proxy.example${path}`, { method }), env, deps);
  return { calls, store, get };
}

test("each route the app uses is forwarded with the key in the upstream path", async () => {
  const { calls, get } = setup();
  for (const route of ["gifs/trending", "gifs/search?q=hola", "stickers/trending", "stickers/search?q=oso", "stickers/categories"]) {
    const response = await get(`/klipy/${route}`);
    assert.equal(response.status, 200, route);
  }
  assert.equal(calls.length, 5);
  for (const url of calls) assert.ok(url.startsWith(`https://api.klipy.com/api/v1/${KEY}/`), url);
});

test("the key never appears in responses, headers or cache keys", async () => {
  const { store, get } = setup();
  for (const path of ["/klipy/gifs/trending", "/klipy/nada", "/klipy/gifs/search"]) {
    const response = await get(path);
    const text = JSON.stringify([...response.headers]) + (await response.clone().text());
    assert.ok(!text.includes(KEY), path);
  }
  for (const url of store.keys()) assert.ok(!url.includes(KEY));
  assert.equal((await setup({ status: 500 }).get("/klipy/gifs/trending")).status, 503);
});

test("unknown paths and unlisted endpoints are 404", async () => {
  const { calls, get } = setup();
  for (const path of ["/", "/klipy", "/klipy/", "/klipy/gifs", "/klipy/gifs/categories", "/klipy/../x", "/klipy/stickers/trending/x", "/api/v1/gifs/trending", "/klipy/%67ifs/trending"]) {
    assert.equal((await get(path)).status, 404, path);
  }
  assert.equal(calls.length, 0);
});

test("only GET is accepted", async () => {
  const { calls, get } = setup();
  for (const method of ["POST", "PUT", "DELETE", "PATCH"]) {
    const response = await get("/klipy/gifs/trending", method);
    assert.equal(response.status, 405, method);
    assert.equal(response.headers.get("Allow"), "GET");
  }
  assert.equal(calls.length, 0);
});

test("parameters are allowlisted and limited", async () => {
  const { calls, get } = setup();
  const bad = [
    "gifs/search",
    "gifs/search?q=",
    "gifs/search?q=%20%20",
    `gifs/search?q=${"a".repeat(101)}`,
    "gifs/search?q=a%00b",
    "gifs/trending?page=0",
    "gifs/trending?page=abc",
    "gifs/trending?page=-1",
    "gifs/trending?page=1.5",
    "gifs/trending?per_page=51",
    "gifs/trending?per_page=0",
  ];
  for (const route of bad) assert.equal((await get(`/klipy/${route}`)).status, 400, route);
  assert.equal(calls.length, 0);

  const ok = await get(`/klipy/gifs/search?q=${"a".repeat(100)}&page=2&per_page=50`);
  assert.equal(ok.status, 200);
});

test("unknown parameters are dropped and content_filter is fixed by the server", async () => {
  const { calls, get } = setup();
  await get("/klipy/gifs/trending?content_filter=off&customer_id=x&per_page=24&page=1");
  const upstream = new URL(calls[0]);
  assert.deepEqual([...upstream.searchParams.keys()].sort(), ["content_filter", "page", "per_page"]);
  assert.equal(upstream.searchParams.get("content_filter"), "medium");
  await get("/klipy/stickers/categories?q=x");
  assert.equal(new URL(calls[1]).searchParams.has("q"), false);
});

test("responses are cached by normalized URL with different lifetimes", async () => {
  const { calls, get } = setup();
  const trending = await get("/klipy/stickers/trending?page=1&per_page=24");
  assert.equal(trending.headers.get("Cache-Control"), "public, max-age=600");
  const same = await get("/klipy/stickers/trending?per_page=24&page=1&extra=1");
  assert.equal(same.status, 200);
  assert.equal(calls.length, 1);

  const search = await get("/klipy/gifs/search?q=hola");
  assert.equal(search.headers.get("Cache-Control"), "public, max-age=120");
  await get("/klipy/gifs/search?q=hola");
  assert.equal(calls.length, 2);
  await get("/klipy/gifs/search?q=adios");
  assert.equal(calls.length, 3);

  const categories = await get("/klipy/stickers/categories");
  assert.equal(categories.headers.get("Cache-Control"), "public, max-age=600");
});

test("responses are safe: nosniff, no cookies", async () => {
  const { get } = setup();
  for (const path of ["/klipy/gifs/trending", "/klipy/nada", "/klipy/gifs/search"]) {
    const response = await get(path);
    assert.equal(response.headers.get("X-Content-Type-Options"), "nosniff", path);
    assert.equal(response.headers.get("Set-Cookie"), null);
    assert.ok(response.headers.get("Cache-Control"));
  }
});

test("rate limit: per IP, 429 in Spanish, and nothing is requested", async () => {
  const keys = [];
  const limiter = { limit: async ({ key }) => (keys.push(key), { success: keys.length <= 1 }) };
  const { calls, get } = setup({ limiter });
  assert.equal((await get("/klipy/gifs/trending")).status, 200);
  const limited = await get("/klipy/gifs/search?q=hola");
  assert.equal(limited.status, 429);
  assert.match((await limited.json()).error, /Demasiadas peticiones/);
  assert.equal(calls.length, 1);
});

test("rate limit keys on CF-Connecting-IP", async () => {
  const keys = [];
  const limiter = { limit: async ({ key }) => (keys.push(key), { success: true }) };
  const deps = { fetch: async () => new Response(BODY), cache: { match: async () => undefined, put: async () => {} } };
  const request = new Request("https://proxy.example/klipy/gifs/trending", { headers: { "CF-Connecting-IP": "203.0.113.7" } });
  await handle(request, { KLIPY_KEY: KEY, RATE_LIMITER: limiter }, deps);
  assert.deepEqual(keys, ["203.0.113.7"]);
});

test("without the rate limit binding it still works", async () => {
  assert.equal((await setup().get("/klipy/gifs/trending")).status, 200);
});

test("upstream 429 and 5xx become a short 503; other errors do not leak", async () => {
  for (const status of [429, 500, 502, 503]) {
    const response = await setup({ status }).get("/klipy/gifs/trending");
    assert.equal(response.status, 503, status);
    assert.deepEqual(await response.json(), { error: "Servicio no disponible" });
  }
  assert.equal((await setup({ status: 401 }).get("/klipy/gifs/trending")).status, 502);
});

test("a failing upstream fetch is a 503 and is not cached", async () => {
  const store = new Map();
  const deps = {
    fetch: async () => {
      throw new Error(`boom ${KEY}`);
    },
    cache: { match: async () => undefined, put: async (r, v) => void store.set(r.url, v) },
  };
  const response = await handle(new Request("https://proxy.example/klipy/gifs/trending"), { KLIPY_KEY: KEY }, deps);
  assert.equal(response.status, 503);
  assert.ok(!(await response.text()).includes(KEY));
  assert.equal(store.size, 0);
});

test("missing secret is a 503 without calling upstream", async () => {
  let called = false;
  const deps = { fetch: async () => ((called = true), new Response(BODY)), cache: { match: async () => undefined, put: async () => {} } };
  const response = await handle(new Request("https://proxy.example/klipy/gifs/trending"), {}, deps);
  assert.equal(response.status, 503);
  assert.equal(called, false);
});
