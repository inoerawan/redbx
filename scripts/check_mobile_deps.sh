#!/usr/bin/env bash
# check_mobile_deps.sh — verify the dependencies needed to build redbx-mobile.
#
# Usage:
#   bash scripts/check_mobile_deps.sh            # core + every supported platform
#   bash scripts/check_mobile_deps.sh core       # core toolchain only
#   bash scripts/check_mobile_deps.sh android    # core + Android
#
# Checks are scoped per platform on purpose: an Android-only machine must not be
# failed by a missing toolchain for some other platform.
#
# iOS support was removed pending a macOS build host. Re-add an `ios` section
# here when that lands.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

MISSING=0

ok()      { printf "  ${GREEN}✅${NC} %s\n" "$1"; }
fail()    { printf "  ${RED}❌${NC} %s ${RED}— not found${NC}\n" "$1"; printf "     ${YELLOW}Install:${NC} %s\n" "$2"; MISSING=$((MISSING+1)); }
note()    { printf "  ${YELLOW}⚠️ ${NC} %s\n" "$1"; printf "     ${YELLOW}Hint:${NC} %s\n" "$2"; }
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

WORKSPACE_ROOT="$(dirname "$(dirname "$(realpath "$0")")")"

# Keep in sync with `compileSdk` in android/redbx/build.gradle.kts.
ANDROID_COMPILE_SDK=35

