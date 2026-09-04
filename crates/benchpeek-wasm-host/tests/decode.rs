//! Loads the real `ascii-kv` component (built by `scripts/build-plugin.sh
//! ascii-kv-wasm ascii-kv`) and checks it decodes exactly like the
//! compiled-in version, proving the WASM plugin boundary round-trips data
//! correctly end to end.

use benchpeek_plugin::Decoder;
use benchpeek_wasm_host::WasmDecoder;

fn plugin_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plugins/ascii-kv.wasm")
}

#[test]
fn decodes_same_as_builtin() {
    let path = plugin_path();
    assert!(
        path.exists(),
        "missing {path:?} - run `scripts/build-plugin.sh ascii-kv-wasm ascii-kv` first"
    );

    let mut wasm = WasmDecoder::load(&path).expect("load ascii-kv component");
    assert_eq!(wasm.name(), "ascii-kv");
    assert!(wasm.signals().is_empty());

    let mut builtin = ascii_kv::AsciiKv::new();

    let line = b"VBAT=12.340 RPM=850.0 TEMP=41.20\n";
    let wasm_out = wasm.decode(line, 1.5);
    let builtin_out = builtin.decode(line, 1.5);

    assert_eq!(wasm_out.len(), 3);
    assert_eq!(wasm_out.len(), builtin_out.len());
    for (w, b) in wasm_out.iter().zip(builtin_out.iter()) {
        assert_eq!(w.signal, b.signal);
        assert_eq!(w.value, b.value);
        assert_eq!(w.t, b.t);
    }
    assert_eq!(wasm_out[0].signal, "VBAT");
    assert_eq!(wasm_out[0].value, 12.34);
}

#[test]
fn buffers_partial_lines_across_calls() {
    let path = plugin_path();
    assert!(path.exists(), "missing {path:?}");

    let mut wasm = WasmDecoder::load(&path).expect("load ascii-kv component");
    assert!(wasm.decode(b"TEMP=", 0.0).is_empty());
    let out = wasm.decode(b"41.5\n", 1.0);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].signal, "TEMP");
    assert_eq!(out[0].value, 41.5);
    assert_eq!(out[0].t, 1.0);
}
