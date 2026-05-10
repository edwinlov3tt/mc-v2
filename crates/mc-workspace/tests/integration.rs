//! Integration tests for mc-workspace.
//!
//! Covers: $ref resolution, missing refs, broken links, standalone cube
//! regression, element_type validation, workspace validate/lint/inspect.

use std::fs;
use std::path::{Path, PathBuf};

/// Helper: create a temporary workspace directory with given files.
struct TempWorkspace {
    dir: PathBuf,
}

impl TempWorkspace {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mc-ws-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn write_file(&self, relative: &str, content: &str) {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn minimal_cube_yaml() -> &'static str {
    r#"
model_format_version: 1
metadata:
  name: "TestCube"
  description: "A test cube."
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    description: "Scenario dim."
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    description: "Version dim."
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Standard"
    description: "Time dim."
    elements:
      - { name: "Jan_2026" }
  - name: "Channel"
    kind: "Standard"
    description: "Channel dim."
    elements:
      - { name: "Paid_Search" }
  - name: "Market"
    kind: "Standard"
    description: "Market dim."
    elements:
      - { name: "Tampa" }
  - name: "Measure"
    kind: "Measure"
    description: "Measure dim."
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum", description: "Spend." }
"#
}

// -----------------------------------------------------------------------
// $ref resolution tests
// -----------------------------------------------------------------------

#[test]
fn ref_resolution_inlines_catalog_elements() {
    let ws = TempWorkspace::new("ref-inline");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
shared_dimensions:
  - id: "Channel"
    source: "catalogs/channels.yaml"
cubes:
  - path: "cubes/test.yaml"
"#,
    );
    ws.write_file(
        "catalogs/channels.yaml",
        r#"
catalog_format_version: 1
dimension: "Channel"
elements:
  - { name: "Paid_Search" }
  - { name: "Display" }
hierarchy: []
"#,
    );
    // Cube with $ref for Channel.
    ws.write_file(
        "cubes/test.yaml",
        r#"
model_format_version: 1
metadata:
  name: "TestCube"
  description: "Test cube with $ref."
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    description: "Scenario dim."
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    description: "Version dim."
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Standard"
    description: "Time dim."
    elements:
      - { name: "Jan_2026" }
  - name: "Channel"
    kind: "Standard"
    description: "Channel dim."
    $ref: "workspace://shared_dimensions/Channel"
  - name: "Market"
    kind: "Standard"
    description: "Market dim."
    elements:
      - { name: "Tampa" }
  - name: "Measure"
    kind: "Measure"
    description: "Measure dim."
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum", description: "Spend." }
"#,
    );

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());

    // Should have no errors — $ref resolved successfully.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == mc_workspace::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected no errors, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn missing_ref_target_returns_mc5003() {
    let ws = TempWorkspace::new("missing-ref");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
shared_dimensions: []
cubes:
  - path: "cubes/test.yaml"
"#,
    );
    ws.write_file(
        "cubes/test.yaml",
        r#"
model_format_version: 1
metadata:
  name: "TestCube"
  description: "Test."
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    description: "S."
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    description: "V."
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Standard"
    description: "T."
    elements:
      - { name: "Jan_2026" }
  - name: "Channel"
    kind: "Standard"
    description: "C."
    $ref: "workspace://shared_dimensions/NoSuchCatalog"
  - name: "Market"
    kind: "Standard"
    description: "M."
    elements:
      - { name: "Tampa" }
  - name: "Measure"
    kind: "Measure"
    description: "Meas."
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum", description: "S." }
"#,
    );

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5003 = diags.iter().any(|d| d.code == "MC5003");
    assert!(has_mc5003, "expected MC5003, got: {:?}", diags);
}

// -----------------------------------------------------------------------
// Broken link test
// -----------------------------------------------------------------------

