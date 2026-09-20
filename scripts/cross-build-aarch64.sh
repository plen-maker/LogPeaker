#!/usr/bin/env bash
# Cross-build benchpeek for the STM32MP257F-DK (aarch64 Linux). Run on the Linux
# build machine (inside the Yocto builder container), not on macOS.
#
# Two ways to get a target sysroot + cross compiler:
#
#  A) Reuse an already built recipe's sysroot (no SDK needed). Any recipe that
#     depends on udev, libxkbcommon, wayland and virtual/egl + libgles2 works, e.g.
#     meerkat-compositor:
#       RECIPE_WORKDIR=$BUILDDIR/tmp-glibc/work/cortexa35-ostl-linux/meerkat-compositor/0.1 \
#       DEST=<layer>/recipes-graphics/benchpeek/files/benchpeek \
#       scripts/cross-build-aarch64.sh
#
#  B) A Yocto SDK:
#       SDK_ENV=/path/to/environment-setup-<target>-linux scripts/cross-build-aarch64.sh
#
# Needs rustup with Rust >= 1.95 (the dependency tree requires it; scarthgap
# ships 1.75, so the Yocto Rust cannot build this).
set -euo pipefail

TARGET=aarch64-unknown-linux-gnu

if [ -n "${RECIPE_WORKDIR:-}" ]; then
    SYSROOT="$RECIPE_WORKDIR/recipe-sysroot"
    NATIVE="$RECIPE_WORKDIR/recipe-sysroot-native"
    GCC="$(ls "$NATIVE"/usr/bin/aarch64-*/aarch64-*-gcc | head -n 1)"
    [ -x "$GCC" ] || { echo "error: no aarch64 cross gcc under $NATIVE" >&2; exit 1; }
    CC="$GCC --sysroot=$SYSROOT"
    export PATH="$NATIVE/usr/bin:$PATH"          # brings the native pkg-config
    export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
    export PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib/pkgconfig:$SYSROOT/usr/share/pkgconfig"
elif [ -n "${SDK_ENV:-}" ]; then
    # shellcheck disable=SC1090
    source "$SDK_ENV"    # exports CC, SDKTARGETSYSROOT, PKG_CONFIG_* ...
else
    echo "error: set RECIPE_WORKDIR (a built recipe's WORKDIR) or SDK_ENV" >&2
    exit 1
fi

rustup target add "$TARGET"

# $CC carries flags (--sysroot ...): wrap it as the linker.
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
        echo "error: $lib not found in the target sysroot" >&2
        exit 1
    }
done
for lib in wayland-client xkbcommon egl glesv2; do
    pkg-config --exists "$lib" || echo "note: $lib.pc is not in the sysroot - fine, it is loaded at runtime" >&2
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
