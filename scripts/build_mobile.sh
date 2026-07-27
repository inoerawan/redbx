#!/usr/bin/env bash
# build_mobile.sh — build redbx-mobile for Android and generate its Kotlin bindings.
#
# Usage:
#   bash scripts/build_mobile.sh              # Android (the only supported target)
#   bash scripts/build_mobile.sh android
#   bash scripts/build_mobile.sh android --debug
#
# Outputs (all gitignored — regenerate rather than commit):
#   android/redbx/src/main/jniLibs/<abi>/libredbx_mobile.so
#   android/redbx/src/generated/kotlin/id/inoerawan/redbx/redbx_mobile.kt
#
# iOS support was removed pending a macOS build host. Add a build_ios() here when
# that lands; do not reintroduce the Linux cross-compilation path.
set -euo pipefail

# ── Helpers ───────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

RED='\033[0;31m'; GREEN='\033[0;32m'; BOLD='\033[1m'; NC='\033[0m'
info()    { printf "${BOLD}[build_mobile]${NC} %s\n" "$*"; }
success() { printf "${GREEN}${BOLD}✅ %s${NC}\n" "$*"; }
die()     { printf "${RED}${BOLD}❌ %s${NC}\n" "$*" >&2; exit 1; }

TARGET="${1:-android}"
PROFILE_FLAG="--release"
PROFILE_DIR="release"
if [[ "${2:-}" == "--debug" ]]; then
    PROFILE_FLAG=""
    PROFILE_DIR="debug"
fi

# Keep in sync with `minSdk` in android/redbx/build.gradle.kts.
ANDROID_MIN_SDK=24

ANDROID_MODULE="android/redbx"
JNILIBS="$ANDROID_MODULE/src/main/jniLibs"
KOTLIN_OUT="$ANDROID_MODULE/src/generated/kotlin"
UNIFFI_CONFIG="crates/redbx-mobile/uniffi.toml"

# Locate an Android NDK the same way cargo-ndk does: explicit env first, then the
# newest one installed under the SDK. Prints the path, or returns 1.
resolve_ndk() {
    local candidate
    for candidate in "${ANDROID_NDK_HOME:-}" "${NDK_HOME:-}"; do
        if [[ -n "$candidate" && -d "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    local sdk
    for sdk in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Android/Sdk"; do
        [[ -n "$sdk" && -d "$sdk/ndk" ]] || continue
        # Highest version number wins.
        candidate="$(find "$sdk/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -1)"
        if [[ -n "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    return 1
}

# ── Pre-flight ────────────────────────────────────────────────────────────────

command -v cargo  &>/dev/null || die "cargo not found — install Rust via rustup"
command -v rustup &>/dev/null || die "rustup not found"

# ── Android build ─────────────────────────────────────────────────────────────

build_android() {
    info "Building Android libraries (profile: $PROFILE_DIR, minSdk: $ANDROID_MIN_SDK)..."

    command -v cargo-ndk &>/dev/null \
        || die "cargo-ndk not found — run: cargo install cargo-ndk"

    # cargo-ndk 4.x discovers an NDK under the SDK on its own, so only fail when
    # nothing is findable — requiring ANDROID_NDK_HOME would reject a working
    # Android Studio install.
    local ndk
    ndk="$(resolve_ndk || true)"
    if [[ -n "$ndk" ]]; then
        info "Using NDK: $ndk"
    else
        die "No Android NDK found. Install one (Android Studio → SDK Manager → NDK, or
   sdkmanager --install 'ndk;<version>') or set ANDROID_NDK_HOME=/path/to/ndk"
    fi

    # Wipe stale artifacts so a removed ABI cannot linger in the AAR.
    rm -rf "$JNILIBS" "$KOTLIN_OUT"
    mkdir -p "$JNILIBS" "$KOTLIN_OUT"

    # Android 15+ requires shared libraries to tolerate a 16 KB page size. NDK r28
    # defaults to this; setting it explicitly keeps r27 correct too. Scoped to this
    # one command so it does not leak into the host build of uniffi-bindgen below
    # (which would both add a pointless flag and bust its build cache).
    local android_rustflags="${RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384"

    info "Compiling for all Android ABIs..."
    # shellcheck disable=SC2086  # PROFILE_FLAG is intentionally word-split (may be empty)
    RUSTFLAGS="$android_rustflags" cargo ndk \
        --platform "$ANDROID_MIN_SDK" \
        -t arm64-v8a \
        -t armeabi-v7a \
        -t x86_64 \
        -t x86 \
        -o "$JNILIBS" \
        build -p redbx-mobile $PROFILE_FLAG

    # Bindings are generated from one ABI; all ABIs expose the same API.
    ANDROID_LIB="target/aarch64-linux-android/$PROFILE_DIR/libredbx_mobile.so"
    [[ -f "$ANDROID_LIB" ]] || die "Expected lib not found: $ANDROID_LIB"

    info "Generating Kotlin bindings..."
    cargo run -p uniffi-bindgen -- generate \
        --library "$ANDROID_LIB" \
        --language kotlin \
        --config "$UNIFFI_CONFIG" \
        --out-dir "$KOTLIN_OUT"

    success "Android build complete"
    info "  JNI libs : $JNILIBS/"
    info "  Kotlin   : $KOTLIN_OUT/"
    find "$JNILIBS" -name "*.so" | sort | while read -r f; do
        printf "             %s (%s)\n" "$f" "$(du -sh "$f" | cut -f1)"
    done
    echo
    info "Next: build the AAR with  cd android && ./gradlew :redbx:assembleRelease"
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

case "$TARGET" in
    android) build_android ;;
    ios)     die "iOS builds were removed — they will be reinstated on a macOS host" ;;
    all)     build_android ;;
    *)       die "Unknown target '$TARGET'. Use: android" ;;
esac