#[test]
fn broken_link_returns_mc5004() {
    let ws = TempWorkspace::new("broken-link");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "cubes/test.yaml"
    name: "TestCube"
links:
  - from_cube: "NoSuchCube"
    from_measure: "Spend"
    to_cube: "TestCube"
    to_measure: "Spend"
    kind: ReadOnly
"#,
    );
    ws.write_file("cubes/test.yaml", minimal_cube_yaml());

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5004 = diags.iter().any(|d| d.code == "MC5004");
    assert!(has_mc5004, "expected MC5004, got: {:?}", diags);
}

// -----------------------------------------------------------------------
// Missing cube file test
// -----------------------------------------------------------------------

#[test]
fn missing_cube_file_returns_mc5002() {
    let ws = TempWorkspace::new("missing-cube");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "cubes/nonexistent.yaml"
"#,
    );

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5002 = diags.iter().any(|d| d.code == "MC5002");
    assert!(has_mc5002, "expected MC5002, got: {:?}", diags);
}

// -----------------------------------------------------------------------
// Standalone cube regression — standalone cubes without workspaces
// must continue working identically.
// -----------------------------------------------------------------------

#[test]
fn standalone_cube_without_workspace_works() {
    // The existing Acme YAML loads through mc_model::load without a workspace.
    let acme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates/mc-model/examples/acme.yaml");
    if acme_path.exists() {
        let result = mc_model::load(&acme_path);
        assert!(
            result.is_ok(),
            "standalone cube must still load: {:?}",
            result.err()
        );
    }
}

// -----------------------------------------------------------------------
// Standalone cube WITH $ref but no workspace → informative error
// -----------------------------------------------------------------------

#[test]
fn standalone_cube_with_ref_but_no_workspace() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "TestWithRef"
dimensions:
  - name: "Channel"
    kind: "Standard"
    $ref: "workspace://shared_dimensions/Channel"
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
"#;
    // mc-model's serde deny_unknown_fields should reject $ref since
    // ParsedDimension doesn't have that field. This is the expected behavior.
    let result = mc_model::load_str(yaml, Some("standalone-with-ref".to_string()));
    assert!(
        result.is_err(),
        "expected error for unresolved $ref in standalone mode"
    );
}

// -----------------------------------------------------------------------
// element_type validation
// -----------------------------------------------------------------------

#[test]
fn element_type_numeric_validates_correctly() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "NumericTest"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Score"
    kind: "Standard"
    element_type: "numeric"
    elements:
      - { name: "100" }
      - { name: "200" }
      - { name: "300" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Value", role: "Input", data_type: "F64", aggregation: "Sum" }
"#;
    let result = mc_model::load_str(yaml, Some("numeric-test".into()));
    assert!(
        result.is_ok(),
        "numeric element_type should validate: {:?}",
        result.err()
    );
}

#[test]
fn element_type_numeric_rejects_non_numbers() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "NumericBadTest"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Score"
    kind: "Standard"
    element_type: "numeric"
    elements:
      - { name: "NotANumber" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Value", role: "Input", data_type: "F64", aggregation: "Sum" }
"#;
    let result = mc_model::load_str(yaml, Some("numeric-bad-test".into()));
    assert!(
        result.is_err(),
        "non-numeric element with element_type: numeric should fail"
    );
}

#[test]
fn element_type_date_validates_correctly() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "DateTest"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Standard"
    element_type: "date"
    elements:
      - { name: "Jan_2026" }
      - { name: "Feb_2026" }
      - { name: "2026-03-01" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Value", role: "Input", data_type: "F64", aggregation: "Sum" }
"#;
    let result = mc_model::load_str(yaml, Some("date-test".into()));
    assert!(
        result.is_ok(),
        "date element_type should validate: {:?}",
        result.err()
    );
}

// -----------------------------------------------------------------------
// Unused catalog warning (MC5006)
// -----------------------------------------------------------------------

