#!/usr/bin/env bash
# Cross-build benchpeek for the STM32MP257F-DK (aarch64 Linux) against a Yocto
# SDK sysroot. Run on the Linux build machine, not on macOS.
#
#   SDK_ENV=/path/to/environment-setup-<target>-linux \
#   [DEST=/path/to/meta-meerkat/recipes-graphics/benchpeek/files/benchpeek] \
#   scripts/cross-build-aarch64.sh
#
# Needs rustup with Rust >= 1.95 (the dependency tree requires it).
set -euo pipefail

: "${SDK_ENV:?set SDK_ENV to the SDK's environment-setup-* script}"
TARGET=aarch64-unknown-linux-gnu

# shellcheck disable=SC1090
source "$SDK_ENV"    # exports CC, SDKTARGETSYSROOT, PKG_CONFIG_* ...

rustup target add "$TARGET"

# The SDK's $CC carries flags (--sysroot, -mcpu ...): wrap it as the linker.
LINKER="$(mktemp)"
trap 'rm -f "$LINKER"' EXIT
printf '#!/bin/sh\nexec %s "$@"\n' "$CC" > "$LINKER"
chmod +x "$LINKER"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$LINKER"
export CC_aarch64_unknown_linux_gnu="$CC"
export PKG_CONFIG_ALLOW_CROSS=1
export CARGO_PROFILE_RELEASE_STRIP=symbols

for lib in libudev; do
    pkg-config --exists "$lib" || {
        echo "error: $lib not found in the SDK sysroot; add its -dev package to TOOLCHAIN_TARGET_TASK and rebuild the SDK" >&2
        exit 1
    }
done
for lib in wayland-client xkbcommon egl glesv2; do
    pkg-config --exists "$lib" || echo "note: $lib.pc is not in the SDK - fine, it is loaded at runtime" >&2
done

# GLES back-end (no Vulkan needed on the board's gcnano GPU), no wgpu.
cargo build --release -p benchpeek-app \
    --no-default-features --features glow \
    --target "$TARGET"

BIN="target/$TARGET/release/benchpeek"
file "$BIN" 2>/dev/null || true
if [ -n "${DEST:-}" ]; then
    install -D -m 0755 "$BIN" "$DEST"
    echo "installed $DEST"
else
    echo "built $BIN"
fi
