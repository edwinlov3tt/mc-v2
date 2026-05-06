//! Phase 6A: integration tests for the 7 new Agent-Ready CLI verbs.
//!
//! Each test spawns `mc` as a subprocess (same pattern as mcp_smoke.rs)
//! and exercises a happy-path invocation against the Acme model.

use std::path::PathBuf;
use std::process::{Command, Output};

fn mc_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mc"))
}

fn acme_yaml() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // mc-cli -> crates
    p.pop(); // crates -> workspace root
    p.push("crates");
    p.push("mc-model");
    p.push("examples");
    p.push("acme.yaml");
    p
}

/// Run `mc` with args. Returns the Output (stdout, stderr, status).
fn run_mc(args: &[&str]) -> Output {
    Command::new(mc_binary())
        .args(args)
        .output()
        .expect("failed to spawn mc binary")
}

/// Parse stdout as a JSON value (panics with context if invalid).
fn parse_json(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stdout);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("output is not valid JSON: {e}\nstdout:\n{text}");
    })
}

// ===========================================================================
// Query tests
// ===========================================================================

#[test]
fn test_query_with_coord() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--coord",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    let value = json.get("value").expect("missing 'value' field");
    // Acme Spend for Jan_2026/Paid_Search/Tampa = 10500
    let v = value.as_f64().expect("value must be a number");
    assert!((v - 10500.0).abs() < 1e-9, "expected 10500, got {v}");
}

#[test]
fn test_query_with_where_filter() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--where",
        "Spend > 10000",
        "--format",
        "json",
        "--limit",
        "5",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    let results = json.get("results").and_then(|r| r.as_array());
    assert!(results.is_some(), "output must have 'results' array");
    assert!(
        !results.unwrap().is_empty(),
        "Spend > 10000 should match at least one row"
    );
}

#[test]
fn test_query_with_aggregate() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--aggregate",
        "mean(Spend)",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    let aggregates = json.get("aggregates").expect("missing 'aggregates'");
    assert!(!aggregates.is_null(), "aggregates should not be null");
    let mean_spend = aggregates.get("mean(Spend)").and_then(|v| v.as_f64());
    assert!(
        mean_spend.is_some() && mean_spend.unwrap() > 0.0,
        "mean(Spend) should be a positive number, got: {:?}",
        mean_spend
    );
}

// ===========================================================================
// Whatif tests
// ===========================================================================

#[test]
fn test_whatif_reports_deltas() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "whatif",
        path.to_str().unwrap(),
        "--set",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--value",
        "20000",
        "--show",
        "Clicks,Revenue",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    // Check that cell_overridden exists with before/after
    let overridden = json
        .get("cell_overridden")
        .expect("missing 'cell_overridden'");
    assert!(overridden.get("before").is_some());
    assert!(overridden.get("after").is_some());
    // Check affected_measures has deltas
    let affected = json
        .get("affected_measures")
        .and_then(|a| a.as_array())
        .expect("missing 'affected_measures' array");
    assert!(!affected.is_empty(), "should have affected measures");
    // Clicks is derived from Spend, so delta should be non-zero
    let clicks_entry = affected
        .iter()
        .find(|m| m.get("measure").and_then(|v| v.as_str()) == Some("Clicks"));
    assert!(
        clicks_entry.is_some(),
        "Clicks should be in affected measures"
    );
    let delta = clicks_entry.unwrap().get("delta").and_then(|v| v.as_f64());
    assert!(
        delta.is_some() && delta.unwrap().abs() > 1e-9,
        "Clicks delta should be non-zero after changing Spend"
    );
}

// ===========================================================================
// Trace tests
// ===========================================================================

#[test]
fn test_trace_returns_tree() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "trace",
        path.to_str().unwrap(),
        "--coord",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Clicks",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    // Phase 6A.1 CRIT-2: every Phase 6A JSON envelope carries
    // schema_version as the first field; trace tree lives under "trace".
    assert_eq!(
        json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0"),
        "missing or wrong schema_version"
    );
    let tree = json.get("trace").expect("missing 'trace' field");
    assert!(tree.get("measure").is_some(), "missing 'measure' field");
    assert!(tree.get("value").is_some(), "missing 'value' field");
    assert!(tree.get("source").is_some(), "missing 'source' field");
    // Clicks is derived, so it should have inputs
    let inputs = tree.get("inputs");
    assert!(
        inputs.is_some() && !inputs.unwrap().is_null(),
        "derived cell trace should have 'inputs'"
    );
}

// ===========================================================================
// Sweep tests
// ===========================================================================

#[test]
fn test_sweep_returns_curve() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "sweep",
        path.to_str().unwrap(),
        "--set",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--range",
        "5000:15000:5000",
        "--metric",
        "mean(Clicks)",
        "--goal",
        "maximize",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    let sweep = json.get("sweep").and_then(|s| s.as_array());
    assert!(sweep.is_some(), "missing 'sweep' array");
    assert_eq!(
        sweep.unwrap().len(),
        3,
        "should have 3 sweep points (5000, 10000, 15000)"
    );
    let optimal = json.get("optimal");
    assert!(
        optimal.is_some() && !optimal.unwrap().is_null(),
        "should report an optimal"
    );
}

