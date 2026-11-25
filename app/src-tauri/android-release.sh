#!/usr/bin/env bash
# Builds the signed release APK (arm64). The keystore password is read from the
# user keyring and exported only for this build process.
set -euo pipefail

keystore="${CONVERSOR_KEYSTORE:-$HOME/.config/conversor-stickers/release.jks}"
[ -f "$keystore" ] || { echo "Falta el almacén de claves: $keystore" >&2; exit 1; }
password="$(secret-tool lookup aplicacion conversor-stickers clave firma)"
[ -n "$password" ] || { echo "No hay contraseña de firma en el llavero" >&2; exit 1; }

# A release must never embed the KLIPY key; KLIPY goes through CONVERSOR_PROXY_URL (from the
# environment, https only) instead.
unset KLIPY_KEY

# CONVERSOR_EDITION=github (default) or play: build.rs (Cargo), Vite and Gradle all read it.
export CONVERSOR_EDITION="${CONVERSOR_EDITION:-github}"
case "$CONVERSOR_EDITION" in
    github|play) ;;
    *) echo "CONVERSOR_EDITION debe ser github o play, no '$CONVERSOR_EDITION'" >&2; exit 1 ;;
esac

cd "$(dirname "$0")"
CONVERSOR_ARM64_ONLY=1 CONVERSOR_KEYSTORE="$keystore" CONVERSOR_KEYSTORE_PASSWORD="$password" \
    exec cargo tauri android build --apk --target aarch64 "$@"
