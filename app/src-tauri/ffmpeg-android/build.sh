#!/usr/bin/env bash
# Builds LGPL FFmpeg from the official signed tarball.
#
#   build.sh [--work DIR] [--output DIR] [--tarball FILE] [--host]
#
# Without --host it builds libffmpeg.so and libffprobe.so (PIE executables with that name so
# Gradle packages them) for arm64-v8a and x86_64, and leaves them in <output>/<abi>/. With --host
# it builds the same configuration for this Linux in <work>/host/, for the parity test
# core/tests/reduced_ffmpeg.rs.
#
#   --work DIR       download and build directory; it must not exist or must be empty
#                    (default: work/ next to this script)
#   --output DIR     destination of the .so files (default: jniLibs/ of the app's Android
#                    project)
#   --tarball PATH   use an already downloaded tarball and its PATH.asc signature instead of
#                    downloading them; they are verified all the same
#
# Nothing is extracted or built if the signature is not from the fingerprint in VERSION or if the
# tarball SHA-256 is not the one in VERSION.
set -euo pipefail

readonly SCRIPT_DIRECTORY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly ANDROID_ABIS=(arm64-v8a x86_64)

fail() {
    echo "build.sh: $*" >&2
    exit 1
}

version_value() {
    local key="$1" value
    value="$(sed -n "s/^${key}=//p" "$SCRIPT_DIRECTORY/VERSION")"
    [[ -n "$value" ]] || fail "${key} is missing in VERSION"
    printf '%s\n' "$value"
}

work="$SCRIPT_DIRECTORY/work"
tools=""
output="$SCRIPT_DIRECTORY/../gen/android/app/src/main/jniLibs"
local_tarball=""
host=false
while (($#)); do
    case "$1" in
        --work) work="${2:?--work needs a directory}"; shift 2 ;;
        --output) output="${2:?--output needs a directory}"; shift 2 ;;
        --tarball) local_tarball="${2:?--tarball needs a file}"; shift 2 ;;
        --host) host=true; shift ;;
        *) fail "unknown option: $1" ;;
    esac
done

FFMPEG_VERSION="$(version_value VERSION)"
TARBALL_URL="$(version_value URL)"
EXPECTED_SHA256="$(version_value SHA256)"
KEY_URL="$(version_value KEY_URL)"
FINGERPRINT="$(version_value FINGERPRINT)"
NDK_VERSION="$(version_value NDK)"
ANDROID_API="$(version_value ANDROID_API)"

# Fills `flags` with those of configuration.txt. No subshell, so `fail` stops the script.
read_common_flags() {
    local line
    flags=()
    while IFS= read -r line; do
        line="${line%%#*}"
        line="${line//[[:space:]]/}"
        [[ -n "$line" ]] || continue
        case "$line" in
            --enable-gpl | --enable-nonfree | --enable-version3)
                fail "configuration.txt cannot contain $line: FFmpeg must stay LGPL 2.1+" ;;
        esac
        flags+=("$line")
    done <"$SCRIPT_DIRECTORY/configuration.txt"
}

prepare_work() {
    if [[ -e "$work" ]]; then
        [[ -d "$work" && -z "$(ls -A "$work")" ]] ||
            fail "$work already exists and is not empty; use a new one"
    fi
    mkdir -p "$work"
    work="$(cd "$work" && pwd)"
}

download() {
    local destination="$1"
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        --output "$destination/$(basename "$TARBALL_URL")" "$TARBALL_URL"
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        --output "$destination/$(basename "$TARBALL_URL").asc" "$TARBALL_URL.asc"
}

# Requires a valid signature whose primary key is exactly FINGERPRINT. The key is imported into
# its own GNUPGHOME: the user's keyring does not count.
verify_signature() {
    local tarball="$1" signature="$1.asc" gnupg="$work/gnupg" status
    [[ -f "$tarball" && -f "$signature" ]] || fail "$tarball or $signature is missing"
    mkdir -m 700 "$gnupg"
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        --output "$gnupg/key.asc" "$KEY_URL"
    GNUPGHOME="$gnupg" gpg --batch --quiet --import "$gnupg/key.asc" 2>/dev/null ||
        fail "could not import the key from $KEY_URL"
    status="$(GNUPGHOME="$gnupg" gpg --batch --status-fd 1 --verify "$signature" "$tarball" 2>/dev/null)" ||
        fail "the signature of $(basename "$tarball") is not valid"
    if grep -qE '^\[GNUPG:\] (BADSIG|ERRSIG|EXPKEYSIG|REVKEYSIG)' <<<"$status"; then
        fail "the signature of $(basename "$tarball") is not valid"
    fi
    # VALIDSIG <subkey fingerprint> ... <primary key fingerprint>
    awk -v fingerprint="$FINGERPRINT" '$2 == "VALIDSIG" && $NF == fingerprint { ok = 1 } END { exit !ok }' \
        <<<"$status" || fail "the signature is not from key $FINGERPRINT"
    echo "Valid signature from $FINGERPRINT."
}

verify_sha256() {
    local tarball="$1" sum
    sum="$(sha256sum "$tarball" | cut -d' ' -f1)"
    [[ "$sum" == "$EXPECTED_SHA256" ]] ||
        fail "SHA-256 of $(basename "$tarball") is $sum; VERSION says $EXPECTED_SHA256"
    echo "SHA-256 correct: $sum."
}

