//! Loads a `benchpeek:decoder` WASM component from disk and exposes it as
//! a plain `benchpeek_plugin::Decoder`, so the app can treat a sandboxed
//! plugin exactly like the compiled-in one.

use std::path::Path;

use benchpeek_plugin::{Decoder, Sample, SignalMeta};
use wasmtime::component::{Component, Linker, ResourceAny};
use wasmtime::error::Context;
use wasmtime::{Config, Engine, Store};

wasmtime::component::bindgen!({
    world: "decoder-plugin",
    path: "../../wit",
});

use benchpeek::decoder::types::{Sample as WitSample, SignalMeta as WitSignalMeta};

/// A decoder backed by an instantiated WASM component.
pub struct WasmDecoder {
    store: Store<()>,
    bindings: DecoderPlugin,
    resource: ResourceAny,
    name: String,
    signals: Vec<SignalMeta>,
}

impl WasmDecoder {
    /// Load and instantiate a `.wasm` component file, then construct its
    /// exported `decoder` resource with no config. `name()`/`signals()` are
    /// cached here since the `Decoder` trait exposes them without store
    /// access.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::load_with_config(path, None)
    }

    /// Same as [`Self::load`], but passes `config` to the constructor -
    /// e.g. a DBC file's contents for a decoder that otherwise falls back
    /// to some baked-in default. Opaque to the host; a decoder with
    /// nothing to configure just ignores it.
    pub fn load_with_config(path: impl AsRef<Path>, config: Option<&str>) -> anyhow::Result<Self> {
        Self::load_inner(path.as_ref(), config).map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    fn load_inner(path: &Path, config: Option<&str>) -> wasmtime::Result<Self> {
        let mut wasm_config = Config::new();
        wasm_config.wasm_component_model(true);
        let engine = Engine::new(&wasm_config)?;

        let component = Component::from_file(&engine, path)
            .with_context(|| format!("loading component {}", path.display()))?;
        let linker = Linker::new(&engine);
        let mut store = Store::new(&engine, ());
        let bindings = DecoderPlugin::instantiate(&mut store, &component, &linker)
            .context("instantiating decoder-plugin component")?;

        let decoder = bindings.benchpeek_decoder_decoder().decoder();
        let resource = decoder
            .call_constructor(&mut store, config)
            .context("calling decoder constructor")?;
        let name = decoder
            .call_name(&mut store, resource)
            .context("calling decoder.name")?;
        let signals = decoder
            .call_signals(&mut store, resource)
            .context("calling decoder.signals")?
            .into_iter()
            .map(from_wit_meta)
            .collect();

        Ok(Self {
            store,
            bindings,
            resource,
            name,
            signals,
        })
    }
}

impl Decoder for WasmDecoder {
    fn name(&self) -> &str {
        &self.name
    }

    fn signals(&self) -> Vec<SignalMeta> {
        self.signals.clone()
    }

    fn decode(&mut self, bytes: &[u8], t: f64) -> Vec<Sample> {
        let decoder = self.bindings.benchpeek_decoder_decoder().decoder();
        match decoder.call_decode(&mut self.store, self.resource, bytes, t) {
            Ok(samples) => samples.into_iter().map(from_wit_sample).collect(),
            Err(e) => {
                eprintln!("benchpeek-wasm-host: decode call failed: {e:#}");
                Vec::new()
            }
        }
    }
}

fn from_wit_meta(m: WitSignalMeta) -> SignalMeta {
    SignalMeta {
        name: m.name,
        unit: m.unit,
        min: m.min,
        max: m.max,
    }
}

fn from_wit_sample(s: WitSample) -> Sample {
    Sample {
        signal: s.signal,
        value: s.value,
        t: s.t,
    }
}
