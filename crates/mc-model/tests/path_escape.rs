//! Phase 3C MC2022 path-escape variant tests
//! (per ADR-0006 acceptance amendment #18).
//!
//! `canonical_inputs.source` paths are resolved relative to the YAML
//! model file's directory. Paths that escape that directory tree
//! (`../foo.csv`, absolute paths) are rejected with MC2022 carrying a
//! "path-escape" message variant — never read from disk.

use std::path::Path;

use mc_model::ValidationError;

fn build_yaml_with_source(source: &str) -> String {
    format!(
        r#"model_format_version: 1
metadata:
  name: "PathEscapeNeg"
  description: "x"
dimensions:
  - {{ name: "Scenario", description: "x", kind: "Scenario", elements: [{{ name: "Base", scenario_meta: "Default" }}] }}
  - {{ name: "Version",  description: "x", kind: "Version",  elements: [{{ name: "Working", version_state: "Draft" }}] }}
  - {{ name: "Measure",  description: "x", kind: "Measure",  elements: [] }}
measures:
  - {{ name: "Spend", description: "x", role: "Input", data_type: "F64", aggregation: "Sum" }}
rules: []
canonical_inputs:
  source: "{source}"
  columns: ["Scenario", "Version", "Measure", "value"]
"#
    )
}

fn run_with_source_in_dir(source: &str, model_dir: &Path) -> Vec<ValidationError> {
    let yaml = build_yaml_with_source(source);
    let parsed = mc_model::parse(&yaml, Some(model_dir.join("m.yaml").display().to_string()))
        .expect("parse");
    let validated = mc_model::validate(parsed).expect("validate");
    mc_model::resolve_inputs(&validated, Some(model_dir))
        .err()
        .unwrap_or_default()
}

#[test]
fn dotdot_segment_rejected_with_path_escape_message() {
    // Use a temp dir as the "model directory"; reference a path that
    // escapes it via `..`.
    let tmp =
        std::env::temp_dir().join(format!("mc_phase3c_pe_{}_{}", std::process::id(), "dotdot"));
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");
    let errs = run_with_source_in_dir("../escape.csv", &tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        errs.iter().any(|e| e.code() == "MC2022"
            && e.to_string().contains("path-escape")
            && e.to_string().contains("..")),
        "expected MC2022 with path-escape message; got: {errs:?}"
    );
}

#[test]
fn absolute_path_rejected_with_path_escape_message() {
    let tmp = std::env::temp_dir().join(format!("mc_phase3c_pe_{}_{}", std::process::id(), "abs"));
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");
    // Use a clearly-absolute path that exists on every Unix system; on
    // Windows this would need a different value, but the test is a
    // negative test so the path doesn't have to actually exist.
    #[cfg(unix)]
    let abs = "/etc/hosts";
    #[cfg(windows)]
    let abs = "C:\\Windows\\system.ini";

    let errs = run_with_source_in_dir(abs, &tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        errs.iter().any(|e| e.code() == "MC2022"
            && e.to_string().contains("path-escape")
            && e.to_string().contains("absolute")),
        "expected MC2022 with absolute-path-escape message; got: {errs:?}"
    );
}

#[test]
fn sibling_path_in_same_directory_resolves_cleanly() {
    // Positive control: a sibling CSV inside the same dir resolves
    // without MC2022.
    let tmp = std::env::temp_dir().join(format!(
        "mc_phase3c_pe_{}_{}",
        std::process::id(),
        "sibling"
    ));
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");
    let csv_path = tmp.join("ok.csv");
    std::fs::write(
        csv_path,
        "Scenario,Version,Measure,value\nBase,Working,Spend,42\n",
    )
    .expect("write csv");

    let yaml = build_yaml_with_source("ok.csv");
    let parsed =
        mc_model::parse(&yaml, Some(tmp.join("m.yaml").display().to_string())).expect("parse");
    let validated = mc_model::validate(parsed).expect("validate");
    let result = mc_model::resolve_inputs(&validated, Some(&tmp));

    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        result.is_ok(),
        "sibling-path resolve should succeed; got errors: {:?}",
        result.err()
    );
}
