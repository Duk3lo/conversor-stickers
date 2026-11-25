# conversor-stickers

Converts a video into an animated WhatsApp sticker, or into a GIF, animated emoji or animated
Discord sticker that meets the target's rules. The input is a local video file or an `http`/`https`
link from any site that [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) supports (TikTok, Instagram,
YouTube, X, Reddit...). Several inputs can be passed at once.

The project has three parts:

- `core/`: the conversion library (decoding with `ffmpeg`, fitting the size, encoding WebP, GIF and
  APNG, and the community catalog rules).
- `cli/`: the `conversor-stickers` command.
- `app/`: a Tauri 2 desktop and Android app with the same conversion, a sticker library and a
  community catalog.

The user interface is in Spanish. Code, identifiers, comments and documentation are in English.

> **Notice:** you are responsible for having the right to download and transform every video you
> pass as a link (because it is yours, its license allows it, or you have the author's permission).
> The program does not check this.

## Requirements

- Rust 1.88 or later (`rustup`) and `cargo`.
- A C compiler (`cc`, for example `gcc` or `clang`). `libwebp-sys2` builds the libwebp it bundles.
- `ffmpeg` and `ffprobe` in `PATH` to run the program (not to build it), for example
  `sudo apt-get install ffmpeg` or `sudo pacman -S ffmpeg`. FFmpeg 9 or later is needed to use an
  animated WebP as input.
- `yt-dlp` in `PATH`, only for links (for example `pipx install yt-dlp`). Keep it up to date, because
  sites change often. Local files work without it.

## Build and install

```sh
cargo build --release              # binary at target/release/conversor-stickers
cargo install --path cli           # installs it in ~/.cargo/bin
```

Commands are run from the workspace root. Before running the checks, the desktop app's frontend has
to be built once (`cd app && npm ci && npm run build`), because `tauri::generate_context!` needs
`app/dist/`.

```sh
cargo fmt --all                                            # format
cargo clippy --all-targets --all-features -- -D warnings   # lint: zero warnings
cargo test --all-features                                  # tests
```

## CLI usage

```sh
conversor-stickers <INPUT>... --preset <PRESET> [-o OUTPUT | -d DIR] [OPTIONS]
```

Each `<INPUT>` is a path or a link. An input that looks like a link (`scheme://...`) must be `http`
or `https`. Use single quotes around links, because many contain `?` or `&`.

| Option | Meaning |
|---|---|
| `--preset` | Target: `whatsapp-sticker`, `discord-gif`, `discord-emoji` or `discord-sticker` |
| `-o, --output` | Output file (one input only). Its extension must match the preset |
| `-d, --dir` | Output directory (default: the current one) |
| `--start` | Start of the clip: seconds (`12.5` or `12,5`) or `mm:ss` (`1:05.5`) |
| `--max-seconds` | Maximum length of the result, in seconds |
| `--fps` | Frames per second of the result |
| `--max-mb` | Maximum size in MB, base 10 (only `discord-gif`) |
| `--fit` | If the clip is longer than `--max-seconds`, speeds it up to 2x to make it fit |
| `--preview` | Writes `<output>.preview.webp` (160x160) to see the effect of speed or fps |
| `--dry-run` | Shows the clip, speed and fps that would be used, without writing anything |

Examples:

```sh
# WhatsApp sticker from the first 6 seconds
conversor-stickers cat.mp4 --preset whatsapp-sticker

# From 12.5 s, at most 4 s, 12 fps
conversor-stickers cat.mp4 --preset whatsapp-sticker --start 12.5 --max-seconds 4 --fps 12

# Discord GIF up to 20 MB, starting at 1:05
conversor-stickers cat.mp4 --preset discord-gif --max-mb 20 --start 1:05 -o cat.gif

# A link, saved as <title>.webp in the current directory
conversor-stickers 'https://www.tiktok.com/@user/video/123' --preset whatsapp-sticker

# Several links and files, outputs in ~/stickers
conversor-stickers 'https://youtu.be/xxxx' cat.mp4 dog.mov --preset discord-gif -d ~/stickers
```

Outputs and previews are overwritten without asking, except when the name comes from a downloaded
video's title. In that case an existing file is not replaced: `<title>_1.webp`, `<title>_2.webp`...
is used. Ctrl-C cancels the conversion and leaves no partial files: output is written to a temporary
file and renamed when done.

With several inputs, each one is processed in order. A failure is reported as
`error en <input>: ...` on stderr and the next input is processed. The final line is
`N de M convertidas`, and the exit code is the code of the first failed input.

### Input formats

MP4, MOV, M4V, 3GP, MKV, WebM, AVI, animated GIF, FLV, MPEG-TS, MPEG, Ogg and WMV. The content is
checked, not the extension. Playlists that point to other files (`ffconcat`, HLS, DASH) and image
sequences are not read, for safety. Other formats fail with exit code 4.

### Links

Each link is downloaded with `yt-dlp` into its own temporary directory (mode 700 on Unix), which is
deleted afterwards, even on failure or cancellation. Then it is converted like a local file. Download
limits: no live streams, no playlists, up to 720p, at most 2 GB, and 10 minutes per download. The
output is named after the video's title, reduced to safe characters (letters, digits, `.`, `_`, `-`,
at most 60 characters). `yt-dlp` runs with the user's configuration and cookies disabled. Logs only
record the host of a link, never the full link.