#[test]
fn unused_catalog_returns_mc5006() {
    let ws = TempWorkspace::new("unused-catalog");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
shared_dimensions:
  - id: "UnusedCatalog"
    source: "catalogs/unused.yaml"
cubes:
  - path: "cubes/test.yaml"
"#,
    );
    ws.write_file(
        "catalogs/unused.yaml",
        r#"
catalog_format_version: 1
dimension: "Unused"
elements:
  - { name: "A" }
hierarchy: []
"#,
    );
    ws.write_file("cubes/test.yaml", minimal_cube_yaml());

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5006 = diags.iter().any(|d| d.code == "MC5006");
    assert!(has_mc5006, "expected MC5006 for unused catalog");
}

// -----------------------------------------------------------------------
// No golden suites (MC5008)
// -----------------------------------------------------------------------

#[test]
fn no_golden_suites_returns_mc5008() {
    let ws = TempWorkspace::new("no-goldens");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "cubes/test.yaml"
"#,
    );
    ws.write_file("cubes/test.yaml", minimal_cube_yaml());

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5008 = diags.iter().any(|d| d.code == "MC5008");
    assert!(has_mc5008, "expected MC5008 for no golden suites");
}

// -----------------------------------------------------------------------
// Workspace inspect produces coherent output
// -----------------------------------------------------------------------

#[test]
fn inspect_produces_text_output() {
    let ws = TempWorkspace::new("inspect-text");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test-ws"
id: "test-ws-id"
description: "A test workspace."
cubes:
  - path: "cubes/test.yaml"
    name: "TestCube"
"#,
    );
    ws.write_file("cubes/test.yaml", minimal_cube_yaml());

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let summary = mc_workspace::inspect_workspace(&workspace, ws.path());
    let text = mc_workspace::inspect_text(&summary, &diags);

    assert!(text.contains("Workspace: test-ws"));
    assert!(text.contains("ID: test-ws-id"));
    assert!(text.contains("Cubes: 1"));
}

// -----------------------------------------------------------------------
// Init scaffolds workspace.yaml
// -----------------------------------------------------------------------

