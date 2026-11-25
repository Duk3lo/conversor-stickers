#!/usr/bin/env bash
# Builds the signed AAB of the Google Play edition (arm64-v8a and x86_64; armeabi-v7a has no
# Python 3.14). The keystore password is read from the user keyring, like android-release.sh.
# The edition has no yt-dlp downloads: Cargo feature `play` + VITE_EDITION=play (UI) +
# CONVERSOR_PLAY=1 (Gradle: violet icon, no yt-dlp in the bundle).
#
#   CONVERSOR_PROXY_URL=https://<proxy>.workers.dev ./android-play.sh
#
# The AAB is copied to $CONVERSOR_PLAY_DIR (default ~/Escritorio/play).
set -euo pipefail

keystore="${CONVERSOR_KEYSTORE:-$HOME/.config/conversor-stickers/release.jks}"
[ -f "$keystore" ] || { echo "Falta el almacén de claves: $keystore" >&2; exit 1; }
password="$(secret-tool lookup aplicacion conversor-stickers clave firma)"
[ -n "$password" ] || { echo "No hay contraseña de firma en el llavero" >&2; exit 1; }

# KLIPY goes through the proxy; a release must never embed the key.
unset KLIPY_KEY
proxy="${CONVERSOR_PROXY_URL:-}"
case "$proxy" in
    https://?*) ;;
    *) echo "CONVERSOR_PROXY_URL es obligatoria y debe empezar por https://" >&2; exit 1 ;;
esac

cd "$(dirname "$0")"
version="$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' tauri.conf.json | head -n 1)"
[ -n "$version" ] || { echo "No se pudo leer la versión de tauri.conf.json" >&2; exit 1; }
destination="${CONVERSOR_PLAY_DIR:-$HOME/Escritorio/play}"

# CONVERSOR_ARM64_ONLY must stay unset: the AAB carries both ABIs.
unset CONVERSOR_ARM64_ONLY
CONVERSOR_PLAY=1 VITE_EDITION=play CONVERSOR_PROXY_URL="$proxy" \
    CONVERSOR_KEYSTORE="$keystore" CONVERSOR_KEYSTORE_PASSWORD="$password" \
    cargo tauri android build --aab --target aarch64 --target x86_64 --features play "$@"

aab="$(find gen/android/app/build/outputs/bundle -name '*.aab' -path '*elease*' -newer tauri.conf.json | head -n 1)"
[ -n "$aab" ] || { echo "No se encontró el AAB generado" >&2; exit 1; }
mkdir -p "$destination"
cp "$aab" "$destination/ConversorStickers-$version-play.aab"
echo "AAB: $destination/ConversorStickers-$version-play.aab"
