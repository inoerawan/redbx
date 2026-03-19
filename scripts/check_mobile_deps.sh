#!/usr/bin/env bash
# check_mobile_deps.sh — verify all dependencies needed to build redbx-mobile
# for Android and iOS. Prints ✅/❌ per item and exits non-zero if any required
# dependency is missing.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

MISSING=0

ok()      { printf "  ${GREEN}✅${NC} %s\n" "$1"; }
fail()    { printf "  ${RED}❌${NC} %s ${RED}— not found${NC}\n" "$1"; printf "     ${YELLOW}Install:${NC} %s\n" "$2"; MISSING=$((MISSING+1)); }
section() { printf "\n${BOLD}%s${NC}\n" "$1"; }

check_cmd() {
    local name="$1" hint="$2"
    if command -v "$name" &>/dev/null; then
        ok "$name ($($name --version 2>&1 | head -1))"
    else
        fail "$name" "$hint"
    fi
}

check_rust_target() {
    local target="$1" purpose="$2"
    if rustup target list --installed 2>/dev/null | grep -q "^${target}$"; then
        ok "rust target: $target ($purpose)"
    else
        fail "rust target: $target ($purpose)" "rustup target add $target"
    fi
}

check_env() {
    local name="$1" hint="$2"
    if [[ -n "${!name:-}" ]]; then
        ok "\$$name = ${!name}"
    else
        fail "\$$name" "$hint"
    fi
}

# ── Core ──────────────────────────────────────────────────────────────────────
section "Core"

check_cmd "rustup" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
check_cmd "rustc"  "rustup install stable"
check_cmd "cargo"  "rustup install stable"
check_cmd "just"   "cargo install just"

# Rust version check
MIN_RUST="1.89"
if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version | sed 's/rustc \([0-9.]*\).*/\1/')
    if printf '%s\n%s\n' "$MIN_RUST" "$RUST_VER" | sort -V -C; then
        ok "Rust >= $MIN_RUST (found $RUST_VER)"
    else
        fail "Rust >= $MIN_RUST (found $RUST_VER)" "rustup update"
    fi
fi

# uniffi-bindgen — workspace binary (must match uniffi dep version in redbx-mobile)
# Usage: cargo run -p uniffi-bindgen -- generate --library <lib> --language kotlin|swift --out-dir <dir>
WORKSPACE_ROOT="$(dirname "$(dirname "$(realpath "$0")")")"
BINDGEN_DEBUG="$WORKSPACE_ROOT/target/debug/uniffi-bindgen"
BINDGEN_RELEASE="$WORKSPACE_ROOT/target/release/uniffi-bindgen"
BINDGEN_SRC="$WORKSPACE_ROOT/crates/uniffi-bindgen/src/main.rs"
if [[ -x "$BINDGEN_RELEASE" ]] || [[ -x "$BINDGEN_DEBUG" ]]; then
    ok "uniffi-bindgen (workspace binary, built)"
elif [[ -f "$BINDGEN_SRC" ]]; then
    ok "uniffi-bindgen (workspace crate present — run: cargo build -p uniffi-bindgen)"
else
    fail "uniffi-bindgen" "crates/uniffi-bindgen missing — re-run workspace setup"
fi

# ── Android ───────────────────────────────────────────────────────────────────
section "Android"

check_cmd "java"      "Install JDK 17+: https://adoptium.net"
check_cmd "gradle"    "Install Gradle: https://gradle.org/install or use wrapper"
check_cmd "cargo-ndk" "cargo install cargo-ndk"

# Android NDK
if [[ -n "${ANDROID_NDK_HOME:-}" ]]; then
    ok "\$ANDROID_NDK_HOME = $ANDROID_NDK_HOME"
elif [[ -n "${NDK_HOME:-}" ]]; then
    ok "\$NDK_HOME = $NDK_HOME"
