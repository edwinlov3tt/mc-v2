//! Phase 4A: byte-identity test for the Acme example shipped under
//! `mosaic-plugin/examples/models/`.
//!
//! Asserts that:
//!
//! 1. `mosaic-plugin/examples/models/acme-marketing.yaml` is byte-identical
//!    to `crates/mc-model/examples/acme.yaml` — the source-of-truth.
//! 2. `mosaic-plugin/examples/models/acme.inputs.csv` is byte-identical
//!    to `crates/mc-model/examples/acme.inputs.csv`.
//!
//! Per the Phase 4A handoff: the plugin example is a copy for plugin
//! self-containment, NOT a divergent fork. If the source-of-truth changes
//! in a future phase, the plugin copy updates in lockstep.
//!
//! Deviation: the handoff manifest names the inputs CSV
//! `acme-marketing.inputs.csv`, but the YAML's `source:` field references
//! `"acme.inputs.csv"`. To keep the YAML byte-identical AND let
//! `mc demo --model mosaic-plugin/examples/models/acme-marketing.yaml`
//! resolve its inputs at runtime, the CSV in the plugin keeps the
//! original filename. Documented in the Phase 4A completion report.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(path: PathBuf) -> Vec<u8> {
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"))
}

#[test]
fn plugin_yaml_is_byte_identical_to_source_yaml() {
    let root = workspace_root();
    let source = read(root.join("crates/mc-model/examples/acme.yaml"));
    let plugin = read(root.join("mosaic-plugin/examples/models/acme-marketing.yaml"));
    assert_eq!(
        source.len(),
        plugin.len(),
        "byte length mismatch: source {} vs plugin {}",
        source.len(),
        plugin.len()
    );
    assert_eq!(
        source, plugin,
        "mosaic-plugin/examples/models/acme-marketing.yaml must be byte-identical to crates/mc-model/examples/acme.yaml"
    );
}

#[test]
fn plugin_inputs_csv_is_byte_identical_to_source_csv() {
    let root = workspace_root();
    let source = read(root.join("crates/mc-model/examples/acme.inputs.csv"));
    let plugin = read(root.join("mosaic-plugin/examples/models/acme.inputs.csv"));
    assert_eq!(
        source.len(),
        plugin.len(),
        "byte length mismatch: source {} vs plugin {}",
        source.len(),
        plugin.len()
    );
    assert_eq!(
        source, plugin,
        "mosaic-plugin/examples/models/acme.inputs.csv must be byte-identical to crates/mc-model/examples/acme.inputs.csv"
    );
}
