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

    ON_MACOS=false
    [[ "$(uname)" == "Darwin" ]] && ON_MACOS=true

    # ── Platform-specific pre-flight ──────────────────────────────────────────
    if $ON_MACOS; then
        command -v xcodebuild &>/dev/null \
            || die "xcodebuild not found — install Xcode from the App Store"

        # Auto-detect SDKROOT from Xcode if not already set
        if [[ -z "${SDKROOT:-}" ]]; then
            SDKROOT=$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null || true)
            [[ -n "$SDKROOT" ]] \
                || die "Could not detect iOS SDK — make sure Xcode and the iOS platform are installed"
            info "Auto-detected iOS SDK: $SDKROOT"
        fi
    else
        # Linux: requires swift + xtool with a manually installed iOS SDK
        command -v swift &>/dev/null \
            || die "swift not found — install Swift for Linux: https://www.swift.org/download"
        command -v xtool &>/dev/null \
            || die "xtool not found — see: https://github.com/nicholaslightle/xtool"

        # iOS SDK check — Apple does not ship the iOS SDK for Linux.
        # Install via: xtool sdk install /path/to/Xcode.xip
        # then set SDKROOT to the printed sdk path.
        if [[ -z "${SDKROOT:-}" ]]; then
            warn "SDKROOT is not set."
            warn "The iOS SDK (iPhoneOS.sdk) is required for a full iOS build."
            warn ""
            warn "  xtool sdk install /path/to/Xcode.xip   # extracts the SDK"
            warn "  then: export SDKROOT=<path printed by xtool>"
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
    fi

    if [[ ! -d "$SDKROOT" ]]; then
        die "SDKROOT='$SDKROOT' does not exist — set it to a valid iPhoneOS.sdk path"
    fi

    info "Using iOS SDK: $SDKROOT"

    # On Linux, derive the xtool-bundled ld64.lld from $SDKROOT and inject linker
    # config at build time so .cargo/config.toml stays portable across platforms.
    # SDKROOT = <bundle>/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk
    # Going up 6 dirs reaches the bundle root where toolset/bin/ld64.lld lives.
    IOS_CARGO_CONFIG=()
    if ! $ON_MACOS; then
        XTOOL_BUNDLE="$HOME/.swiftpm/swift-sdks/darwin.artifactbundle"
        XTOOL_LD64="$XTOOL_BUNDLE/toolset/bin/ld64.lld"
        [[ -x "$XTOOL_LD64" ]] \
            || die "Could not find ld64.lld in xtool bundle at '$XTOOL_LD64' — is the SDK fully installed?"
        info "Using linker: $XTOOL_LD64"
        IOS_CARGO_CONFIG=(
            --config "target.aarch64-apple-ios.linker=\"$XTOOL_LD64\""
            --config "target.aarch64-apple-ios.rustflags=['-C','linker-flavor=ld64.lld']"
            --config "target.aarch64-apple-ios-sim.linker=\"$XTOOL_LD64\""
            --config "target.aarch64-apple-ios-sim.rustflags=['-C','linker-flavor=ld64.lld']"
        )
    fi

    IOS_OUT="crates/redbx-mobile/ios"
    IOS_LIB="$IOS_OUT/lib"
    SWIFT_OUT="$IOS_OUT/swift"
    XCF_OUT="$IOS_OUT/RedbxMobile.xcframework"
    mkdir -p "$IOS_LIB" "$SWIFT_OUT"

    info "Compiling for aarch64-apple-ios (device)..."
    SDKROOT="$SDKROOT" cargo build -p redbx-mobile --release --target aarch64-apple-ios \
        "${IOS_CARGO_CONFIG[@]}"

    info "Compiling for aarch64-apple-ios-sim (simulator)..."
    SDKROOT_SIM="${SDKROOT_SIM:-$(dirname "$SDKROOT")/../../../iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk}"
    SDKROOT_SIM="$(cd "$SDKROOT_SIM" 2>/dev/null && pwd || echo "$SDKROOT_SIM")"
    if [[ ! -d "$SDKROOT_SIM" ]]; then
        die "Could not find iPhoneSimulator.sdk — set SDKROOT_SIM explicitly"
    fi
    info "Using iOS Simulator SDK: $SDKROOT_SIM"
    SDKROOT="$SDKROOT_SIM" cargo build -p redbx-mobile --release --target aarch64-apple-ios-sim \
        "${IOS_CARGO_CONFIG[@]}"

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
    if $ON_MACOS; then
        xcodebuild -create-xcframework \
            -library "$IOS_LIB/libredbx_mobile-device.a" \
            -library "$IOS_LIB/libredbx_mobile-sim.a" \
            -output "$XCF_OUT"
    else
        # On Linux, xcodebuild is unavailable and xtool does not support xcframework
        # creation. Build the directory structure manually — an XCFramework is just
        # a versioned directory tree + Info.plist understood by Xcode / SPM.
        DEVICE_DIR="$XCF_OUT/ios-arm64"
        SIM_DIR="$XCF_OUT/ios-arm64-simulator"
        mkdir -p "$DEVICE_DIR" "$SIM_DIR"
        cp "$IOS_LIB/libredbx_mobile-device.a" "$DEVICE_DIR/libredbx_mobile.a"
        cp "$IOS_LIB/libredbx_mobile-sim.a"    "$SIM_DIR/libredbx_mobile.a"

        cat > "$XCF_OUT/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>AvailableLibraries</key>
    <array>
        <dict>
            <key>LibraryIdentifier</key>
            <string>ios-arm64</string>
            <key>LibraryPath</key>
            <string>libredbx_mobile.a</string>
            <key>SupportedArchitectures</key>
            <array><string>arm64</string></array>
            <key>SupportedPlatform</key>
            <string>ios</string>
        </dict>
        <dict>
            <key>LibraryIdentifier</key>
            <string>ios-arm64-simulator</string>
            <key>LibraryPath</key>
            <string>libredbx_mobile.a</string>
            <key>SupportedArchitectures</key>
            <array><string>arm64</string></array>
            <key>SupportedPlatform</key>
            <string>ios</string>
            <key>SupportedPlatformVariant</key>
            <string>simulator</string>
        </dict>
    </array>
    <key>CFBundlePackageType</key>
    <string>XFWK</string>
    <key>XCFrameworkFormatVersion</key>
    <string>1.0</string>
</dict>
</plist>
PLIST
    fi

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
