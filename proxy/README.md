# conversor-stickers-proxy

A tiny Cloudflare Worker that holds the KLIPY API key as a secret, so the public app does not embed
it. It exposes only the endpoints the app uses, under `/klipy/<path>`:

- `GET /klipy/gifs/trending`, `GET /klipy/gifs/search?q=`
- `GET /klipy/stickers/trending`, `GET /klipy/stickers/search?q=`, `GET /klipy/stickers/categories`

Accepted query parameters: `q` (at most 100 characters), `page` and `per_page` (numeric, `per_page`
at most 50). The content filter is fixed by the Worker. Anything else is a 404 (path) or 405 (method).
Responses are cached (10 minutes for trending and categories, 2 minutes for searches) and each IP is
limited to 60 requests per minute. The key, IPs and queries are never logged or returned.

## Setup

```sh
cd proxy
npm ci
npx wrangler login
npx wrangler secret put KLIPY_KEY   # paste the key when asked
npm run deploy
```

`npm run deploy` prints the Worker address (`https://conversor-stickers-proxy.<account>.workers.dev`).
For local work, `npm run dev` runs it with a `.dev.vars` file containing `KLIPY_KEY=...` (ignored by
git). `npm test` runs the unit tests with `node --test`.

## How the app finds the proxy

The app reads the build-time environment variable `CONVERSOR_PROXY_URL` (an `https://` address with
no path). Set it to the Worker address when building a release, for example:

```sh
CONVERSOR_PROXY_URL=https://conversor-stickers-proxy.<account>.workers.dev \
    app/src-tauri/android-release.sh
```

If the user has their own KLIPY key, or one was embedded at build time with `KLIPY_KEY`, the app talks
to `api.klipy.com` directly and ignores the proxy. Public releases must not set `KLIPY_KEY`.

## Rotating the key

Create the new key in KLIPY, run `npx wrangler secret put KLIPY_KEY` with it and revoke the old one.
No new app release is needed, because the app never sees the key.