// ===========================================================================
// Diff tests
// ===========================================================================

#[test]
fn test_diff_between_scenarios() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "diff",
        path.to_str().unwrap(),
        "--left",
        "Scenario=Baseline",
        "--right",
        "Scenario=Aggressive",
        "--format",
        "json",
        "--limit",
        "10",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    // Verify the diff output has the expected structure
    let changed = json.get("changed_cells").and_then(|c| c.as_u64());
    assert!(changed.is_some(), "missing 'changed_cells' field");
    let top_changes = json.get("top_changes").and_then(|t| t.as_array());
    assert!(top_changes.is_some(), "missing 'top_changes' array");
    let summary = json.get("summary");
    assert!(summary.is_some(), "missing 'summary' field");
}

// ===========================================================================
// Write tests
// ===========================================================================

#[test]
fn test_write_dry_run() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "write",
        path.to_str().unwrap(),
        "--coord",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--value",
        "99999",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "exit code: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);
    assert_eq!(
        json.get("dry_run").and_then(|v| v.as_bool()),
        Some(true),
        "dry_run field should be true"
    );
    assert!(json.get("current_value").is_some());
    assert!(json.get("new_value").is_some());
    // Verify no .tessera/writes.jsonl was created next to the example
    let model_dir = path.parent().unwrap();
    let writes_log = model_dir.join(".tessera").join("writes.jsonl");
    assert!(!writes_log.exists(), "dry-run must NOT create writes.jsonl");
}

// ===========================================================================
// MCP stdout isolation test
// ===========================================================================

#[test]
fn test_mcp_query_does_not_corrupt_stdout() {
    // This is the key test for P0 Fix 1: verify that calling a Phase 6A
    // tool via MCP produces valid JSON-RPC without interleaved output.
    let path = acme_yaml().to_string_lossy().into_owned();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"mosaic.model.query","arguments":{{"path":"{path}","coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend","format":"json"}}}}}}"#
    );

    let mut child = Command::new(mc_binary())
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mc mcp");

    use std::io::Write;
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(req.as_bytes()).expect("write");
    stdin.write_all(b"\n").expect("newline");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout_str.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "MCP response must be exactly one line (no interleaved output). Got {} lines:\n{}",
        lines.len(),
        stdout_str
    );
    // The single line must be valid JSON
    let response: serde_json::Value = serde_json::from_str(lines[0]).unwrap_or_else(|e| {
        panic!("MCP response is not valid JSON: {e}\nline: {}", lines[0]);
    });
    assert_eq!(
        response.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0")
    );
    assert_eq!(response.get("id").and_then(|v| v.as_u64()), Some(1));
    // Should have a result with content
    let result = response.get("result").expect("missing result");
    assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(false));
    // The content text should contain the query result (the coord value 10500)
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .expect("missing content array");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("missing text in content");
    assert!(
        text.contains("10500"),
        "MCP response should contain the query result value 10500, got: {text}"
    );
}

// ===========================================================================
// JSON validity test (all verbs)
// ===========================================================================

#[test]
fn test_all_verbs_json_valid() {
    let path = acme_yaml();
    let path_str = path.to_str().unwrap();
    let coord = "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend";
    let coord_clicks = "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Clicks";

    // Each verb invocation is tested for valid JSON output
    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "model", "query", path_str, "--coord", coord, "--format", "json",
            ],
            "query --coord",
        ),
        (
            &[
                "model",
                "query",
                path_str,
                "--aggregate",
                "mean(Spend)",
                "--format",
                "json",
            ],
            "query --aggregate",
        ),
        (
            &[
                "model", "whatif", path_str, "--set", coord, "--value", "20000", "--show",
                "Clicks", "--format", "json",
            ],
            "whatif",
        ),
        (
            &[
                "model",
                "trace",
                path_str,
                "--coord",
                coord_clicks,
                "--format",
                "json",
            ],
            "trace",
        ),
        (
            &[
                "model",
                "diff",
                path_str,
                "--left",
                "Scenario=Baseline",
                "--right",
                "Scenario=Aggressive",
                "--format",
                "json",
                "--limit",
                "5",
            ],
            "diff",
        ),
        (
            &[
                "model",
                "write",
                path_str,
                "--coord",
                coord,
                "--value",
                "12345",
                "--dry-run",
                "--format",
                "json",
            ],
            "write --dry-run",
        ),
    ];

    for (args, desc) in cases {
        let output = run_mc(args);
        assert!(
            output.status.success(),
            "{desc}: non-zero exit. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout_text = String::from_utf8_lossy(&output.stdout);
        let _: serde_json::Value = serde_json::from_str(&stdout_text).unwrap_or_else(|e| {
            panic!("{desc}: output is not valid JSON: {e}\nstdout:\n{stdout_text}");
        });
    }
}

// ===========================================================================
// Phase 6A.1 Block 2 — envelope-discipline regressions
// ===========================================================================