# Leaves in `tools` the clang and binutils of the NDK pinned in VERSION.
find_ndk() {
    local ndk="${ANDROID_NDK_HOME:-${NDK_HOME:-}}" revision
    [[ -n "$ndk" ]] || fail "set ANDROID_NDK_HOME (or NDK_HOME) to NDK $NDK_VERSION"
    revision="$(sed -n 's/^Pkg.Revision *= *//p' "$ndk/source.properties" 2>/dev/null || true)"
    [[ "$revision" == "$NDK_VERSION" ]] ||
        fail "the NDK at $ndk is '$revision'; VERSION asks for $NDK_VERSION"
    tools="$ndk/toolchains/llvm/prebuilt/linux-x86_64"
}

# Adds the target ones to `flags`.
add_target_flags() {
    local target="$1" triple arch
    case "$target" in
        host)
            # No nasm: the separate assembler only speeds things up, it does not change the output.
            flags+=(--disable-x86asm)
            return ;;
        arm64-v8a) triple=aarch64-linux-android; arch=aarch64 ;;
        x86_64) triple=x86_64-linux-android; arch=x86_64; flags+=(--disable-x86asm) ;;
        *) fail "unknown target: $target" ;;
    esac
    flags+=(
        --enable-cross-compile --target-os=android --arch="$arch"
        --cc="$tools/bin/${triple}${ANDROID_API}-clang"
        --cxx="$tools/bin/${triple}${ANDROID_API}-clang++"
        --ar="$tools/bin/llvm-ar" --nm="$tools/bin/llvm-nm"
        --ranlib="$tools/bin/llvm-ranlib" --strip="$tools/bin/llvm-strip"
        --pkg-config=false
        --extra-ldflags=-Wl,-z,max-page-size=16384
    )
}

# Rejects the build if configure did not leave FFmpeg as LGPL.
check_license() {
    local config="$1/config.h" component
    for component in GPL NONFREE VERSION3; do
        grep -qx "#define CONFIG_${component} 0" "$config" ||
            fail "the configuration enabled CONFIG_${component}"
    done
}

# Every LOAD segment must be 16 KB aligned.
check_alignment() {
    local binary="$1" readelf="$2"
    "$readelf" -lW "$binary" | awk '
        $1 == "LOAD" { loads++; if ($NF != "0x4000") bad++ }
        END { exit !(loads > 0 && bad == 0) }' ||
        fail "$(basename "$binary") is not 16 KB aligned"
}

build() {
    local target="$1" source="$2" build_dir="$work/build-$1"
    local flags
    read_common_flags
    add_target_flags "$target"
    mkdir "$build_dir"
    echo "Configuring $target..."
    # Out-of-tree build: configure runs from build-<target>.
    (cd "$build_dir" && "$source/configure" "${flags[@]}" >configure.log 2>&1) ||
        fail "configure failed for $target; see $build_dir/configure.log and ffbuild/config.log"
    check_license "$build_dir"
    echo "Building $target..."
    make -C "$build_dir" -j"$(nproc)" ffmpeg ffprobe >"$build_dir/make.log" 2>&1 ||
        fail "make failed for $target; see $build_dir/make.log"
}

install_android() {
    local abi="$1" readelf="$tools/bin/llvm-readelf"
    mkdir -p "$output/$abi"
    install -m 755 "$work/build-$abi/ffmpeg" "$output/$abi/libffmpeg.so"
    install -m 755 "$work/build-$abi/ffprobe" "$output/$abi/libffprobe.so"
    check_alignment "$output/$abi/libffmpeg.so" "$readelf"
    check_alignment "$output/$abi/libffprobe.so" "$readelf"
    echo "$abi: $(cd "$output/$abi" && stat -c '%n %s bytes' libffmpeg.so libffprobe.so | paste -sd ', ')"
}

install_host() {
    local destination="$work/host"
    mkdir "$destination"
    install -m 755 "$work/build-host/ffmpeg" "$destination/ffmpeg"
    install -m 755 "$work/build-host/ffprobe" "$destination/ffprobe"
    echo "Host ready: CONVERSOR_FFMPEG_DIR=$destination"
}

main() {
    # Validates configuration.txt before downloading anything.
    local flags
    read_common_flags
    prepare_work
    local downloads="$work/downloads" tarball
    mkdir "$downloads"
    if [[ -n "$local_tarball" ]]; then
        cp -- "$local_tarball" "$downloads/$(basename "$TARBALL_URL")"
        cp -- "$local_tarball.asc" "$downloads/$(basename "$TARBALL_URL").asc"
    else
        download "$downloads"
    fi
    tarball="$downloads/$(basename "$TARBALL_URL")"
    verify_signature "$tarball"
    verify_sha256 "$tarball"

    mkdir "$work/source"
    tar -xJf "$tarball" -C "$work/source"
    local source="$work/source/ffmpeg-$FFMPEG_VERSION"
    [[ -x "$source/configure" ]] || fail "the tarball does not contain ffmpeg-$FFMPEG_VERSION/configure"

    if $host; then
        build host "$source"
        install_host
    else
        local abi
        find_ndk
        for abi in "${ANDROID_ABIS[@]}"; do
            build "$abi" "$source"
            install_android "$abi"
        done
    fi
}

main
