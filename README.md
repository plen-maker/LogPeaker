# benchpeek

An open, extensible **board-diagnostics cockpit**. Connect to a board over a
transport (serial today; CAN/USB next), let a *decoder plugin* turn the raw
stream into named signals, and benchpeek does the rest: live table, live plot,
session record/replay, and a rule engine that turns values into a plain-language
**diagnosis** ("VBAT 10.2 V — battery voltage out of range, check charging
system").

Status: **v1** — the decoder boundary is a real WASM component (`wasmtime`,
component model). Plugins run sandboxed, load at runtime, and are
indistinguishable from a compiled-in decoder to the rest of the app.

## Workspace

| crate | role |
|-------|------|
| `benchpeek-plugin`     | the decoder API (`Decoder`, `Sample`, `SignalMeta`) |
| `benchpeek-core`       | signal store (ring buffers), rule evaluator, record/replay |
| `benchpeek-app`        | egui cockpit + source threads (simulated / serial / CAN / replay) |
| `benchpeek-wasm-host`  | loads a `benchpeek:decoder` component and wraps it as a `Decoder` |
| `plugins/ascii-kv`     | reference decoder logic: newline-delimited `KEY=VALUE` telemetry |
| `plugins/ascii-kv-wasm`| the same decoder, exported as a WASM component (builds for `wasm32-unknown-unknown` only) |
| `plugins/can-raw`      | reference CAN decoder: one undecoded signal per CAN ID (see [CAN](#can-socketcan)) |
| `wit/decoder.wit`      | the plugin ABI: a `decoder` resource with `name` / `signals` / `decode` |

## Run

```sh
cargo run -p benchpeek-app
```

Click **Start simulated board**: three signals stream in, and after ~15 s the
simulated battery sags below 11.5 V so the diagnosis panel lights up.

For real hardware, pick a serial **Port**, set the **Baud**, and **Open serial**.
The `ascii-kv` decoder expects lines like:

```
VBAT=12.34 RPM=850 TEMP=41.2
```

**Start recording** writes the session to JSON-lines; **Replay file** plays one
back with original timing. **Start simulated board** now generates the same
`KEY=VALUE` text a real board would and runs it through whichever decoder is
selected — so it exercises the WASM plugin path too, not just the built-in one.

## Plugins (WASM components)

A decoder plugin implements the `decoder` resource in [`wit/decoder.wit`](wit/decoder.wit):
`constructor`, `name`, `signals`, `decode`. Build one with:

```sh
scripts/build-plugin.sh <crate-name> [output-name]
# e.g. the reference decoder:
scripts/build-plugin.sh ascii-kv-wasm ascii-kv
```

This compiles the crate for `wasm32-unknown-unknown` and componentizes it with
`wasm-tools component new` into `target/plugins/<output-name>.wasm`. In the app,
pick **WASM plugin** under DECODER, point the path at that file, and start any
source — the plugin is loaded fresh each time. `benchpeek-wasm-host`'s tests
(`cargo test -p benchpeek-wasm-host`, after building the plugin above) load the
real component and assert it decodes byte-for-byte identically to the compiled-in
version.

`plugins/ascii-kv-wasm` shows the pattern for wrapping an existing `Decoder`: it's
a thin shim around `plugins/ascii-kv`'s logic, not a reimplementation — porting a
plugin to WASM shouldn't mean rewriting it.

## CAN (SocketCAN)

An alternative to Serial/Simulated/Replay: streams frames off a SocketCAN
interface (`can0`, or `vcan0` for testing without hardware). Pick an
interface under **CAN if** in SOURCE and **Open CAN** - reconnects
automatically if the interface isn't up yet or drops, same as serial.
Requires Linux with the interface already created (`ip link add dev vcan0
type vcan && ip link set up vcan0` for a virtual one).

Each frame is rendered as a `candump`-style `<id>#<hex data>` line and run
through the same pluggable `Decoder` every other source uses (see
`SourceKind::Can` / `can_frame_line` in `source.rs`) - so a WASM decoder
could parse CAN frames too, with no plugin ABI changes. Select **CAN raw**
under DECODER for the reference decoder: it emits one undecoded signal per
CAN ID (`can_<id hex>`, the frame's bytes packed little-endian into a
number). A real per-bus decoding (DBC-driven signal extraction) is still on
the roadmap below.

## Rules

Rules live in code (`default_rules()`) for now; `rules/example.toml` documents the
file format that the loader (`RuleSet::from_toml_file`) already supports.

## Log watch

Independent of the signal source above: tails a remote board's log over SSH
(`journalctl -f -o cat` by default) into its own store, classifying each line
Info/Warn/Fault by keyword. Set **Host** (+ User/Command) in LOG WATCH and hit
**Start watch** - can run at the same time as a serial/simulated source, since
it's a different concern (text events, not numeric signals) and often a
different host entirely. View it under the **Logs** tab in the central panel;
its fault/warn counts feed into the same top-bar indicator as signal
diagnoses. Key-based SSH auth only (`BatchMode=yes`) - a spawned process has
no tty to prompt a password into, so set up `ssh-copy-id` first.

## Reading the plot and the History tab

Signal lines on the Plot tab are colored by live health (green/amber/red),
not an arbitrary palette. Any signal with a matching rule also gets faint
dashed threshold guides at its min/max, colored by the rule's severity, so
the healthy range is visible on the chart itself.

The **History** tab (next to Plot/Logs) is a timeline of health *transitions*
— when a signal first went into warn/fault, when it escalated, and when it
cleared — separate from the Diagnosis panel, which only ever shows the
current snapshot.

## Roadmap

- Plugin SDK docs + a second decoder (CAN DBC or NMEA) in a different source
  language, to prove the WASM boundary isn't Rust-only
- Real CAN decoding: a DBC-driven decoder to replace `can-raw`'s one-signal-
  per-ID placeholder with actual physical signals
- USB transport beyond USB-serial (raw bulk/interrupt endpoints for a
  non-CDC device) - Serial already covers USB-CDC boards, CAN is covered
  above
- KiCad project import with net ↔ live-signal cross-highlight
- Scripted test sequences with pass/fail reports

## Connection guide

Launch opens a skippable, animated connection guide: a stylized STM32MP257F-DK
board overview zooms toward CN21 (USB-C ST-LINK/power), then prompts “Plug in
the device”. Reduce motion freezes the camera at the connector. Replay or
reopen it using **Connection guide** in the top bar. **Enable auto-connect**
uses the existing USB-serial source discovery; it does not identify the board
model or guarantee a telemetry stream. A Linux console needs a telemetry
producer to supply the decoder's KEY=VALUE samples.

This is a native egui vector illustration, not a dimensionally accurate CAD
model or a Blender render. Connector roles reference ST's
[UM3385, figure 4](https://www.st.com/resource/en/user_manual/um3385-discovery-kit-with-stm32mp257f-mpu-stmicroelectronics.pdf).

## Yocto files and sync

In **Yocto → Files & Transfer**, enter the Build Server SSH host/user and an
absolute remote folder, then **Browse**. Directories open with a click; **Up**
navigates to the parent. This browses the host filesystem; to access a Docker
project, use its host bind-mount path.

Set an existing absolute local folder and choose Upload or Download. **Preview
sync** runs an rsync dry run; **Apply sync** becomes available for that exact
host, user, paths and options. Copies directory contents recursively, keeps
destination-only files, and skips symlinks. Existing files are skipped unless
**Update existing files** is enabled. The preview is informational: changes to
files between preview and execution can change what gets copied. Cancellation
stops the worker; files already copied remain. Errors appear in the panel.

Requires key-based SSH, a trusted host key in known_hosts, GNU find on the
server, and rsync installed both locally and remotely. No remote operations
occur until the user clicks Browse, Preview sync, or Apply sync.
