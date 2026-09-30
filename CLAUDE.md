# benchpeek - handoff notes for Claude Code

Carries over what a fresh session (e.g. on macOS) can't derive from the code.
The README and `docs/plugin-sdk.md` cover the features themselves.

## Working with this user
- Writes Hungarian, casual, lots of typos: reply in Hungarian, read for intent.
- Wants to design the UI themselves. Offer ideas and options; don't hand over
  finished designs. `/design` returned 403 for this account.
- Often away: keep going on clearly established work, but ask before pushing
  anything new, rewriting history, or touching git config.

## Commands
- Run: `cargo run -p benchpeek-app`
- `cargo test` needs prebuilt WASM plugins first:
  `scripts/build-plugin.sh ascii-kv-wasm ascii-kv` and
  `scripts/build-can-dbc-py.sh` (needs `pip install componentize-py` on PATH).
  `tests/can_dbc.rs` takes ~130 s: each test instantiates CPython-in-WASM.
- Hardware test, `#[ignore]`d: `cargo test -p benchpeek-app --bin benchpeek -- --ignored usb_read_loop_receives_a_real_device_response`
  needs an ST-Link at USB 0483:3753.

## Design decisions that aren't obvious from the code
- Every source feeds bytes to the `Decoder` trait. CAN frames are rendered as
  candump lines `<id>#<hex>\n` in `source.rs::can_frame_line`, so no second
  decoder ABI exists.
- WIT `constructor(config: option<string>)`: the host reads the config file
  (e.g. a .dbc) because components are built `--stub-wasi` and have no
  filesystem. `benchpeek-wasm-host` registers no WASI imports.
- `can-dbc-py` masks DBC `BO_` ids with `0x1FFFFFFF`: DBC sets bit 31 on
  extended ids, SocketCAN's `raw_id()` strips it.
- `SequenceRunner` treats a signal older than 3 s as no data, so a dead
  transport can't leave a step passed on a frozen value.

## Live SWD memory monitoring (in progress)

Agreed direction with the user: read a running target's RAM live through
the ST-Link's own SWD debug port, instead of only decoding whatever a
transport happens to send. No existing benchpeek source can do this -
Serial/CAN/USB all wait for the target to talk first. This would let a
decoder plugin expose any firmware variable as a signal with zero firmware
changes, which is the "nobody's done this in one open tool" angle the user
wants (closest existing tools - SEGGER Ozone, STM32CubeMonitor - are
closed/single-vendor).

Board power note from the user: the STM32MP257F-DK needs 5V/3A over
USB-C PD (confirmed via ST community + UM3385) to run reliably - a bare
laptop port isn't enough. CN21 (ST-Link) and the dedicated PWR input are
separate physical connectors on this board, so there's no conflict between
powering it (power bank or mains into PWR) and having ST-Link's USB-C go
straight to whatever runs benchpeek.

Progress (`crates/benchpeek-app/src/stlink.rs`, not yet a `Source`):
- `StlinkHandle::open` + `.command()`: shared open/claim/transfer_blocking
  plumbing (reuses the exact pattern `source.rs`'s raw USB transport
  already proved against real hardware), one `command(cmd_bytes,
  max_resp_len)` call per ST-Link command.
- Implemented: `.version()` (`STLINK_APIV3_GET_VERSION_EX`, 0xFB - a plain
  query, answers directly with its data), `.enter_swd_debug()` and
  `.read_idcode()` (both under `STLINK_DEBUG_COMMAND`, 0xF2 - answer with a
  status byte first, `STLINK_DEBUG_ERR_OK`/`_FAULT`). Every byte layout is
  cited to the specific OpenOCD `stlink_usb.c` function it came from in
  each method's doc comment, not guessed.
- **Not implemented: reading target memory.** OpenOCD's `read_mem32` needs
  a second USB transaction (a follow-up status query) whose exact byte
  layout (ap_num/csw packing) wasn't pinned down with enough confidence to
  ship - landed the verified half (link-up + IDCODE) instead of guessing
  at the rest. That's the next step.
- All parsing is unit-tested (version, debug-status OK/fault, IDCODE
  offset/short-response). The two hardware tests
  (`probes_a_real_stlinks_version`, `enters_swd_and_reads_a_real_idcode`,
  both `#[ignore]`d) have **not** been run against real hardware - no
  simulated or real ST-Link has been present in any sandbox instance since
  the raw USB transport work. Run both for real before trusting this
  module; `enters_swd_and_reads_a_real_idcode` is the actual milestone
  (proves the debug link comes up and identifies a real core).
