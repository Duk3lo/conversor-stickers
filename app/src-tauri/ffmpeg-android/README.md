# FFmpeg for Android

The Android app decodes with FFmpeg 9.1, built here from the official tarball and packaged as two
executables, `libffmpeg.so` and `libffprobe.so`, for `arm64-v8a` and `x86_64`. The core launches
them just like on desktop, with `FfmpegDecoder::with_executables`.

## Files

- `build.sh`: downloads, verifies and builds.
- `VERSION`: version, URL, tarball SHA-256, fingerprint of the FFmpeg release key, NDK version and
  minimum API.
- `configuration.txt`: the exact `configure` flags (there are no patches).
- `binaries.sha256`: sums of the `.so` files that come out with the NDK in `VERSION`. The build is
  reproducible: two builds in different directories give the same bytes.

## Building

You need `curl`, `gpg`, `make`, a C compiler for `--host` and NDK 30.0.16248370 in
`ANDROID_NDK_HOME` (or `NDK_HOME`).

```sh
app/src-tauri/ffmpeg-android/build.sh                 # arm64-v8a and x86_64
app/src-tauri/ffmpeg-android/build.sh --host          # Linux, for the parity test
```

- By default it downloads and builds in `work/` (ignored by git), which must not exist or must be
  empty; `--work DIR` uses another one.
- The `.so` files go to `../gen/android/app/src/main/jniLibs/<abi>/` (ignored by git);
  `--output DIR` leaves them elsewhere.
- `--tarball PATH` uses an already downloaded tarball (and `PATH.asc`).
- Before extracting anything it requires the signature to be from key
  `FCF986EA15E6E293A5644F10B4322F04D67658D8` (imported from <https://ffmpeg.org/ffmpeg-devel.asc>
  into its own keyring) and the SHA-256 to be the one in `VERSION`. It also fails if
  `configuration.txt` asks for GPL, nonfree or version3, if `configure` enables them, or if a
  `LOAD` segment of a `.so` is not 16 KB aligned.

To check the binaries:

```sh
(cd app/src-tauri/gen/android/app/src/main/jniLibs && sha256sum -c ../../../../../../ffmpeg-android/binaries.sha256)
```

## Stream-copy muxing

For «save video» (yt-dlp merges video and audio without re-encoding) it also has the muxers `mp4`,
`mov`, `matroska` and `webm`, the `aac_adtstoasc` and `extract_extradata` bitstream filters, the
parsers of the codecs they carry, the `aac` and `mp3` demuxers and the `aac` and `mp3float`
decoders (FFmpeg needs them to learn the sample rate of ADTS AAC in MPEG-TS). No encoders were
added and it stays LGPL. Size change (bytes), before to after:

| Binary | arm64-v8a | x86_64 |
| --- | --- | --- |
| `libffmpeg.so` | 6632328 to 7656936 | 7433768 to 8494152 |
| `libffprobe.so` | 6441280 to 7454624 | 7220768 to 8269104 |

## Framing filters

Framing «Encajar», the blurred background and the shape masks need the filters `pad`, `split`,
`overlay`, `gblur` and `geq`; without them Fit never worked on Android. The blur is `gblur`
because `boxblur` is GPL-only in FFmpeg and this build must stay LGPL (configure drops it
silently). Size change (bytes), before to after:

| Binary | arm64-v8a | x86_64 |
| --- | --- | --- |
| `libffmpeg.so` | 7656936 to 7751624 | 8494152 to 8609192 |
| `libffprobe.so` | 7454624 to 7549344 | 8269104 to 8384192 |

Saving videos on Android needs Android 10+ (MediaStore `RELATIVE_PATH`); the minimum SDK is 24, so older
versions get the error «guardar vídeos necesita Android 10 o superior».

## Parity test

With the host executables, the core has to decode videos and images the same as with the system
`ffmpeg`, and reject a trap video:

```sh
CONVERSOR_FFMPEG_DIR=<work>/host cargo test -p conversor-core --test reduced_ffmpeg
```

Without `CONVERSOR_FFMPEG_DIR` the tests do nothing.

## License

FFmpeg is built without `--enable-gpl`, `--enable-nonfree` or `--enable-version3`, so it stays
**LGPL 2.1 or later**. It ships as a separate program that talks over pipes and is not linked into
the app code. Source: <https://ffmpeg.org/releases/ffmpeg-9.1.tar.xz>. Whoever shares an APK also
shares that tarball.