### Presets

KB and MB are base 10 (500 KB = 500 000 bytes).

| Preset | Format | Canvas | Max size | Max duration |
|---|---|---|---|---|
| `whatsapp-sticker` | Animated WebP | 512x512 | 500 KB | 10 s |
| `discord-gif` | GIF | Square, side up to 480 px | 10 MB by default (`--max-mb`, up to 20) | 30 s |
| `discord-emoji` | GIF | 128x128 | 256 KB | 10 s |
| `discord-sticker` | APNG (`.png`) | 320x320 | 512 KB | 5 s |

If the result is too large, it is reduced in a fixed order (quality, fps, detail, colors and canvas
scale, depending on the preset). `Reducciones` in the summary says what was reduced. If it still does
not fit, the command fails with exit code 5 and suggests a shorter `--max-seconds` or another
`--start`.

WhatsApp rules: <https://github.com/WhatsApp/stickers/blob/main/Android/README.md>. Discord rules
come from its developer documentation (<https://docs.discord.com/developers/resources/sticker> and
<https://docs.discord.com/developers/resources/emoji>). Limits that Discord does not define (for
example the 480 px side of `discord-gif` or the 10 s emoji duration) are this project's decisions.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Other error (for example, the output could not be written) |
| 2 | Invalid arguments |
| 3 | Invalid or unreadable input |
| 4 | `ffmpeg` or `ffprobe` missing or failing |
| 5 | Does not fit the preset's size or duration |
| 6 | `ffmpeg`, `ffprobe` or `yt-dlp` took too long |
| 7 | `yt-dlp` missing, or the download failed |
| 130 | Cancelled with Ctrl-C |

## Desktop app

`app/` is a Tauri 2 application. It has the same conversion in one window: choose videos, a target,
a clip range and "Crear" (Create), then preview the result and save it.

### Linux requirements

- Everything the CLI needs.
- WebKitGTK 4.1 with its headers (`webkit2gtk-4.1` on Arch; `libwebkit2gtk-4.1-dev`,
  `librsvg2-dev`, `libssl-dev` and `libxdo-dev` on Debian or Ubuntu).
- For the video preview: GStreamer `gst-plugins-good` and an `ffmpeg` with the `libvpx` encoder.
  Without them the app works, but the preview is unavailable and the range is set with the slider.
- Node 26 with npm, and `tauri-cli` 2.11.4 (`cargo install tauri-cli --version 2.11.4 --locked`).

### Run and package

```sh
cd app
npm ci
cargo tauri dev      # runs the app with hot reload
cargo tauri build    # .deb and AppImage in target/release/bundle/
```

`npm run dev` opens the interface in a browser with sample data and a selector for each screen state.
It does not convert anything.

### Catalog, library and links

- **Library**: your own packs and categories, with favorites and a backup. "Exportar copia de
  seguridad" creates a `stickers-backup-YYYY-MM-DD.zip`. Restoring validates the whole backup in a
  temporary directory first, then replaces the current library. The current library is kept aside
  and a backup is made automatically before a restore.
- **Community**: packs from the public repository `conversor-stickers-comunidad`, read through the
  protocol described in [docs/CATALOG.md](docs/CATALOG.md). Only `https` is used. Each file is
  checked against its size and SHA-256, and the first download shows a warning, because third-party
  packs may contain errors. Proposing a pack creates a `.zip` for a pull request. It does not upload
  anything.
- **Links**: up to 100 items in the queue. They are downloaded one at a time with the same limits as
  the CLI, and the temporary video is deleted when it is removed from the queue or the app closes.
  The window does not load anything from the internet.
- **Ctrl-C, SIGTERM and SIGHUP** close the app and stop downloads. `nohup` does not protect it. Start
  it with `setsid -f conversor-app` if it must survive closing the terminal.

