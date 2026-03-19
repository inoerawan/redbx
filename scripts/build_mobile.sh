#!/usr/bin/env bash
# build_mobile.sh — build redbx-mobile for Android and/or iOS
#
# Usage:
#   bash scripts/build_mobile.sh              # both platforms
#   bash scripts/build_mobile.sh android      # Android only
#   bash scripts/build_mobile.sh ios          # iOS only
#
# Outputs:
#   Android: crates/redbx-mobile/android/jniLibs/<abi>/libredbx_mobile.so
#            crates/redbx-mobile/android/kotlin/   (UniFFI Kotlin bindings)
#   iOS:     crates/redbx-mobile/ios/lib/          (.a per target)
#            crates/redbx-mobile/ios/RedbxMobile.xcframework
#            crates/redbx-mobile/ios/swift/        (UniFFI Swift bindings)
set -euo pipefail

# ── Helpers ───────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BOLD='\033[1m'; NC='\033[0m'
info()    { printf "${BOLD}[build_mobile]${NC} %s\n" "$*"; }
success() { printf "${GREEN}${BOLD}✅ %s${NC}\n" "$*"; }
warn()    { printf "${YELLOW}⚠️  %s${NC}\n" "$*"; }
die()     { printf "${RED}${BOLD}❌ %s${NC}\n" "$*" >&2; exit 1; }

TARGET="${1:-all}"   # android | ios | all

# ── Pre-flight ────────────────────────────────────────────────────────────────

command -v cargo    &>/dev/null || die "cargo not found — install Rust via rustup"
command -v rustup   &>/dev/null || die "rustup not found"

# ── Android build ─────────────────────────────────────────────────────────────

build_android() {
    info "Building Android libraries..."

    command -v cargo-ndk &>/dev/null \
        || die "cargo-ndk not found — run: cargo install cargo-ndk"

    if [[ -z "${ANDROID_NDK_HOME:-}" && -z "${NDK_HOME:-}" ]]; then
        die "Neither ANDROID_NDK_HOME nor NDK_HOME is set — install Android NDK and export the path"
    fi

    ANDROID_OUT="crates/redbx-mobile/android"
    JNILIBS="$ANDROID_OUT/jniLibs"
    KOTLIN_OUT="$ANDROID_OUT/kotlin"
    mkdir -p "$JNILIBS" "$KOTLIN_OUT"

    info "Compiling for all Android ABIs (release)..."
    cargo ndk \
        -t aarch64-linux-android \
        -t armv7-linux-androideabi \
        -t x86_64-linux-android \
        -t i686-linux-android \
        -o "$JNILIBS" \
        build -p redbx-mobile --release

    # Use the arm64 lib for bindgen (all ABIs expose the same API)
    ANDROID_LIB="target/aarch64-linux-android/release/libredbx_mobile.so"
    [[ -f "$ANDROID_LIB" ]] || die "Expected lib not found: $ANDROID_LIB"

    info "Generating Kotlin bindings..."
    cargo run -p uniffi-bindgen -- generate \
        --library "$ANDROID_LIB" \
        --language kotlin \
        --out-dir "$KOTLIN_OUT"

    success "Android build complete"
    info "  JNI libs : $JNILIBS/"
    info "  Kotlin   : $KOTLIN_OUT/"
    find "$JNILIBS" -name "*.so" | sort | while read -r f; do
        printf "             %s (%s)\n" "$f" "$(du -sh "$f" | cut -f1)"
    done
}

# ── iOS build ─────────────────────────────────────────────────────────────────

build_ios() {
    info "Building iOS libraries..."

    command -v swift &>/dev/null \
        || die "swift not found — install Swift for Linux: https://www.swift.org/download"
    command -v xtool &>/dev/null \
        || die "xtool not found — see: https://github.com/nicholaslightle/xtool"

    # iOS SDK check — Apple only ships the iOS SDK inside Xcode (macOS).
    # On Linux you must set SDKROOT to an extracted iPhoneOS.sdk path.
    # Without it, Rust's linker calls xcrun (macOS-only) and fails.
    if [[ -z "${SDKROOT:-}" ]]; then
        warn "SDKROOT is not set."
        warn "The iOS SDK (iPhoneOS.sdk) is required for a full iOS build."
        warn ""
        warn "Options:"
        warn "  macOS: export SDKROOT=\$(xcrun --sdk iphoneos --show-sdk-path)"
        warn "  Linux: xtool sdk install /path/to/Xcode.xip   # extracts the SDK"
        warn "         then set SDKROOT to the installed sdk path"
        warn ""
        warn "Falling back to cargo check (type-check only — no .a produced)."
        echo
        info "Type-checking for aarch64-apple-ios..."
        cargo check -p redbx-mobile --target aarch64-apple-ios
        info "Type-checking for aarch64-apple-ios-sim..."
        cargo check -p redbx-mobile --target aarch64-apple-ios-sim
        success "iOS type-check passed (no SDKROOT — skipped link + bindgen)"
        return 0
    fi

    if [[ ! -d "$SDKROOT" ]]; then
        die "SDKROOT='$SDKROOT' does not exist — set it to a valid iPhoneOS.sdk path"
    fi

    info "Using iOS SDK: $SDKROOT"

    IOS_OUT="crates/redbx-mobile/ios"
    IOS_LIB="$IOS_OUT/lib"
    SWIFT_OUT="$IOS_OUT/swift"
    XCF_OUT="$IOS_OUT/RedbxMobile.xcframework"
    mkdir -p "$IOS_LIB" "$SWIFT_OUT"

    info "Compiling for aarch64-apple-ios (device)..."
    SDKROOT="$SDKROOT" cargo build -p redbx-mobile --release --target aarch64-apple-ios

    info "Compiling for aarch64-apple-ios-sim (simulator)..."
    SDKROOT="$SDKROOT" cargo build -p redbx-mobile --release --target aarch64-apple-ios-sim

    cp target/aarch64-apple-ios/release/libredbx_mobile.a \
        "$IOS_LIB/libredbx_mobile-device.a"
    cp target/aarch64-apple-ios-sim/release/libredbx_mobile.a \
        "$IOS_LIB/libredbx_mobile-sim.a"

    info "Generating Swift bindings..."
    cargo run -p uniffi-bindgen -- generate \
        --library "$IOS_LIB/libredbx_mobile-device.a" \
        --language swift \
        --out-dir "$SWIFT_OUT"

    info "Creating XCFramework..."
    rm -rf "$XCF_OUT"
    xtool project create-xcframework \
        --library "$IOS_LIB/libredbx_mobile-device.a" \
        --library "$IOS_LIB/libredbx_mobile-sim.a" \
        --output "$XCF_OUT"

    success "iOS build complete"
    info "  Static libs : $IOS_LIB/"
    info "  Swift       : $SWIFT_OUT/"
    info "  XCFramework : $XCF_OUT"
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

case "$TARGET" in
    android) build_android ;;
    ios)     build_ios ;;
    all)
        build_android
        echo
        build_ios
        ;;
    *)
        die "Unknown target '$TARGET'. Use: android | ios | all"
        ;;
esac