# Locate an Android SDK root. Prints the path, or returns 1.
resolve_sdk() {
    local sdk
    for sdk in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Android/Sdk"; do
        if [[ -n "$sdk" && -d "$sdk" ]]; then
            printf '%s\n' "$sdk"
            return 0
        fi
    done
    return 1
}

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
    sdk="$(resolve_sdk || true)"
    if [[ -n "$sdk" && -d "$sdk/ndk" ]]; then
        candidate="$(find "$sdk/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -1)"
        if [[ -n "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    fi

    return 1
}

# ── Core ──────────────────────────────────────────────────────────────────────

check_core() {
    section "Core"

    check_cmd "rustup" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    check_cmd "rustc"  "rustup install stable"
    check_cmd "cargo"  "rustup install stable"

    # Rust version check
    local min_rust="1.89" rust_ver
    if command -v rustc &>/dev/null; then
        rust_ver=$(rustc --version | sed 's/rustc \([0-9.]*\).*/\1/')
        if printf '%s\n%s\n' "$min_rust" "$rust_ver" | sort -V -C; then
            ok "Rust >= $min_rust (found $rust_ver)"
        else
            fail "Rust >= $min_rust (found $rust_ver)" "rustup update"
        fi
    fi

    # uniffi-bindgen — workspace binary (must match the uniffi dep in redbx-mobile)
    # Usage: cargo run -p uniffi-bindgen -- generate --library <lib> --language kotlin --out-dir <dir>
    local bindgen_src="$WORKSPACE_ROOT/crates/uniffi-bindgen/src/main.rs"
    if [[ -x "$WORKSPACE_ROOT/target/release/uniffi-bindgen" ]] \
        || [[ -x "$WORKSPACE_ROOT/target/debug/uniffi-bindgen" ]]; then
        ok "uniffi-bindgen (workspace binary, built)"
    elif [[ -f "$bindgen_src" ]]; then
        ok "uniffi-bindgen (workspace crate present — built on demand)"
    else
        fail "uniffi-bindgen" "crates/uniffi-bindgen missing — re-run workspace setup"
    fi
}

# ── Android ───────────────────────────────────────────────────────────────────

check_android() {
    section "Android"

    check_cmd "cargo-ndk" "cargo install cargo-ndk"

    # JDK — Gradle/AGP do not track the newest JDK immediately; 17 or 21 is the
    # supported range for AGP 8.x.
    local jbr_hint="Install JDK 17 or 21: https://adoptium.net"
    if [[ -x "$HOME/Applications/android-studio/jbr/bin/java" ]]; then
        jbr_hint="export JAVA_HOME=\$HOME/Applications/android-studio/jbr  # Android Studio's bundled JDK 21"
    fi

    local java_bin="java"
    [[ -n "${JAVA_HOME:-}" && -x "${JAVA_HOME}/bin/java" ]] && java_bin="${JAVA_HOME}/bin/java"

    if command -v "$java_bin" &>/dev/null || [[ -x "$java_bin" ]]; then
        local java_ver
        java_ver=$("$java_bin" -version 2>&1 | head -1 | sed -n 's/.*version "\([0-9]*\).*/\1/p')
        if [[ -z "$java_ver" ]]; then
            note "java found but version could not be parsed" "expected JDK 17 or 21"
        elif [[ "$java_ver" -lt 17 ]]; then
            fail "JDK >= 17 (found $java_ver)" "$jbr_hint"
        elif [[ "$java_ver" -gt 21 ]]; then
            fail "JDK 17 or 21 (found $java_ver — too new for AGP 8.x)" "$jbr_hint"
        else
            ok "java (JDK $java_ver)"
        fi
    else
        fail "java" "$jbr_hint"
    fi

    # Android SDK platform matching compileSdk
    local sdk
    sdk="$(resolve_sdk || true)"
    if [[ -z "$sdk" ]]; then
        fail "Android SDK" \
             "Install Android Studio, or set ANDROID_HOME to an SDK installation"
    elif [[ -d "$sdk/platforms/android-$ANDROID_COMPILE_SDK" ]]; then
        ok "Android SDK platform android-$ANDROID_COMPILE_SDK (in $sdk)"
    else
        fail "Android SDK platform android-$ANDROID_COMPILE_SDK" \
             "Android Studio → SDK Manager → SDK Platforms → API $ANDROID_COMPILE_SDK, or (needs cmdline-tools)
                sdkmanager --install 'platforms;android-$ANDROID_COMPILE_SDK'
              compileSdk is set in android/redbx/build.gradle.kts"
    fi

    # Gradle is consumed through the wrapper (android/gradlew), so a system-wide
    # gradle is optional. Only the wrapper itself is required.
    if [[ -x "$WORKSPACE_ROOT/android/gradlew" ]]; then
        ok "gradle wrapper (android/gradlew)"
    else
        fail "gradle wrapper (android/gradlew)" \
             "open the android/ folder in Android Studio (it generates the wrapper), or with a system Gradle:
                cd android && gradle wrapper --gradle-version 8.11.1"
    fi

    # Android NDK. cargo-ndk 4.x finds one under the SDK by itself, so an unset
    # ANDROID_NDK_HOME is fine as long as an NDK is actually installed.
    local ndk
    ndk="$(resolve_ndk || true)"
    if [[ -n "$ndk" ]]; then
        ok "Android NDK = $ndk"
    else
        fail "Android NDK" \
             "Install via Android Studio → SDK Manager → NDK, or: sdkmanager --install 'ndk;<version>'"
    fi

    check_rust_target "aarch64-linux-android"    "ARM64 devices"
    check_rust_target "armv7-linux-androideabi"  "ARM 32-bit devices"
    check_rust_target "x86_64-linux-android"     "x86_64 emulator"
    check_rust_target "i686-linux-android"       "x86 emulator"
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

TARGET="${1:-all}"

case "$TARGET" in
    core)    check_core ;;
    android) check_core; check_android ;;
    all)     check_core; check_android ;;
    *)
        printf "${RED}${BOLD}Unknown target '%s'. Use: core | android | all${NC}\n" "$TARGET" >&2
        exit 2
        ;;
esac

# ── Summary ───────────────────────────────────────────────────────────────────
printf "\n"
if [[ $MISSING -eq 0 ]]; then
    printf "${GREEN}${BOLD}All '%s' dependencies present. Ready to build.${NC}\n" "$TARGET"
    exit 0
else
    printf "${RED}${BOLD}%s missing dependency/dependencies for '%s'. See install hints above.${NC}\n" \
        "$MISSING" "$TARGET"
    exit 1
fi