- Still undecided: how a decoder plugin gets told which addresses/struct
  layout to poll once memory reads exist (`constructor(config)` could
  carry an ELF's debug symbols, similar to how `can-dbc-py` takes a
  `.dbc`).

## Yocto build server & SD card flashing

Real hardware, not simulated: STM32MP257F-DK board, ST-Link over USB-C for
SWD+UART, a 16GB SanDisk microSD, and a Yocto/OpenSTLinux build server
(`192.168.100.236`, user `yocto`, SSH key auth, docker container
`stm32mp2-builder`). Workspace inside the container:
`/yocto-st/projects/stm32mp2-62` (`source layers/openembedded-core/oe-init-build-env build-62`
before any `bitbake`). Flashable images land in
`build-62/tmp-glibc/deploy/images/stm32mp25-disco/`.

- **Never write to a physical device (`/dev/sd*`, `/dev/mmcblk*`, `/dev/nvme*`)
  without an explicit go-ahead in that specific turn.** The user has a past
  incident (an NVMe got flashed by accident) and was explicit: only flash
  when told to, right then. Building/staging images and SSHing into the
  build server are fine without asking; the line is the actual write.
- Flashing recipe that works: a flashlayout `.tsv` under
  `deploy/images/stm32mp25-disco/flashlayout_st-image-weston/<variant>/`
  gives `(name, offset, binary)` rows for `IP == mmc0`. For each, `dd
  if=<binary> of=/dev/sdX bs=512 seek=<offset/512> conv=notrunc,fsync`. Do
  the actual `dd` inside `docker run --rm --device=/dev/sdX ubuntu:22.04
  ...` on the build server - it's root and has the `disk`-group-free device
  node without needing `sudo` (the `yocto` user has no passwordless sudo,
  and interactive SSH can't supply a password).
  - Use the Bash tool's own timeout for the whole SSH call (the `rootfs`
    partition alone is ~1GB and takes 3+ minutes over this USB reader at
    ~5-7MB/s); do **not** additionally wrap the command in a short shell
    `timeout N` - if that fires, the remote write actually keeps running
    undisturbed (docker doesn't get the local SSH client's death signal
    propagated to a killed write), but the local timeout host loses track
    of it and reports a false failure.
  - `conv=fsync` on every `dd` is required. Verifying immediately after
    writing (read the same bytes back from the device in the same session)
    can give a **false positive**: the readback can hit the build server's
    own page cache instead of the physical card, so it looks byte-perfect
    even if the physical write never completed. The only way to know for
    sure is to physically eject/reinsert the card (or unplug/replug the
    reader) between writing and verifying, which drops the cache and forces
    a real read. This actually happened once this session - full write
    reported success and read back clean, then failed a real check after
    reinsertion, and had to be redone.
- **`opteemin` vs `optee` flashlayout - pick `optee` for this board if you
  want a display.** `local.conf` has `BOOTSCHEME_LABELS:append = " opteemin"`
  (added for the MeerkatOS work), so both variants get built side by side
  under `flashlayout_st-image-weston/opteemin/` and `.../optee/`. The
  `opteemin` (OP-TEE minimal) variant's RIF config
  (`a35-td/optee/stm32mp257f-dk-ca35tdcid-ostl-rif.dtsi` in the fetched
  external device-tree source) marks the LTDC's L3 sub-resource
  `RIF_SEC` even in the non-secure profile, so Linux's `display-controller`
  driver fails to probe (`error: -13`, then wlroots/meerkat-compositor logs
  "Failed to open any DRM device" and never gets `/dev/dri/card0`). The
  plain `optee` (full) flashlayout
  (`FlashLayout_sdcard_stm32mp257f-dk-ca35tdcid-ostl-optee.tsv` - only
  `fsbla1/fsbla2/fip-a` differ, `bootfs/vendorfs/rootfs/userfs` are the same
  files) grants Linux the LTDC properly and the display comes up. Confirmed
  working on real hardware 2026-09-25. Whether to just remove the
  `opteemin` line from `local.conf` (so it stops building a variant nobody
  wants) is still an open question for the user.
- Serial console: ST-Link exposes two `/dev/ttyACM*` devices; the shell/log
  one is typically `ttyACM0` at 115200 8N1 (`stty -F /dev/ttyACM0 115200
  cs8 -cstopb -parenb raw -echo`). The board auto-logs-in to a root shell,
  handy for live `systemctl`/`journalctl` checks without a separate login
  step.
