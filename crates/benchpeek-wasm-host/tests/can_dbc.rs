//! Loads the real `can-dbc-py` component (built by
//! `scripts/build-can-dbc-py.sh`, requires `componentize-py` on PATH) and
//! checks it decodes a CAN frame against its embedded DBC correctly,
//! proving the plugin boundary works for a non-Rust decoder too.

use benchpeek_plugin::Decoder;
use benchpeek_wasm_host::WasmDecoder;

fn plugin_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plugins/can-dbc-py.wasm")
}

/// Frame for CAN ID 0x301: VBAT/RPM Intel (little-endian) unsigned, TEMP
/// Motorola (big-endian) signed - exercises both DBC byte orders in one
/// frame. Expected values (hand-computed from the DBC in decoder.py):
/// VBAT = 1260 * 0.01       = 12.6
/// RPM  =  850 * 1          = 850.0
/// TEMP =  812 * 0.1 - 40   = 41.2
fn test_frame_line() -> &'static [u8] {
    b"301#EC045203032C0000\n"
}

#[test]
fn decodes_dbc_signals_from_a_can_frame() {
    let path = plugin_path();
    assert!(
        path.exists(),
        "missing {path:?} - run `scripts/build-can-dbc-py.sh` first"
    );

    let mut wasm = WasmDecoder::load(&path).expect("load can-dbc-py component");
    assert_eq!(wasm.name(), "can-dbc-py");

    let signals = wasm.signals();
    assert_eq!(signals.len(), 3);
    assert_eq!(signals[0].name, "VBAT");
    assert_eq!(signals[0].unit.as_deref(), Some("V"));
    assert_eq!(signals[2].name, "TEMP");
    assert_eq!(signals[2].min, Some(-40.0));

    let out = wasm.decode(test_frame_line(), 2.5);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].signal, "VBAT");
    assert!((out[0].value - 12.6).abs() < 1e-9);
    assert_eq!(out[1].signal, "RPM");
    assert_eq!(out[1].value, 850.0);
    assert_eq!(out[2].signal, "TEMP");
    assert!((out[2].value - 41.2).abs() < 1e-9);
    assert_eq!(out[2].t, 2.5);
}

#[test]
fn ignores_frames_for_unknown_ids() {
    let path = plugin_path();
    assert!(path.exists(), "missing {path:?}");

    let mut wasm = WasmDecoder::load(&path).expect("load can-dbc-py component");
    let out = wasm.decode(b"7DF#0102030405060708\n", 0.0);
    assert!(out.is_empty());
}

/// The host, not the component, owns reading an arbitrary `.dbc` file
/// (components built with `--stub-wasi` have no filesystem access) and
/// passes its text through as the constructor's `config` - this is what
/// lets `can-dbc-py` decode a real vehicle's DBC instead of only its
/// baked-in worked example, without a rebuild.
#[test]
fn accepts_a_custom_dbc_via_runtime_config() {
    let path = plugin_path();
    assert!(path.exists(), "missing {path:?}");

    let custom_dbc = r#"
BO_ 512 TELEMETRY: 8 ECU
 SG_ SPEED : 0|16@1+ (0.5,0) [0|1000] "km/h" ECU
"#;
    let mut wasm = WasmDecoder::load_with_config(&path, Some(custom_dbc))
        .expect("load can-dbc-py component with custom config");

    let signals = wasm.signals();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "SPEED");
    assert_eq!(signals[0].unit.as_deref(), Some("km/h"));

    // The default embedded VBAT/RPM/TEMP frame must decode to nothing here
    // - proof this really replaced the baked-in DBC rather than merging
    // with it.
    assert!(wasm.decode(test_frame_line(), 0.0).is_empty());

    let out = wasm.decode(b"200#6400000000000000\n", 1.0);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].signal, "SPEED");
    assert_eq!(out[0].value, 50.0);
}

#[test]
fn buffers_partial_lines_across_calls() {
    let path = plugin_path();
    assert!(path.exists(), "missing {path:?}");

    let mut wasm = WasmDecoder::load(&path).expect("load can-dbc-py component");
    let line = test_frame_line();
    let (head, tail) = line.split_at(10);
    assert!(wasm.decode(head, 0.0).is_empty());
    let out = wasm.decode(tail, 1.0);
    assert_eq!(out.len(), 3);
}