On NVIDIA with Wayland, WebKitGTK can show a black window. The app sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` for this case unless you set the variable yourself.

## Android

The Android app uses the desktop interface and downloads links the same way:

- **Links**: `yt-dlp` is packaged in the APK with Chaquopy (Python 3.14, `yt-dlp` pinned by version and
  hash in `app/src-tauri/gen/android/app/requirements-android.txt`). Each download is limited to 720p,
  200 MB and 10 minutes, and playlists are not downloaded.
- **Sharing**: receives "Compartir" from other apps. Videos and images go into the session, and text
  with links goes to the download queue.
- **Direct links**: `.mp4`, `.webm`, `.gif`, `.webp`, `.png` and `.jpg` are downloaded without `yt-dlp`,
  up to 100 MB.

To build the APK you need Python 3.14 installed on the machine, because Chaquopy uses it to install
`yt-dlp` with pip. FFmpeg for Android is built by `app/src-tauri/ffmpeg-android/build.sh`; see its
[README](app/src-tauri/ffmpeg-android/README.md).

## Release APK

Signed release APK for arm64 phones:

```sh
app/src-tauri/android-release.sh
```

The script reads the keystore password from the keyring (`secret-tool lookup aplicacion
conversor-stickers clave firma`) and exports `CONVERSOR_KEYSTORE` and `CONVERSOR_KEYSTORE_PASSWORD`
only for the build. Set `CONVERSOR_PROXY_URL` in the environment to use the KLIPY proxy (see "GIF and
sticker search"); `KLIPY_KEY` is deliberately unset by the script. Without those variables Gradle leaves the release APK unsigned. The APK ends up in
`app/src-tauri/gen/android/app/build/outputs/apk/universal/release/`.

The key lives outside the repository, in `~/.config/conversor-stickers/release.jks` (alias
`conversor-stickers`). If you lose the keystore or its password, new versions cannot be installed over
the old app: back both up.

## Ediciones: `github` y `play`

La variable `CONVERSOR_EDITION` elige la edición; por defecto es `github`.

- `github`: todo, incluidas las extensiones y la descarga de enlaces con yt-dlp.
- `play`: sin la sección Extensiones, sin descargar enlaces, sin Python/Chaquopy en el APK y sin esos
  comandos ni sus permisos.

```sh
CONVERSOR_EDITION=play app/src-tauri/android-release.sh
```

La misma variable la leen `build.rs` (cfg `edition = "github"`, y qué comandos y permisos existen),
Vite (flag de la interfaz) y Gradle (Chaquopy). Para comprobar `play` en Rust:
`CONVERSOR_EDITION=play cargo clippy -p conversor-app --all-targets -- -D warnings`.

## GIF and sticker search (KLIPY and GIPHY)

The **Crear** (Create) view has a GIF search tab for KLIPY or GIPHY. The chosen GIF goes into the
session like any file, and the resulting sticker goes into your library. The content filter is `pg-13`
by default.

API keys are looked up in this order:

1. Your own key, entered in the settings. It is stored in `providers.json` in the app configuration
   directory (mode 0600 on Unix) and is never logged. It takes priority.
2. A key built into the app at compile time, only from the `KLIPY_KEY` environment variable. Public
   releases must not set it: the key would be extractable from the binary.
3. The app's proxy (KLIPY only): the build-time variable `CONVERSOR_PROXY_URL` (an `https://` address)
   makes KLIPY requests go to `<proxy>/klipy/...` without any key; the Cloudflare Worker in
   [`proxy/`](proxy/README.md) holds the real key. `app/src-tauri/android-release.sh` reads
   `CONVERSOR_PROXY_URL` from the environment and ignores `KLIPY_KEY`.

Without a key or proxy the app still builds, and the search tab explains how to get one. Providers require their
attribution to be shown ("Powered by KLIPY" or "Powered by GIPHY"), and the app shows it. Packs with
KLIPY or GIPHY stickers cannot be proposed to the community, because their terms do not allow
redistribution.

## Project layout

```
core/        conversion library (conversor-core)
cli/         command-line tool (conversor-stickers)
app/         Tauri 2 app (conversor-app): src/ is the frontend, src-tauri/ the Rust backend
docs/        protocol documentation (CATALOG.md)
```

## Benchmark

```sh
cargo bench -p conversor-core
```

It measures `convert` from start to end for `whatsapp-sticker` and `discord-gif`, with the default
options, on a generated 1280x720, 30 fps, 10 s test video (stored in `target/videos-prueba/`).

## Licenses

The project is distributed under MIT OR Apache-2.0. Its main dependencies are permissive:

| Component | License | Use |
|---|---|---|
| `webp-animation`, `libwebp-sys2` (libwebp) | MIT OR Apache-2.0; BSD-3-Clause | WebP encoding (core) |
| `gif`, `color_quant`, `png` | MIT OR Apache-2.0; MIT | GIF and APNG encoding (core) |
| `fast_image_resize`, `rayon`, `thiserror`, `tracing`, `tempfile`, `rustix` | MIT or Apache-2.0 | Core dependencies |
| `clap`, `indicatif`, `ctrlc`, `tracing-subscriber` | MIT or Apache-2.0 | CLI |
| `tauri`, `tauri-plugin-dialog`, `zip`, `ureq`, `serde`, `serde_json` | Apache-2.0 OR MIT; MIT | Desktop app |
| `yt-dlp` | Unlicense | External process for links (desktop); packaged in the APK on Android |
| FFmpeg (`ffmpeg`, `ffprobe`) | LGPL-2.1+, or GPL if built with `--enable-gpl` (check with `ffmpeg -buildconf`) | External process; not linked into the project |
| Chaquopy / Python 3.14 | MIT / PSF-2.0 | Android: packages Python and pip into the APK |

FFmpeg is used as a separate program and communicates over pipes. The Android build uses an FFmpeg
built without GPL, nonfree or version3 options, so it stays LGPL 2.1 or later. Whoever shares an APK
also shares that FFmpeg source tarball.

Image encoders with other licenses (`imagequant`, GPL-3.0-or-later; `gifski`, AGPL-3.0-or-later) are
not used, because they would change the license of the binary.