/// CRIT-2: every Phase 6A verb's `--format json` output carries
/// `schema_version: "1.0"` as the first envelope field, matching the
/// Phase 3B diagnostic-envelope shape and the existing tessera /
/// validate / inspect / lint outputs.
#[test]
fn test_all_phase_6a_verbs_emit_schema_version() {
    let path = acme_yaml();
    let path_str = path.to_str().unwrap();
    let coord = "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend";
    let coord_clicks = "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Clicks";

    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "model", "query", path_str, "--coord", coord, "--format", "json",
            ],
            "query --coord",
        ),
        (
            &[
                "model",
                "query",
                path_str,
                "--aggregate",
                "mean(Spend)",
                "--format",
                "json",
            ],
            "query --aggregate",
        ),
        (
            &[
                "model", "whatif", path_str, "--set", coord, "--value", "20000", "--show",
                "Clicks", "--format", "json",
            ],
            "whatif",
        ),
        (
            &[
                "model",
                "trace",
                path_str,
                "--coord",
                coord_clicks,
                "--format",
                "json",
            ],
            "trace",
        ),
        (
            &[
                "model",
                "sweep",
                path_str,
                "--set",
                coord,
                "--range",
                "5000:15000:5000",
                "--metric",
                "mean(Clicks)",
                "--goal",
                "maximize",
                "--format",
                "json",
            ],
            "sweep",
        ),
        (
            &[
                "model",
                "diff",
                path_str,
                "--left",
                "Scenario=Baseline",
                "--right",
                "Scenario=Aggressive",
                "--format",
                "json",
                "--limit",
                "5",
            ],
            "diff",
        ),
        (
            &[
                "model",
                "write",
                path_str,
                "--coord",
                coord,
                "--value",
                "12345",
                "--dry-run",
                "--format",
                "json",
            ],
            "write --dry-run",
        ),
    ];

    for (args, desc) in cases {
        let output = run_mc(args);
        assert!(
            output.status.success(),
            "{desc}: non-zero exit. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json = parse_json(&output.stdout);
        assert_eq!(
            json.get("schema_version").and_then(|v| v.as_str()),
            Some("1.0"),
            "{desc}: missing or wrong schema_version in JSON envelope"
        );
    }
}

/// CRIT-3 part 1: I/O failures (file not found) return exit code 3, not 1.
#[test]
fn test_query_returns_exit_3_when_model_file_missing() {
    let output = run_mc(&[
        "model",
        "query",
        "/this/path/should/not/exist/zzz.yaml",
        "--coord",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--format",
        "json",
    ]);
    let code = output.status.code();
    assert_eq!(
        code,
        Some(3),
        "expected exit 3 for missing-file (got {code:?}). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// CRIT-3 part 2: parse / validate failures return exit code 1, not 3.
#[test]
fn test_query_returns_exit_1_when_model_invalid() {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("mc-cli-invalid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tempdir");
    let path = dir.join("invalid.yaml");
    let mut f = std::fs::File::create(&path).expect("create file");
    // Definitely-bad YAML: not even a map at top level.
    f.write_all(b": : :\n  -- not yaml --\n").expect("write");
    drop(f);

    let output = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--coord",
        "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend",
        "--format",
        "json",
    ]);
    let code = output.status.code();
    assert_eq!(
        code,
        Some(1),
        "expected exit 1 for invalid YAML (got {code:?}). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// MIN-5: MCP `mosaic.model.query` returns the JSON envelope *both* in
/// the captured stdout text AND as a `structured` field on the
/// outcome — the same shape the legacy `validate` / `inspect` / `lint`
/// tools use. Without this, agents calling the Phase 6A MCP tools
/// would have to double-parse to get a structured response.
#[test]
fn test_mcp_query_returns_structured_envelope() {
    let path = acme_yaml().to_string_lossy().into_owned();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"mosaic.model.query","arguments":{{"path":"{path}","coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend","format":"json"}}}}}}"#
    );

    let mut child = Command::new(mc_binary())
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mc mcp");

    use std::io::Write;
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(req.as_bytes()).expect("write");
    stdin.write_all(b"\n").expect("newline");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout_str.lines().collect();
    assert_eq!(lines.len(), 1, "MCP response must be exactly one line");
    let response: serde_json::Value =
        serde_json::from_str(lines[0]).expect("MCP response is not valid JSON");
    let result = response.get("result").expect("missing result");
    let structured = result
        .get("structuredContent")
        .or_else(|| result.get("structured"))
        .expect("missing structured (Phase 6A.1 MIN-5)");
    // structured can be a string (the JSON envelope text) — parse it
    // and confirm it carries schema_version: "1.0".
    let parsed: serde_json::Value = match structured {
        serde_json::Value::String(s) => serde_json::from_str(s)
            .unwrap_or_else(|e| panic!("structured content is not valid JSON: {e}")),
        v => v.clone(),
    };
    assert_eq!(
        parsed.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0"),
        "structured envelope missing schema_version"
    );
}