else
    fail "\$ANDROID_NDK_HOME or \$NDK_HOME" \
         "Install Android NDK and set ANDROID_NDK_HOME=/path/to/ndk"
fi

check_rust_target "aarch64-linux-android"    "ARM64 devices"
check_rust_target "armv7-linux-androideabi"  "ARM 32-bit devices"
check_rust_target "x86_64-linux-android"     "x86_64 emulator"
check_rust_target "i686-linux-android"       "x86 emulator"

# ── iOS ───────────────────────────────────────────────────────────────────────
if [[ "$(uname)" == "Darwin" ]]; then
    section "iOS (macOS / Xcode)"

    check_cmd "xcodebuild" "Install Xcode from the App Store: https://apps.apple.com/app/xcode/id497799835"

    check_rust_target "aarch64-apple-ios"      "physical iOS devices"
    check_rust_target "aarch64-apple-ios-sim"  "iOS simulator (Apple Silicon)"

    # Auto-detect SDKROOT on macOS via xcrun; respect a user-set value if present.
    if [[ -n "${SDKROOT:-}" ]]; then
        if [[ -d "${SDKROOT}" ]]; then
            ok "\$SDKROOT = $SDKROOT"
        else
            fail "\$SDKROOT (directory does not exist)" \
                 "Set SDKROOT to a valid iPhoneOS.sdk path, or unset it to let xcrun auto-detect"
        fi
    elif command -v xcrun &>/dev/null; then
        SDKROOT_AUTO=$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null || true)
        if [[ -n "$SDKROOT_AUTO" ]] && [[ -d "$SDKROOT_AUTO" ]]; then
            ok "\$SDKROOT (auto-detected via xcrun): $SDKROOT_AUTO"
        else
            fail "\$SDKROOT" \
                 "Install Xcode + iOS SDK; then: export SDKROOT=\$(xcrun --sdk iphoneos --show-sdk-path)"
        fi
    else
        fail "xcrun / Xcode" "Install Xcode from the App Store: https://apps.apple.com/app/xcode/id497799835"
    fi
else
    section "iOS (Linux via xtool + Swift)"

    check_cmd "swift"  "Install Swift for Linux: https://www.swift.org/download"
    check_cmd "xtool"  "Install xtool: https://github.com/nicholaslightle/xtool — see README"

    check_rust_target "aarch64-apple-ios"      "physical iOS devices"
    check_rust_target "aarch64-apple-ios-sim"  "iOS simulator (Apple Silicon)"

    # iOS SDK (SDKROOT) — Apple does not ship the iOS SDK for Linux.
    # Options:
    #   A) Extract from Xcode.xip on macOS, install via: xtool sdk install /path/to/Xcode.xip
    #      Then set SDKROOT to the path printed by xtool after install.
    #   B) Use GitHub Actions macos-latest runner for production iOS builds.
    #   C) cargo check (type-check) works without SDKROOT; full link requires it.
    if [[ -n "${SDKROOT:-}" ]]; then
        if [[ -d "${SDKROOT}" ]]; then
            ok "\$SDKROOT = $SDKROOT"
        else
            fail "\$SDKROOT (directory does not exist)" \
                 "Set SDKROOT to a valid iPhoneOS.sdk path (see notes above)"
        fi
    else
        # Non-fatal on Linux: cargo check still works without SDKROOT
        printf "  ${YELLOW}⚠️ ${NC} \$SDKROOT not set — cargo check works, but full link/bindgen requires iOS SDK\n"
        printf "     ${YELLOW}Hint:${NC} install Xcode SDK via xtool sdk install /path/to/Xcode.xip\n"
    fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────
printf "\n"
if [[ $MISSING -eq 0 ]]; then
    printf "${GREEN}${BOLD}All dependencies present. Ready to build.${NC}\n"
    exit 0
else
    printf "${RED}${BOLD}$MISSING missing dependency/dependencies. See install hints above.${NC}\n"
    exit 1
fi
