//! WASM component wrapper around the `ascii-kv` decoder. Proves that a
//! plain `benchpeek_plugin::Decoder` implementation can be shipped as a
//! sandboxed component and loaded by the host at runtime, with the exact
//! same behavior as the compiled-in version.

use std::cell::RefCell;

use benchpeek_plugin::Decoder as _;

wit_bindgen::generate!({
    world: "decoder-plugin",
    path: "../../wit",
});

use exports::benchpeek::decoder::decoder::{Guest, GuestDecoder, Sample, SignalMeta};

struct Component;

pub struct AsciiKvResource {
    inner: RefCell<ascii_kv::AsciiKv>,
}

impl Guest for Component {
    type Decoder = AsciiKvResource;
}

impl GuestDecoder for AsciiKvResource {
    fn new(_config: Option<String>) -> Self {
        // ascii-kv has nothing to configure: KEY=VALUE parsing is fixed.
        Self {
            inner: RefCell::new(ascii_kv::AsciiKv::new()),
        }
    }

    fn name(&self) -> String {
        self.inner.borrow().name().to_string()
    }

    fn signals(&self) -> Vec<SignalMeta> {
        self.inner
            .borrow()
            .signals()
            .into_iter()
            .map(|m| SignalMeta {
                name: m.name,
                unit: m.unit,
                min: m.min,
                max: m.max,
            })
            .collect()
    }

    fn decode(&self, bytes: Vec<u8>, t: f64) -> Vec<Sample> {
        self.inner
            .borrow_mut()
            .decode(&bytes, t)
            .into_iter()
            .map(|s| Sample {
                signal: s.signal,
                value: s.value,
                t: s.t,
            })
            .collect()
    }
}

export!(Component);
