# Community catalog protocol

The catalog is a **static site**: an index plus the files of each pack. GitHub Pages serves it today
from the public repository `conversor-stickers-comunidad`; any server can serve it as long as the
paths below exist. The app only needs the base URL (default
`https://duk3lo.github.io/conversor-stickers-comunidad/`). To change it, set
`CONVERSOR_CATALOGO_URL` at build time. The URL must use `https`.

The rules live in one place, `core/src/catalog.rs`. The CLI and the app both use them.

## Paths

- `GET <base>index.json`
- `GET <base>packs/<id>/<file>`

## `index.json` (format 1)

```json
{
  "format": 1,
  "generated": "2026-11-22T00:00:00Z",
  "categories": [{ "id": "animales", "name": "Animales" }],
  "packs": [{
    "id": "gatos-garabato",
    "version": 1,
    "name": "Gatos garabato",
    "author": "Ana",
    "license": "CC-BY-4.0",
    "category": "animales",
    "tags": ["gato"],
    "animated": false,
    "added": "2026-11-22",
    "icon": { "file": "icon.png", "bytes": 3012, "sha256": "...64 hex..." },
    "stickers": [
      { "file": "01.webp", "bytes": 18234, "sha256": "...", "emojis": ["🐱"] }
    ]
  }],
  "withdrawn": [{ "id": "otro-paquete", "date": "2026-11-23", "reason": "..." }]
}
```

`generated` comes from the most recent date in the catalog, not from the clock, so the same catalog
always produces the same bytes. If `format` is higher than the format the app knows, the app shows
"update the app to see the catalog" and reads nothing else. The server may send an `ETag`; the app
uses it to avoid downloading an index that has not changed.

## Rules

`conversor-stickers catalog validate` checks these (the repository's CI runs it), and the app checks
them again when it downloads:

- `id`: `^[a-z0-9]+(-[a-z0-9]+)*$`, 3 to 64 characters, unique.
- `file`: `^\d{2}\.webp$` for stickers, `icon.png` for the icon.
- 3 to 30 stickers. All of them static, or all animated, according to `animated`.
- Each sticker is a 512x512 WebP that passes the core validator (100 KB static, 500 KB animated,
  duration and fps limits). The icon is a 96x96 PNG under 50 KB.
- `license`: `CC0-1.0`, `CC-BY-4.0` or `CC-BY-SA-4.0`.
- `name`, `author`, tags and reasons: 1 to 128 characters, with no control characters. Up to 10
  tags. `category` must exist in `categories`. Each sticker has 1 to 3 emojis.
- The index weighs at most 5 MB and has at most 2000 packs.
- A pack listed in `withdrawn` cannot stay in the catalog.

## How the app downloads

Only `https://`, with at most 3 redirects, and only to the same host. Timeout 15 s, and a limit on
the bytes read while streaming. Each file is checked against `bytes` and `sha256` from the index and
against the validator before it is accepted. A pack is added whole or not at all. Local packs whose
`id` appears in `withdrawn` are marked as withdrawn. They cannot be added to WhatsApp, but they are
not deleted automatically.

## On-disk layout (repository)

```
categories.json              [{ "id", "name" }]
withdrawn.json               [{ "id", "date", "reason" }]
packs/<id>/pack.json         pack fields without bytes or sha256
packs/<id>/NN.webp           stickers, numbered 01, 02, ...
packs/<id>/icon.png          tray icon
```

`pack.json` has `icon: { "file": "icon.png" }` and, for each sticker, `file` and `emojis`. Files that
are not listed in it are rejected.

```sh
conversor-stickers catalog validate <DIR>                  # 0 valid, 3 breaks a rule
conversor-stickers catalog index <DIR> --output index.json
```

`index` writes the sums and sizes. The same directory always produces the same bytes.

## Proposing from the app

"Propose to the community" validates the pack with these same rules. It saves a `.zip` with the
`<id>/` folder and a `LEEME.txt` file (Spanish, user-facing) that links to `CONTRIBUTING.md`. The app
uploads nothing: you open the pull request yourself, and the folder goes into `packs/`.