- `weston-graphical-session.service` can come up `disabled`/`inactive` right
  after first boot even when everything is otherwise fine, with a "unit
  file ... changed on disk, run daemon-reload" warning - a stale systemd
  unit cache from first-boot scripts touching the unit file after systemd's
  initial scan. `systemctl daemon-reload && systemctl start
  weston-graphical-session.service` picks it up; check
  `journalctl -b | grep -i weston` for the real underlying error (that's
  where the DRM `-13` above actually surfaced) rather than trusting
  `systemctl status` on the top-level unit alone.
- **benchpeek segfaults as a Wayland client under meerkat-compositor** -
  confirmed on real hardware 2026-09-25, unchanged across an eframe
  wgpu-vs-glow fix, an `EGL_PLATFORM` hint, and disabling window
  decorations. `coredumpctl` + `eu-stack` (no gdb on the board) show the
  crash is inside the vendor GC-Nano EGL driver's own
  `wl_proxy_create_wrapper` call (SEGV_MAPERR, null-proxy pattern),
  happening while the driver bootstraps its own internal Wayland
  connection during `eglCreateWindowSurface` - not benchpeek's or
  glutin's code, and there's no source or vendor doc to patch it from
  here. This is a known-quirky GPU stack: `meta-meerkat`'s own
  `recipes-graphics/wlroots/files/0001-egl-tolerate-vivante-device-query.patch`
  already works around a *different* GC-Nano EGL bug
  (`eglQueryDeviceStringEXT` failing), and a comment in
  `meerkat-compositor.c` (near `wlr_output_commit_state`, the frame
  callback) documents forcing full-frame damage every commit to sidestep
  a suspected partial-damage-region bug interacting with this board's
  `WL_OUTPUT_TRANSFORM_90` (the panel is driven rotated). Both are
  wlroots-side patches; a generic Wayland client library like
  winit/glutin gets none of them. **Fix in progress:**
  `crates/benchpeek-app/src/drm_backend.rs` drives `/dev/dri/card0`
  directly via GBM+EGL (`BENCHPEEK_DRM=1`), the same way
  meerkat-compositor itself does, bypassing the driver's Wayland-client
  code entirely instead of chasing individual quirks in it. v0 renders
  only (no input yet) and re-sets the CRTC every frame instead of
  page-flipping.
  **Confirmed rendering on real hardware 2026-09-30**: with
  `card.acquire_master_lock()` added (mode-setting needs DRM master, not
  just root - reading resources/connectors works on any fd, but
  `set_crtc` doesn't without it) and `meerkat-compositor` fully stopped,
  `BENCHPEEK_DRM=1 /usr/bin/benchpeek` ran a 60s render loop on the
  LVDS panel with zero errors and was visually confirmed on the actual
  screen. Still v0: no input yet, and re-sets the CRTC every frame
  instead of page-flipping (works, but is presumably why any flicker
  shows up - not yet characterized). Next up, in order the user asked
  for: (1) input via evdev, (2) real page-flipping, (3) UI design work
  (the user's own).
  - `meerkat-compositor` fights back hard when you try to free the DRM
    master for a manual test: stopping `weston-graphical-session.service`
    (system) does NOT stop it - the actual compositor process is
    supervised by a *separate* unit, `meerkat-session.service`, not
    obviously named after weston/meerkat-compositor's own systemd unit
    names (`weston.service`/`weston.socket`, which also exist and
    respawn it independently via socket activation). Stopping/killing
    any one or two of these three still gets it relaunched by whichever
    you missed. The reliable sequence: `systemctl stop
    meerkat-session.service && systemctl mask meerkat-session.service`
    (root shell) - that alone was enough once found; the earlier
    `weston.socket`/`weston.service` user-unit masking turned out to be
    unnecessary but harmless. `grep -rl meerkat-compositor
    /usr/lib/systemd/ /etc/systemd/` finds all of them if this changes.
  - After masking `meerkat-session.service` and rebooting, a *different*
    process can end up holding the DRM master instead: `psplash-drm`
    (`psplash-drm-start.service`, boot splash screen), which normally
    gets told to quit (`echo QUIT > /tmp/splash_fifo`) once the real
    graphical session is ready - but nothing does that anymore once
    meerkat-session is masked, so it just sits there holding
    `/dev/dri/card0`. `acquire_master_lock` fails with EBUSY either way,
    so if stopping/masking the meerkat units doesn't clear it, check
    `fuser /dev/dri/card0` (or `for p in /proc/[0-9]*; do ls $p/fd
    2>/dev/null | grep -q card0 && cat $p/comm; done` - no `lsof` on this
    image) before assuming it's meerkat again; `kill -9` on psplash-drm's
    pid works fine for a one-off test even though it skips the fifo.
    `seatd-weston.service` can also be in the way and may need
    `systemctl stop` too.
  - **Touch input confirmed working 2026-09-30**: `touch_input.rs`
    (evdev on `/dev/input/touchscreen0`, the ILI210x) logs "touchscreen
    input active" and the user confirmed on-screen response to taps.
  - **Page-flipping confirmed working 2026-09-30 too**: `set_crtc` now
    only runs once (first frame); every frame after that is a real
    `page_flip` + blocking `receive_events()` wait, pacing the loop to
    the display's refresh rate instead of spinning unthrottled. User
    confirmed no flicker/tearing regression versus the earlier
    set_crtc-every-frame version (which also looked fine - this was a
    correctness/efficiency fix, not chasing a visible bug).
  - **All three of the DRM backend's v0 goals are done**: render, touch
    input, page-flip. `BENCHPEEK_DRM=1 /usr/bin/benchpeek` (as root,
    with meerkat-session/psplash-drm/seatd-weston stopped per above) is
    a real, working alternative to running under meerkat-compositor's
    Wayland session, with the crashing vendor-driver code path avoided
    entirely. Next: UI design work, which is the user's own - see
    "Working with this user" at the top of this file.
- `meta-meerkat` is a much bigger existing project than "a demo
  compositor": `recipes-graphics/wayland/files/` alone has ~20 patches
  building out a phone-like desktop-shell (app-window management, a
  "genie" minimize/restore animation, lock screen, panel autohide,
  screenshot authorization) plus `recipes-extended/flatpak` patches for
  sandboxed app support. Worth searching here for prior art before
  solving a display/input/GPU problem on this board from scratch again -
  there may already be a patch or pattern for it.

## Open items
- **macOS build is expected to fail** (not compiled there yet): `benchpeek-app`
  depends unconditionally on `socketcan` with the `enumerate` feature, which
  needs Linux libudev and Linux CAN APIs. First task on macOS: move it to
  `[target.'cfg(target_os = "linux")'.dependencies]` and `cfg`-gate
  `SourceKind::Can`, `run_can`/`can_session`/`can_frame_line` (and its test),
  `list_can_interfaces` and the CAN UI block in `app.rs`. Verify by compiling
  on the Mac. `nusb` and `serialport` have macOS backends but are untested there.
- Branch `origin/mono-workspace-ui` holds a separate monochrome "Workspace UI"
  (`src/ws/`, `--classic` runs the old cockpit). Everything it shows is demo
  data (`src/ws/demo.rs`); it is not wired to the real engine. It edits
  `app.rs`, `source.rs` and the app `Cargo.toml` slightly, so expect small
  merge conflicts with the macOS fix.
- Deliberately skipped from a self-review: device scans (`list_ports`,
  `list_can_interfaces`, `list_usb_devices`) block the UI thread; a decoder
  registry to replace the `DecoderKind` match; a shared picker widget;
  validating `hold_secs >= timeout_secs`; `can-raw`'s `u64 as f64` precision loss.

## Environment notes
- The Linux side had no display, so no UI change was ever seen running, only
  built and unit-tested. On macOS, run the app and look at it.
- Blender scenes for making more intro animations are in `tools/blender/`: the
  base board model (`Benchpeek_STM32MP257F-DK.blend`), the latest scene
  (`..._USB_OTG_board_fade_60fps.blend`) and an isolated-port variant
  (`..._USB_OTG_isolated_60fps.blend`). Contents were never opened from the
  Linux sandbox (no Blender there); the names and save times are all that's
  known. The scripts that drive them (`plugin_intro.py`, `pack_bpk.py`) are on
  `origin/mono-workspace-ui`. Two intermediate iterations and the 89 ST STEP
  source models (58 MB) stayed on the Linux side under
  `~/Documents/Codex/2026-09-10/csa/`. The app itself builds from the baked
  `crates/benchpeek-app/assets/board_intro.bpk` alone.
- Git identity was unset on the Linux side; commits there are authored
  `Unknown <ddnemet@ddnemet.tail6ed0b4.ts.net>`. Set `user.name`/`user.email`
  on the Mac.
