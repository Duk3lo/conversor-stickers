// Tiny KLIPY proxy: holds the API key as a secret so the public app does not embed it.
// Only the endpoints the app uses are exposed, with allowlisted and size-limited parameters.
// Neither the key, the client IP nor the queries are ever logged or echoed.

const UPSTREAM = "https://api.klipy.com/api/v1";
const PREFIX = "/klipy/";
const MAX_QUERY_LENGTH = 100;
const MAX_PER_PAGE = 50;
const TTL_TRENDING = 600;
const TTL_SEARCH = 120;

// Mirrors app/src-tauri/src/gifs.rs. `params` are the query parameters each route accepts.
const ROUTES = {
  "gifs/trending": { params: ["page", "per_page"], ttl: TTL_TRENDING },
  "gifs/search": { params: ["q", "page", "per_page"], ttl: TTL_SEARCH, needsQuery: true },
  "stickers/trending": { params: ["page", "per_page"], ttl: TTL_TRENDING },
  "stickers/search": { params: ["q", "page", "per_page"], ttl: TTL_SEARCH, needsQuery: true },
  "stickers/categories": { params: [], ttl: TTL_TRENDING },
};

function json(status, body, extra = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
      ...extra,
    },
  });
}

const fail = (status, message) => json(status, { error: message });

function isNumber(value, max) {
  return /^[0-9]{1,6}$/.test(value) && Number(value) >= 1 && Number(value) <= max;
}

// Returns the clean parameters, in a fixed order, or null if any is invalid.
function cleanParams(route, search) {
  const clean = new URLSearchParams();
  for (const name of route.params) {
    const value = search.get(name);
    if (name === "q") {
      const text = (value ?? "").trim();
      if (text === "" || [...text].length > MAX_QUERY_LENGTH || /\p{Cc}/u.test(text)) {
        return null;
      }
      clean.set("q", text);
    } else if (value !== null) {
      if (!isNumber(value, name === "page" ? 100000 : MAX_PER_PAGE)) return null;
      clean.set(name, value);
    }
  }
  if (route.needsQuery && !clean.has("q")) return null;
  clean.set("content_filter", "medium");
  return clean;
}

async function limited(env, request) {
  if (!env.RATE_LIMITER) return false;
  const client = request.headers.get("CF-Connecting-IP") ?? "unknown";
  const { success } = await env.RATE_LIMITER.limit({ key: client });
  return !success;
}

export async function handle(request, env, deps) {
  if (request.method !== "GET") {
    return json(405, { error: "Método no permitido" }, { Allow: "GET" });
  }
  const url = new URL(request.url);
  if (!url.pathname.startsWith(PREFIX)) return fail(404, "No encontrado");
  const route = ROUTES[url.pathname.slice(PREFIX.length)];
  if (!route) return fail(404, "No encontrado");

  const params = cleanParams(route, url.searchParams);
  if (params === null) return fail(400, "Parámetros no válidos");

  if (await limited(env, request)) {
    return fail(429, "Demasiadas peticiones: espera un minuto");
  }

  // The cache key never contains the KLIPY key: it is built from our own public URL.
  params.sort();
  const cacheKey = new Request(`${url.origin}${url.pathname}?${params}`);
  const cached = await deps.cache.match(cacheKey);
  if (cached) return cached;

  if (!env.KLIPY_KEY) return fail(503, "Servicio no disponible");
  let upstream;
  try {
    upstream = await deps.fetch(`${UPSTREAM}/${env.KLIPY_KEY}/${url.pathname.slice(PREFIX.length)}?${params}`, {
      headers: { Accept: "application/json" },
    });
  } catch {
    return fail(503, "Servicio no disponible");
  }
  if (upstream.status === 429 || upstream.status >= 500) {
    return fail(503, "Servicio no disponible");
  }
  if (!upstream.ok) return fail(upstream.status === 404 ? 404 : 502, "Respuesta no válida del servicio");

  const response = new Response(await upstream.text(), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
      "Cache-Control": `public, max-age=${route.ttl}`,
      "X-Content-Type-Options": "nosniff",
    },
  });
  await deps.cache.put(cacheKey, response.clone());
  return response;
}

export default {
  fetch(request, env) {
    const deps = { fetch: (url, init) => fetch(url, init), cache: caches.default };
    return handle(request, env, deps).catch(() => fail(500, "Error interno"));
  },
};
