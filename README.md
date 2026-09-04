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
| `benchpeek-app`        | egui cockpit + source threads (simulated / serial / replay) |
| `benchpeek-wasm-host`  | loads a `benchpeek:decoder` component and wraps it as a `Decoder` |
| `plugins/ascii-kv`     | reference decoder logic: newline-delimited `KEY=VALUE` telemetry |
| `plugins/ascii-kv-wasm`| the same decoder, exported as a WASM component (builds for `wasm32-unknown-unknown` only) |
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

## Rules

Rules live in code (`default_rules()`) for now; `rules/example.toml` documents the
file format that the loader (`RuleSet::from_toml_file`) already supports.

## Roadmap

- Plugin SDK docs + a second decoder (CAN DBC or NMEA) in a different source
  language, to prove the WASM boundary isn't Rust-only
- CAN / SocketCAN + USB transports
- KiCad project import with net ↔ live-signal cross-highlight
- Scripted test sequences with pass/fail reports
