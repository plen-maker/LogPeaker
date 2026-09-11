# Plugin SDK

A benchpeek decoder plugin is a `benchpeek:decoder` [WASM component](https://component-model.bytecodealliance.org/):
a sandboxed, language-agnostic module the host (`benchpeek-wasm-host`) loads
at runtime and treats exactly like a compiled-in `Decoder` - the app,
source threads, signal store, and rule engine can't tell the difference.
This doc is the contract a plugin implements, plus a worked recipe for two
languages already in this repo (Rust, Python) and pointers for others.

If you just want to *use* a plugin, see the README's "Plugins (WASM
components)" section instead - this doc is for writing one.

## The contract

Defined in [`wit/decoder.wit`](../wit/decoder.wit):

```wit
resource decoder {
    constructor(config: option<string>);
    name: func() -> string;
    signals: func() -> list<signal-meta>;
    decode: func(bytes: list<u8>, t: f64) -> list<sample>;
}
```

- **`constructor(config)`** - `config` is opaque to the host: it's the
  contents of whatever file path the user typed into the app's **Config**
  field next to the plugin path (see `benchpeek-app`'s `make_decoder`),
  read and passed through as-is. A component built with `--stub-wasi` (below) has no
  filesystem access of its own, so this is the *only* way a plugin gets
  external configuration - a DBC file's contents, a signal name mapping,
  whatever your decoder needs. A decoder with nothing to configure just
  ignores it (`None` means "no config given").
- **`name()`** - a human-readable label, shown in the app's status line.
- **`signals()`** - signals you already know about (name, unit, healthy
  range), used to seed default rules. Returning an empty list is fine and
  common - `SignalStore` auto-registers any signal name `decode` emits
  that wasn't pre-declared, so `signals()` is a convenience, not a
  requirement. Return it eagerly (don't wait for `decode` to be called
  first) if you want default rules to exist before the first sample lands.
- **`decode(bytes, t)`** - the workhorse. Called with a fresh chunk of raw
  bytes from the transport and the session-relative timestamp (seconds) it
  was read at; returns every `sample` the chunk completed. **Chunks are
  not guaranteed to align with your protocol's framing** - a line, a CAN
  frame, whatever you're parsing, may arrive split across two calls, so
  buffer any incomplete trailing data in `self` and pick up where you left
  off next call. Every reference decoder in this repo does this (see
  `plugins/ascii-kv/src/lib.rs`, `plugins/can-raw/src/lib.rs`,
  `plugins/can-dbc-py/decoder.py`) - copy the pattern rather than
  reinventing it.

That's the entire ABI. No host-provided imports exist to call back into
(no logging, no clock, no I/O) - see "Constraints" below for why.

## Two ways in: byte-stream or frame-shaped protocols

Every source in benchpeek (serial, simulated, CAN, replay) ultimately feeds
`decode` a byte stream - even CAN, which isn't naturally line-oriented.
`benchpeek-app`'s CAN source renders each frame as a `candump`-style
`<id>#<hex data>\n` ASCII line before handing it to whichever decoder is
active (see `source.rs::can_frame_line`), so a CAN decoder plugin is still
just parsing lines, only with hex-and-hash syntax instead of `KEY=VALUE`.
This is why the WIT ABI didn't need a second, frame-shaped resource for
CAN: reduce your protocol to bytes at the transport layer, and every
decoder - compiled-in or WASM, Rust or not - shares one interface.

## Recipe 1: Rust, via `wit-bindgen`

`plugins/ascii-kv-wasm` is the template - a thin shim around
`plugins/ascii-kv`'s plain `Decoder` impl:

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wit-bindgen = "0.61.1"
```

```rust
wit_bindgen::generate!({
    world: "decoder-plugin",
    path: "../../wit",
});

use exports::benchpeek::decoder::decoder::{Guest, GuestDecoder, Sample, SignalMeta};

struct Component;
pub struct MyResource { /* ... your state ... */ }

impl Guest for Component {
    type Decoder = MyResource;
}

impl GuestDecoder for MyResource {
    fn new(config: Option<String>) -> Self { /* ... */ }
    fn name(&self) -> String { /* ... */ }
    fn signals(&self) -> Vec<SignalMeta> { /* ... */ }
    fn decode(&self, bytes: Vec<u8>, t: f64) -> Vec<Sample> { /* ... */ }
}

export!(Component);
```

Build with:

```sh
scripts/build-plugin.sh <crate-name> [output-name]
# e.g.
scripts/build-plugin.sh ascii-kv-wasm ascii-kv
```

This compiles for `wasm32-unknown-unknown` and componentizes the result
with `wasm-tools component new` into `target/plugins/<output-name>.wasm`.
Keep your plugin crate out of the workspace's `default-members` (see the
root `Cargo.toml`'s comment on `ascii-kv-wasm`) - its generated export
symbols don't link on the host target, so a plain `cargo build`/`cargo
test` across the workspace would fail; the dedicated script targets it
explicitly instead.

## Recipe 2: another language, via `componentize-py`

`plugins/can-dbc-py` proves the boundary isn't Rust-only: a real DBC
parser and bit-level signal extractor, in Python, with **no changes needed
to `benchpeek-wasm-host` or the WIT file** to load it - the component
model doesn't care what produced the `.wasm`.

Two files, deliberately split:

- `app.py` - the entry module `componentize-py componentize` loads by
  name. Can be empty; nothing in this world is exported at the world
  level itself.
- `decoder.py` - a **top-level, separately importable** module (not
  nested under `app`) named after the WIT interface (`decoder`), containing
  a class also named `Decoder` that structurally implements the generated
  `Protocol` (no need to import or subclass it - componentize-py finds it
  by module/class name):

  ```python
  class Decoder:
      def __init__(self, config):       # config: str | None
          ...
      def name(self):                    # -> str
          ...
      def signals(self):                  # -> list[SignalMeta]
          ...
      def decode(self, data, t):          # data: bytes, t: float -> list[Sample]
          ...
  ```

  `SignalMeta`/`Sample` are dataclasses generated at
  `wit_world.imports.types` - `from wit_world.imports import types`, then
  `types.Sample(signal=..., value=..., t=...)`. (This split - one
  top-level module per exported *interface*, versus a single flat `app.py`
  - isn't documented anywhere obvious; getting it wrong fails with
  `ModuleNotFoundError: No module named 'decoder'` at componentize time,
  which is how it was worked out for this repo.)

Build with:

```sh
pip install componentize-py
scripts/build-can-dbc-py.sh
```

`-s`/`--stub-wasi` in that script **is not optional**: componentize-py's
runtime embeds a real CPython interpreter, which needs WASI imports for
its own bookkeeping (clocks, a PRNG seed, etc.) even if your plugin code
never touches the filesystem. `benchpeek-wasm-host`'s `Linker` registers
no WASI imports at all (same reason `ascii-kv-wasm` stays on
`wasm32-unknown-unknown` rather than `wasm32-wasip2`), so an un-stubbed
component fails to instantiate. `-s` replaces every WASI import with a
trapping stub, making the component fully self-contained.

Other languages with component-model tooling (C via `wit-bindgen` + a WASI
SDK, Go via TinyGo, JS via `jco`) should work the same way in principle -
implement the `decoder` resource, componentize, apply whatever that
toolchain's equivalent of "no WASI imports" is - but only the Rust and
Python paths above have actually been built and tested against this
host.

## Testing your plugin

Load the real `.wasm` from a Rust integration test, the same way
`crates/benchpeek-wasm-host/tests/decode.rs` and `tests/can_dbc.rs` do -
this exercises the exact same `wasmtime` component instantiation and call
path the app uses, not just your language's own unit tests:

```rust
let mut wasm = benchpeek_wasm_host::WasmDecoder::load("target/plugins/your-plugin.wasm")?;
// or, with config:
let mut wasm = benchpeek_wasm_host::WasmDecoder::load_with_config(path, Some(config_str))?;
assert_eq!(wasm.name(), "your-plugin");
let samples = wasm.decode(b"...", 0.0);
```

`cargo test -p benchpeek-wasm-host` runs these once the `.wasm` exists;
build it first (`scripts/build-plugin.sh` or `scripts/build-can-dbc-py.sh`,
or your own toolchain's equivalent).

## Constraints, and why

- **No host imports.** `benchpeek-wasm-host`'s `Linker::new(&engine)` never
  registers anything - a plugin component must be fully self-contained
  (no WASI, no custom host functions). This keeps the sandbox boundary
  simple and the host trustable regardless of what language produced the
  component; it's also why `config` (a plain string in, no callback out)
  is the only way to hand a plugin external data.
- **`decode` owns all buffering.** There's no separate "flush" or
  "end-of-stream" call - a plugin must be able to make progress with
  whatever prefix of a frame/line it's been given so far, and hold the
  rest for next time (see "byte-stream or frame-shaped protocols" above).
- **One instantiation per `Source` start.** `WasmDecoder::load[_with_config]`
  instantiates fresh each time the app's Start/Open/Replay button is
  clicked - so `constructor` is the right place to reset any DBC/config
  parsing, but don't assume state survives a disconnect/reconnect.

## Using a plugin once built

Point the app at it: DECODER panel → **WASM plugin** → **Path** to the
`.wasm`, optionally **Config** to a file whose contents get passed to
`constructor` → start any source. See the README's CAN section for a
concrete example (`can-dbc-py` plus an arbitrary `.dbc` file).
