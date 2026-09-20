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
- The Blender masters for the intro animation are NOT in git (they lived under
  `~/Documents/Codex/2026-09-10/csa/` on the Linux side). Only the baked
  `crates/benchpeek-app/assets/board_intro.bpk` is tracked; the build needs
  nothing else.
- Git identity was unset on the Linux side; commits there are authored
  `Unknown <ddnemet@ddnemet.tail6ed0b4.ts.net>`. Set `user.name`/`user.email`
  on the Mac.