#[test]
fn init_creates_workspace_yaml() {
    let dir = std::env::temp_dir().join(format!("mc-ws-init-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    mc_workspace::init_workspace("my-project", &dir, Some("marketing-mix")).unwrap();

    let manifest = dir.join("workspace.yaml");
    assert!(manifest.exists(), "workspace.yaml should be created");
    let content = fs::read_to_string(&manifest).unwrap();
    assert!(content.contains("my-project"));
    assert!(content.contains("marketing-mix"));

    // Cleanup.
    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// Acme workspace example validates
// -----------------------------------------------------------------------

#[test]
fn acme_workspace_example_validates() {
    let acme_ws = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/acme-workspace");
    if !acme_ws.exists() {
        // Skip if the example hasn't been created yet.
        return;
    }

    let workspace = mc_workspace::parse_workspace(&acme_ws).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, &acme_ws);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == mc_workspace::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Acme workspace should validate cleanly, got errors: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// -----------------------------------------------------------------------
// Lint: cube without description (MC5012)
// -----------------------------------------------------------------------

#[test]
fn lint_cube_no_description_returns_mc5012() {
    let ws = TempWorkspace::new("lint-no-desc");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "cubes/test.yaml"
    name: "NoDescript"
"#,
    );
    // Cube without metadata.description.
    ws.write_file(
        "cubes/test.yaml",
        r#"
model_format_version: 1
metadata:
  name: "TestCube"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    description: "S."
    elements:
      - { name: "Baseline", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    description: "V."
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Standard"
    description: "T."
    elements:
      - { name: "Jan_2026" }
  - name: "Channel"
    kind: "Standard"
    description: "C."
    elements:
      - { name: "Paid_Search" }
  - name: "Market"
    kind: "Standard"
    description: "M."
    elements:
      - { name: "Tampa" }
  - name: "Measure"
    kind: "Measure"
    description: "Meas."
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum", description: "S." }
"#,
    );

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::lint_workspace(&workspace, ws.path());
    let has_mc5012 = diags.iter().any(|d| d.code == "MC5012");
    assert!(has_mc5012, "expected MC5012 for cube without description");
}

// -----------------------------------------------------------------------
// Diagnostic sort order
// -----------------------------------------------------------------------

#[test]
fn diagnostics_sort_by_severity_then_code() {
    let mut diags = vec![
        mc_workspace::WorkspaceDiagnostic {
            code: "MC5008",
            severity: mc_workspace::Severity::Info,
            message: "info".into(),
        },
        mc_workspace::WorkspaceDiagnostic {
            code: "MC5002",
            severity: mc_workspace::Severity::Error,
            message: "error".into(),
        },
        mc_workspace::WorkspaceDiagnostic {
            code: "MC5006",
            severity: mc_workspace::Severity::Warning,
            message: "warning".into(),
        },
    ];
    mc_workspace::sort_diagnostics(&mut diags);
    assert_eq!(diags[0].code, "MC5002"); // Error first
    assert_eq!(diags[1].code, "MC5006"); // Warning next
    assert_eq!(diags[2].code, "MC5008"); // Info last
}

// -----------------------------------------------------------------------
// Duplicate cube path detection (MC5009)
// -----------------------------------------------------------------------

#[test]
fn duplicate_cube_path_returns_mc5009() {
    let ws = TempWorkspace::new("dup-cube");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "cubes/test.yaml"
  - path: "cubes/test.yaml"
"#,
    );
    ws.write_file("cubes/test.yaml", minimal_cube_yaml());

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());
    let has_mc5009 = diags.iter().any(|d| d.code == "MC5009");
    assert!(
        has_mc5009,
        "expected MC5009 for duplicate cube path, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// -----------------------------------------------------------------------
// Path escape detection (MC5013)
// -----------------------------------------------------------------------

#[test]
fn path_escape_returns_mc5013() {
    let ws = TempWorkspace::new("path-escape");
    ws.write_file(
        "workspace.yaml",
        r#"
workspace_format_version: 1
name: "test"
id: "test"
cubes:
  - path: "../../../etc/passwd"
"#,
    );

    let workspace = mc_workspace::parse_workspace(ws.path()).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, ws.path());

    // Should get MC5002 (file not found) or MC5013 (path escape).
    // The file likely doesn't exist as a valid cube, so MC5002 fires
    // before the path-escape check. But if the file DID exist outside
    // the workspace, MC5013 would catch it. Let's test with a real
    // escape that does exist: a sibling temp directory.
    let has_escape_or_missing = diags
        .iter()
        .any(|d| d.code == "MC5002" || d.code == "MC5013");
    assert!(
        has_escape_or_missing,
        "expected MC5002 or MC5013 for path escape, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn path_escape_with_real_file_returns_mc5013() {
    // Create two sibling workspaces; workspace A references a cube in workspace B.
    let parent = std::env::temp_dir().join(format!("mc-ws-escape-{}", std::process::id()));
    let _ = fs::remove_dir_all(&parent);

    let ws_a = parent.join("ws-a");
    let ws_b = parent.join("ws-b");
    fs::create_dir_all(ws_a.join("cubes")).unwrap();
    fs::create_dir_all(ws_b.join("cubes")).unwrap();

    // Put a valid cube in ws-b.
    fs::write(ws_b.join("cubes/stolen.yaml"), minimal_cube_yaml()).unwrap();

    // ws-a references ws-b's cube via path escape.
    fs::write(
        ws_a.join("workspace.yaml"),
        r#"
workspace_format_version: 1
name: "ws-a"
id: "ws-a"
cubes:
  - path: "../ws-b/cubes/stolen.yaml"
"#,
    )
    .unwrap();

    let workspace = mc_workspace::parse_workspace(&ws_a).unwrap();
    let diags = mc_workspace::validate_workspace(&workspace, &ws_a);
    let has_mc5013 = diags.iter().any(|d| d.code == "MC5013");
    assert!(
        has_mc5013,
        "expected MC5013 for cross-workspace path escape, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );

    let _ = fs::remove_dir_all(&parent);
}
