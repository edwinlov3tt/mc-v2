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
    // Phase 6A.2 item 1.3: trace's envelope bumped to schema_version
    // "1.1" (the only Phase 6A verb to bump in 6A.2; the rest stay
    // at "1.0"). The shape change moved `inputs` from object to array
    // and replaced `measure` with the canonical `coord` string.
    assert_eq!(
        json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.1"),
        "trace must emit schema_version 1.1 (Phase 6A.2 item 1.3)"
    );
    let tree = json.get("trace").expect("missing 'trace' field");
    assert!(tree.get("coord").is_some(), "missing 'coord' field");
    assert!(tree.get("value").is_some(), "missing 'value' field");
    assert!(tree.get("source").is_some(), "missing 'source' field");
    assert!(
        tree.get("child_count").is_some(),
        "missing 'child_count' field"
    );
    // Clicks is derived, so inputs must be a non-empty array.
    let inputs = tree
        .get("inputs")
        .and_then(|i| i.as_array())
        .expect("inputs must be an array");
    assert!(!inputs.is_empty(), "derived cell trace should have inputs");
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
        // Phase 6A.2 item 1.3: trace's envelope bumped to "1.1"; the
        // other 6A verbs stay at "1.0".
        let expected = if *desc == "trace" { "1.1" } else { "1.0" };
        assert_eq!(
            json.get("schema_version").and_then(|v| v.as_str()),
            Some(expected),
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

// ===========================================================================
// Phase 6A.2 item 1.1 — write-log replay (process-notes Rule 9)
// ===========================================================================

/// RAII guard around a `mc-cli`-owned working directory. Removes the
/// dir on drop so writes.jsonl does not leak between tests.
struct WorkDir {
    path: PathBuf,
}
impl WorkDir {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Make a temp dir containing a fresh copy of `acme.yaml` + `acme.inputs.csv`.
/// Avoids the `tempfile` dep (would violate Phase 6A.2 hard rule #4) by
/// composing process id + tag for uniqueness.
fn make_acme_workdir(tag: &str) -> (WorkDir, PathBuf) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mc-cli-{tag}-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create work dir");
    let mut src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    src_dir.pop();
    src_dir.pop();
    src_dir.push("crates");
    src_dir.push("mc-model");
    src_dir.push("examples");
    let yaml_dst = dir.join("acme.yaml");
    let csv_dst = dir.join("acme.inputs.csv");
    std::fs::copy(src_dir.join("acme.yaml"), &yaml_dst).expect("copy yaml");
    std::fs::copy(src_dir.join("acme.inputs.csv"), csv_dst).expect("copy csv");
    (WorkDir { path: dir }, yaml_dst)
}

/// Append a raw line to `<dir>/.tessera/writes.jsonl`, creating the dir if needed.
fn append_writes_jsonl(dir: &std::path::Path, line: &str) {
    let tessera = dir.join(".tessera");
    std::fs::create_dir_all(&tessera).expect("mkdir .tessera");
    let path = tessera.join("writes.jsonl");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open writes.jsonl");
    f.write_all(line.as_bytes()).expect("write");
    if !line.ends_with('\n') {
        f.write_all(b"\n").expect("nl");
    }
}

fn query_value(yaml: &std::path::Path, coord: &str) -> serde_json::Value {
    let output = run_mc(&[
        "model",
        "query",
        yaml.to_str().unwrap(),
        "--coord",
        coord,
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "query failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_json(&output.stdout)
}

const COORD_SPEND_JAN_TAMPA: &str =
    "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend";

#[test]
fn test_query_reflects_post_hoc_write() {
    let (dir, yaml) = make_acme_workdir("query-reflects-write");
    // Pre-write: Acme Spend@Jan/Paid_Search/Tampa = 10500
    let before = query_value(&yaml, COORD_SPEND_JAN_TAMPA);
    assert_eq!(before.get("value").and_then(|v| v.as_f64()), Some(10500.0));
    // Write 999
    let w = run_mc(&[
        "model",
        "write",
        yaml.to_str().unwrap(),
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--value",
        "999",
        "--format",
        "json",
    ]);
    assert!(w.status.success(), "write failed");
    // Re-query (fresh process load) — must see the post-hoc value.
    let after = query_value(&yaml, COORD_SPEND_JAN_TAMPA);
    assert_eq!(
        after.get("value").and_then(|v| v.as_f64()),
        Some(999.0),
        "post-hoc write must be visible to subsequent query (item 1.1 P0)"
    );
    drop(dir);
}

#[test]
fn test_test_ignores_post_hoc_writes() {
    let (dir, yaml) = make_acme_workdir("test-ignores-writes");
    // Write a value that would FAIL the spend_input_anchor golden if it
    // were replayed: golden expects 11500 at Mar_2026/Paid_Search/Tampa.
    let coord_mar = "Scenario=Baseline,Version=Working,Time=Mar_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend";
    let w = run_mc(&[
        "model",
        "write",
        yaml.to_str().unwrap(),
        "--coord",
        coord_mar,
        "--value",
        "99999",
        "--format",
        "json",
    ]);
    assert!(w.status.success(), "write failed");
    // Confirm the write is in the log.
    let log = dir.path().join(".tessera").join("writes.jsonl");
    assert!(log.exists(), "writes.jsonl must exist");
    // `mc model test` must IGNORE the write (Reproducible policy) and
    // continue to pass all 9 goldens against the canonical CSV.
    let t = run_mc(&["model", "test", yaml.to_str().unwrap(), "--format", "json"]);
    assert!(
        t.status.success(),
        "model test must remain green: stderr={}",
        String::from_utf8_lossy(&t.stderr)
    );
    let json = parse_json(&t.stdout);
    let goldens = json
        .get("goldens")
        .and_then(|g| g.as_array())
        .expect("goldens array");
    for g in goldens {
        let status = g.get("status").and_then(|s| s.as_str()).unwrap_or("");
        assert_eq!(
            status, "Pass",
            "golden {:?} regressed because writes.jsonl leaked into mc model test (item 1.1 Reproducible policy)",
            g.get("name")
        );
    }
    // Also confirm the post-hoc write IS visible via query (CurrentReality policy).
    let q = query_value(&yaml, coord_mar);
    assert_eq!(q.get("value").and_then(|v| v.as_f64()), Some(99999.0));
    drop(dir);
}

#[test]
fn test_write_log_corrupt_returns_exit_3() {
    let (dir, yaml) = make_acme_workdir("write-log-corrupt");
    append_writes_jsonl(dir.path(), "this is not json {{{");
    let q = run_mc(&[
        "model",
        "query",
        yaml.to_str().unwrap(),
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert_eq!(
        q.status.code(),
        Some(3),
        "corrupt writes.jsonl must return exit 3 (handoff matrix W3); stderr={}",
        String::from_utf8_lossy(&q.stderr)
    );
    drop(dir);
}

#[test]
fn test_write_log_empty_file_silent_noop() {
    let (dir, yaml) = make_acme_workdir("write-log-empty");
    let tessera = dir.path().join(".tessera");
    std::fs::create_dir_all(&tessera).expect("mkdir");
    std::fs::write(tessera.join("writes.jsonl"), b"").expect("touch empty");
    let q = run_mc(&[
        "model",
        "query",
        yaml.to_str().unwrap(),
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert!(
        q.status.success(),
        "empty writes.jsonl must be silent no-op"
    );
    let json = parse_json(&q.stdout);
    assert_eq!(json.get("value").and_then(|v| v.as_f64()), Some(10500.0));
    drop(dir);
}

#[test]
fn test_write_log_two_writes_same_coord_last_wins() {
    let (dir, yaml) = make_acme_workdir("write-log-last-wins");
    let line1 = format!(
        r#"{{"timestamp":"2026-05-06T00:00:00Z","coord":"{}","value":111,"source":"test"}}"#,
        COORD_SPEND_JAN_TAMPA
    );
    let line2 = format!(
        r#"{{"timestamp":"2026-05-06T00:00:01Z","coord":"{}","value":222,"source":"test"}}"#,
        COORD_SPEND_JAN_TAMPA
    );
    append_writes_jsonl(dir.path(), &line1);
    append_writes_jsonl(dir.path(), &line2);
    let json = query_value(&yaml, COORD_SPEND_JAN_TAMPA);
    assert_eq!(
        json.get("value").and_then(|v| v.as_f64()),
        Some(222.0),
        "second write must win on replay (last-write-wins, handoff matrix W5)"
    );
    drop(dir);
}

#[test]
fn test_write_log_stale_element_returns_exit_3() {
    let (dir, yaml) = make_acme_workdir("write-log-stale-element");
    // Channel "NonExistentChannel" is not in the YAML.
    let stale = r#"{"timestamp":"2026-05-06T00:00:00Z","coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=NonExistentChannel,Market=Tampa,Measure=Spend","value":1,"source":"test"}"#;
    append_writes_jsonl(dir.path(), stale);
    let q = run_mc(&[
        "model",
        "query",
        yaml.to_str().unwrap(),
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert_eq!(
        q.status.code(),
        Some(3),
        "stale-element writes.jsonl must return exit 3 (handoff matrix W1); stderr={}",
        String::from_utf8_lossy(&q.stderr)
    );
    let stderr = String::from_utf8_lossy(&q.stderr);
    assert!(
        stderr.contains("NonExistentChannel") || stderr.contains("element"),
        "stale-element error should mention the missing element; got: {stderr}"
    );
    drop(dir);
}

#[test]
fn test_write_log_to_derived_measure_returns_exit_3() {
    let (dir, yaml) = make_acme_workdir("write-log-derived");
    // Clicks is a derived measure (Spend / CPC). The kernel rejects
    // direct writes to derived cells, but a writes.jsonl line can
    // refer to one (e.g., user-edited the model after the write).
    let derived = r#"{"timestamp":"2026-05-06T00:00:00Z","coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Clicks","value":1,"source":"test"}"#;
    append_writes_jsonl(dir.path(), derived);
    let q = run_mc(&[
        "model",
        "query",
        yaml.to_str().unwrap(),
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert_eq!(
        q.status.code(),
        Some(3),
        "derived-measure writes.jsonl line must return exit 3 (handoff matrix W2); stderr={}",
        String::from_utf8_lossy(&q.stderr)
    );
    drop(dir);
}

// ===========================================================================
// Phase 6A.2 items 1.2 + 1.3 — trace formula + array-shape inputs
// ===========================================================================

const COORD_CLICKS_JAN_TAMPA: &str = "Scenario=Baseline,Version=Working,Time=Jan_2026,\
    Channel=Paid_Search,Market=Tampa,Measure=Clicks";
const COORD_REVENUE_JAN_TAMPA: &str = "Scenario=Baseline,Version=Working,Time=Jan_2026,\
    Channel=Paid_Search,Market=Tampa,Measure=Revenue";
const COORD_SPEND_Q1_PAID_FL: &str = "Scenario=Baseline,Version=Working,Time=Q1_2026,\
    Channel=Paid_Media,Market=Florida,Measure=Spend";

fn trace_json(coord: &str) -> serde_json::Value {
    let path = acme_yaml();
    let out = run_mc(&[
        "model",
        "trace",
        path.to_str().unwrap(),
        "--coord",
        coord,
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "trace failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse_json(&out.stdout)
}

#[test]
fn test_trace_emits_authored_formula_for_derived_cell() {
    // Clicks = Spend / CPC per Acme rule.
    let json = trace_json(COORD_CLICKS_JAN_TAMPA);
    let formula = json
        .get("trace")
        .and_then(|t| t.get("formula"))
        .and_then(|f| f.as_str())
        .expect("derived cell must emit a string formula");
    assert_eq!(
        formula, "Spend / CPC",
        "Phase 6A.2 item 1.2: formula must be authored expression, not debug AST"
    );
    // Revenue = Customers * AOV — second derived cell, double-checks the rule lookup.
    let rev = trace_json(COORD_REVENUE_JAN_TAMPA);
    let rev_formula = rev
        .get("trace")
        .and_then(|t| t.get("formula"))
        .and_then(|f| f.as_str())
        .expect("Revenue must emit a string formula");
    assert_eq!(rev_formula, "Customers * AOV");
}

#[test]
fn test_trace_consolidated_coord_has_null_formula() {
    let json = trace_json(COORD_SPEND_Q1_PAID_FL);
    let trace = json.get("trace").expect("trace");
    assert_eq!(
        trace.get("source").and_then(|v| v.as_str()),
        Some("consolidation")
    );
    let formula = trace.get("formula").expect("formula key always present");
    assert!(
        formula.is_null(),
        "consolidated coord must emit JSON null formula (Phase 6A.2 item 1.2 W2); got {formula:?}"
    );
}

#[test]
fn test_trace_input_cell_has_null_formula() {
    // Spend@Jan/Paid_Search/Tampa is an input cell.
    let json = trace_json(COORD_SPEND_JAN_TAMPA);
    let trace = json.get("trace").expect("trace");
    assert_eq!(trace.get("source").and_then(|v| v.as_str()), Some("input"));
    let formula = trace.get("formula").expect("formula key always present");
    assert!(
        formula.is_null(),
        "input cell must emit JSON null formula (Phase 6A.2 item 1.2 W3); got {formula:?}"
    );
    assert_eq!(
        trace.get("child_count").and_then(|v| v.as_u64()),
        Some(0),
        "input cell child_count must be 0"
    );
}

#[test]
fn test_trace_consolidated_emits_array_with_all_children() {
    // Q1_2026 / Paid_Media / Florida — Time has 3 leaves under Q1 (Jan,
    // Feb, Mar), Channel has 3 leaves under Paid_Media, Market has 3
    // leaves under Florida. So 27 leaf children total.
    let json = trace_json(COORD_SPEND_Q1_PAID_FL);
    let trace = json.get("trace").expect("trace");
    let inputs = trace
        .get("inputs")
        .and_then(|i| i.as_array())
        .expect("inputs must be a JSON array (Phase 6A.2 item 1.3)");
    assert_eq!(
        inputs.len(),
        27,
        "consolidated Spend must expand to 27 leaf children"
    );
    let child_count = trace
        .get("child_count")
        .and_then(|v| v.as_u64())
        .expect("child_count present");
    assert_eq!(
        child_count as usize,
        inputs.len(),
        "child_count must equal inputs.len() (handoff matrix W6)"
    );
    // No two children share a coord — the old object-keyed shape silently
    // dropped duplicates; the array shape must preserve all 27.
    let coords: std::collections::HashSet<&str> = inputs
        .iter()
        .filter_map(|c| c.get("coord").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(
        coords.len(),
        27,
        "all 27 child coords must be distinct (no duplicate-key dedup)"
    );
}

#[test]
fn test_trace_envelope_schema_version_is_1_1() {
    let json = trace_json(COORD_CLICKS_JAN_TAMPA);
    assert_eq!(
        json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.1"),
        "Phase 6A.2 item 1.3: trace bumps schema_version to 1.1"
    );
}

#[test]
fn test_trace_input_cell_has_empty_inputs_array_and_zero_child_count() {
    let json = trace_json(COORD_SPEND_JAN_TAMPA);
    let trace = json.get("trace").expect("trace");
    let inputs = trace
        .get("inputs")
        .and_then(|i| i.as_array())
        .expect("inputs must be an array (always)");
    assert!(
        inputs.is_empty(),
        "input cell must emit empty inputs array (Phase 6A.2 item 1.3)"
    );
    assert_eq!(trace.get("child_count").and_then(|v| v.as_u64()), Some(0));
}

// ===========================================================================
// Phase 6A.2 item 1.4 — MCP numeric params + parsed structured
// ===========================================================================

/// Send one JSON-RPC `tools/call` request to `mc mcp` and return the
/// parsed response (single-line, JSON-RPC 2.0).
fn mcp_call(name: &str, args_json: &str) -> serde_json::Value {
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{args_json}}}}}"#
    );
    let mut child = Command::new(mc_binary())
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mc mcp");
    use std::io::Write;
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(req.as_bytes()).expect("write");
        stdin.write_all(b"\n").expect("nl");
    }
    let output = child.wait_with_output().expect("wait");
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let line = stdout_str.lines().next().expect("at least one line");
    serde_json::from_str(line).expect("MCP response valid JSON")
}

fn acme_path_str() -> String {
    acme_yaml().to_string_lossy().into_owned()
}

#[test]
fn test_mcp_whatif_accepts_json_number_value() {
    let p = acme_path_str();
    let resp = mcp_call(
        "mosaic.model.whatif",
        &format!(
            r#"{{"path":"{p}","set_coord":"{COORD_SPEND_JAN_TAMPA}","value":999,"show":"Revenue"}}"#
        ),
    );
    let result = resp.get("result").expect("result");
    assert_eq!(
        result.get("isError").and_then(|v| v.as_bool()),
        Some(false),
        "JSON number value must be accepted (Phase 6A.2 item 1.4); response: {resp}"
    );
}

#[test]
fn test_mcp_whatif_still_accepts_string_value() {
    // Compat path: clients that send strings (because the legacy schema
    // advertised them) keep working via the coercing accessor.
    let p = acme_path_str();
    let resp = mcp_call(
        "mosaic.model.whatif",
        &format!(
            r#"{{"path":"{p}","set_coord":"{COORD_SPEND_JAN_TAMPA}","value":"888","show":"Revenue"}}"#
        ),
    );
    let result = resp.get("result").expect("result");
    assert_eq!(
        result.get("isError").and_then(|v| v.as_bool()),
        Some(false),
        "string value must still be accepted (Phase 6A.2 item 1.4 W1 backward compat)"
    );
}

#[test]
fn test_mcp_query_accepts_integer_limit() {
    let p = acme_path_str();
    let resp = mcp_call(
        "mosaic.model.query",
        &format!(r#"{{"path":"{p}","coord":"{COORD_SPEND_JAN_TAMPA}","limit":1,"format":"json"}}"#),
    );
    let result = resp.get("result").expect("result");
    assert_eq!(
        result.get("isError").and_then(|v| v.as_bool()),
        Some(false),
        "JSON integer limit must be accepted"
    );
}

#[test]
fn test_mcp_query_returns_parsed_structured_json_object() {
    let p = acme_path_str();
    let resp = mcp_call(
        "mosaic.model.query",
        &format!(r#"{{"path":"{p}","coord":"{COORD_SPEND_JAN_TAMPA}","format":"json"}}"#),
    );
    let result = resp.get("result").expect("result");
    let structured = result.get("structured").expect("structured");
    // Phase 6A.2 item 1.4 W3: structured is a parsed JSON object, NOT
    // a JSON-encoded string. Agents read it directly.
    assert!(
        structured.is_object(),
        "structured must be a JSON object (not a string); got: {structured:?}"
    );
    assert_eq!(
        structured.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0"),
        "structured object must carry schema_version"
    );
    assert_eq!(
        structured.get("value").and_then(|v| v.as_f64()),
        Some(10500.0)
    );
}

// ===========================================================================
// Phase 6A.2 item 1.7 — query pagination + --offset
// ===========================================================================

#[test]
fn test_query_with_low_limit_reports_truncated_true() {
    let path = acme_yaml();
    let out = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--where",
        "Spend > 0",
        "--limit",
        "3",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json = parse_json(&out.stdout);
    assert_eq!(json.get("limit").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(json.get("count").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(
        json.get("truncated").and_then(|v| v.as_bool()),
        Some(true),
        "low-limit query must set truncated=true (Phase 6A.2 item 1.7)"
    );
    assert_eq!(
        json.get("next_offset").and_then(|v| v.as_u64()),
        Some(3),
        "next_offset = offset + count when truncated"
    );
}

#[test]
fn test_query_with_offset_skips_first_n_matches() {
    let path = acme_yaml();
    let p = path.to_str().unwrap();
    let first = run_mc(&[
        "model",
        "query",
        p,
        "--where",
        "Spend > 0",
        "--limit",
        "5",
        "--format",
        "json",
    ]);
    let second = run_mc(&[
        "model",
        "query",
        p,
        "--where",
        "Spend > 0",
        "--limit",
        "5",
        "--offset",
        "5",
        "--format",
        "json",
    ]);
    assert!(first.status.success() && second.status.success());
    let j1 = parse_json(&first.stdout);
    let j2 = parse_json(&second.stdout);
    let r1 = j1.get("results").and_then(|v| v.as_array()).unwrap();
    let r2 = j2.get("results").and_then(|v| v.as_array()).unwrap();
    assert_eq!(r1.len(), 5);
    assert_eq!(r2.len(), 5);
    // The offset=5 page must NOT contain any of the offset=0 page rows.
    let coords_first: std::collections::HashSet<String> = r1
        .iter()
        .filter_map(|r| {
            serde_json::to_string(r.get("coord").unwrap_or(&serde_json::Value::Null)).ok()
        })
        .collect();
    for row in r2 {
        let key = serde_json::to_string(row.get("coord").unwrap_or(&serde_json::Value::Null)).ok();
        if let Some(k) = key {
            assert!(
                !coords_first.contains(&k),
                "offset=5 page leaked offset=0 row: {k}"
            );
        }
    }
}

#[test]
fn test_query_offset_beyond_matches_returns_empty() {
    let path = acme_yaml();
    let out = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--where",
        "Spend > 100000000", // no rows
        "--offset",
        "10",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json = parse_json(&out.stdout);
    assert_eq!(json.get("count").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(
        json.get("truncated").and_then(|v| v.as_bool()),
        Some(false),
        "offset beyond matches must NOT set truncated=true"
    );
    assert!(
        json.get("next_offset")
            .map(|v| v.is_null())
            .unwrap_or(false),
        "next_offset must be JSON null when not truncated"
    );
}

#[test]
fn test_query_envelope_includes_all_pagination_fields() {
    let path = acme_yaml();
    let out = run_mc(&[
        "model",
        "query",
        path.to_str().unwrap(),
        "--where",
        "Spend > 0",
        "--limit",
        "100",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json = parse_json(&out.stdout);
    for field in &["limit", "offset", "count", "truncated", "next_offset"] {
        assert!(
            json.get(field).is_some(),
            "envelope must always carry '{field}' (Phase 6A.2 item 1.7 stable schema)"
        );
    }
    // Schema_version stays at 1.0 — pagination is additive.
    assert_eq!(
        json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0"),
        "query envelope schema_version must stay at 1.0 (additive change)"
    );
}

// ===========================================================================
// Phase 6A.2 item 1.5 — tessera transform consumes real mc-recipe schema
// ===========================================================================

fn workspace_path(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push(rel);
    p
}

#[test]
fn test_transform_with_acme_recipe_emits_mapped_rows() {
    let source = workspace_path("crates/mc-model/examples/acme.inputs.csv");
    // Use the long-format recipe whose `columns:` source headers match
    // the actual `acme.inputs.csv` column names. The Phase 6A bespoke
    // parser produced ONLY defaults from this input; the real
    // `mc_recipe::Recipe` parse pulls Time / Channel / Market through.
    let recipe = workspace_path("crates/mc-recipe/examples/recipes/acme-long-format.recipe.yaml");
    let out = run_mc(&[
        "tessera",
        "transform",
        "--source",
        source.to_str().unwrap(),
        "--recipe",
        recipe.to_str().unwrap(),
        "--preview",
        "1",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "transform failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_json(&out.stdout);
    let rows = json
        .get("rows")
        .and_then(|r| r.as_array())
        .expect("rows array");
    let row = rows.first().expect("at least one preview row");
    // Time, Channel, Market all came from real `mc-recipe` column
    // mappings; Phase 6A's bespoke parser would have left these absent.
    assert_eq!(row.get("Time").and_then(|v| v.as_str()), Some("Jan_2026"));
    assert_eq!(
        row.get("Channel").and_then(|v| v.as_str()),
        Some("Paid_Search")
    );
    assert_eq!(row.get("Market").and_then(|v| v.as_str()), Some("Tampa"));
}

#[test]
fn test_transform_json_envelope_has_schema_version() {
    let source = workspace_path("crates/mc-model/examples/acme.inputs.csv");
    let recipe = workspace_path("crates/mc-recipe/examples/recipes/acme-long-format.recipe.yaml");
    let out = run_mc(&[
        "tessera",
        "transform",
        "--source",
        source.to_str().unwrap(),
        "--recipe",
        recipe.to_str().unwrap(),
        "--preview",
        "1",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let json = parse_json(&out.stdout);
    assert_eq!(
        json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0"),
        "transform JSON must wrap output in a schema_version envelope (Phase 6A.2 item 1.5 / Codex COD-2)"
    );
    assert!(
        json.get("rows").is_some(),
        "envelope must have 'rows' field"
    );
    assert!(
        json.get("count").is_some(),
        "envelope must have 'count' field"
    );
}

#[test]
fn test_transform_recipe_parse_error_returns_exit_1() {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("mc-cli-bad-recipe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let recipe = dir.join("bad.yaml");
    let mut f = std::fs::File::create(&recipe).expect("create");
    // Definitely-bad recipe: wrong shape entirely.
    f.write_all(b"this: is\nnot: a\nrecipe: schema\n")
        .expect("write");
    drop(f);
    let source = workspace_path("crates/mc-model/examples/acme.inputs.csv");
    let out = run_mc(&[
        "tessera",
        "transform",
        "--source",
        source.to_str().unwrap(),
        "--recipe",
        recipe.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "malformed recipe must return exit 1 (model/recipe class); stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_mcp_trace_accepts_integer_depth() {
    let p = acme_path_str();
    let resp = mcp_call(
        "mosaic.model.trace",
        &format!(r#"{{"path":"{p}","coord":"{COORD_CLICKS_JAN_TAMPA}","depth":2}}"#),
    );
    let result = resp.get("result").expect("result");
    assert_eq!(
        result.get("isError").and_then(|v| v.as_bool()),
        Some(false),
        "JSON integer depth must be accepted"
    );
    let structured = result.get("structured").expect("structured");
    assert!(
        structured.is_object(),
        "trace structured must be a parsed JSON object"
    );
    assert_eq!(
        structured.get("schema_version").and_then(|v| v.as_str()),
        Some("1.1"),
        "trace structured carries the bumped schema_version"
    );
}

// ===========================================================================
// Phase 6A.3 item 1 — multi-cell whatif (--set repeatable)
// ===========================================================================

const COORD_AOV_JAN_TAMPA: &str =
    "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=AOV";

/// 6A.3 item 1: two `--set` flags apply atomically. Revenue =
/// Customers * AOV, and Customers depends on Spend; setting both Spend
/// and AOV at the same anchor coord must produce a Revenue delta that
/// reflects BOTH overrides (cross-effect).
#[test]
fn test_whatif_multiple_set_flags_apply_atomically() {
    let path = acme_yaml();
    let path_str = path.to_str().unwrap();

    // Baseline run: only override Spend; record the resulting Revenue
    // delta. Uses the new --set "coord=value" form to exercise the
    // repeatable parser path even on the single-override side.
    let only_spend = run_mc(&[
        "model",
        "whatif",
        path_str,
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=20000"),
        "--show",
        "Revenue",
        "--format",
        "json",
    ]);
    assert!(
        only_spend.status.success(),
        "single-override (new form) must succeed: stderr={}",
        String::from_utf8_lossy(&only_spend.stderr)
    );
    let only_spend_json = parse_json(&only_spend.stdout);
    let only_spend_delta = only_spend_json
        .get("affected_measures")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("measure").and_then(|v| v.as_str()) == Some("Revenue"))
        })
        .and_then(|m| m.get("delta").and_then(|v| v.as_f64()))
        .expect("Revenue delta on Spend-only override");

    // Multi-override run: override Spend AND AOV at the same anchor.
    let both = run_mc(&[
        "model",
        "whatif",
        path_str,
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=20000"),
        "--set",
        &format!("{COORD_AOV_JAN_TAMPA}=500"),
        "--show",
        "Revenue",
        "--format",
        "json",
    ]);
    assert!(
        both.status.success(),
        "multi-override must succeed: stderr={}",
        String::from_utf8_lossy(&both.stderr)
    );
    let both_json = parse_json(&both.stdout);
    let overrides = both_json
        .get("overrides")
        .and_then(|o| o.as_array())
        .expect("overrides[] must be an array");
    assert_eq!(
        overrides.len(),
        2,
        "two --set flags must produce two overrides[] entries"
    );
    let both_delta = both_json
        .get("affected_measures")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("measure").and_then(|v| v.as_str()) == Some("Revenue"))
        })
        .and_then(|m| m.get("delta").and_then(|v| v.as_f64()))
        .expect("Revenue delta on multi-override");

    // The two-override Revenue delta MUST differ from the Spend-only
    // delta — that is the entire point of multi-cell whatif. AOV is a
    // direct factor in Revenue, so changing both Spend and AOV is
    // strictly more impactful than changing Spend alone.
    assert!(
        (both_delta - only_spend_delta).abs() > 1e-6,
        "multi-override Revenue delta ({both_delta}) must differ from Spend-only delta ({only_spend_delta})"
    );
}

/// 6A.3 item 1 W1: when the same coordinate appears in two `--set`
/// flags, the LAST value wins. Document-as-spec; matches whatif
/// semantics ("apply all in order, then read"). No error is raised.
#[test]
fn test_whatif_same_coord_set_twice_last_wins() {
    let path = acme_yaml();
    let output = run_mc(&[
        "model",
        "whatif",
        path.to_str().unwrap(),
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=12345"),
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=20000"),
        "--show",
        "Spend,Revenue",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "duplicate-coord overrides must NOT error: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output.stdout);

    // The last override wins on the cube, so Spend@anchor reads back as 20000.
    let after_spend = json
        .get("affected_measures")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("measure").and_then(|v| v.as_str()) == Some("Spend"))
        })
        .and_then(|m| m.get("after").and_then(|v| v.as_f64()))
        .expect("Spend after value");
    assert!(
        (after_spend - 20000.0).abs() < 1e-9,
        "last-write-wins: Spend after must be 20000, got {after_spend}"
    );
}

/// 6A.3 item 1 W3: if any override fails (e.g., target is a derived
/// measure that the kernel rejects), every override is rolled back and
/// exit code is 1. The agent never observes partial state.
#[test]
fn test_whatif_one_override_fails_rolls_back_all() {
    let path = acme_yaml();
    // Clicks is a derived measure (Clicks = Spend / CPC). Writing to it
    // is rejected by the kernel — see Cube::write derived-rejection path.
    let coord_clicks = "Scenario=Baseline,Version=Working,Time=Jan_2026,\
        Channel=Paid_Search,Market=Tampa,Measure=Clicks";
    let output = run_mc(&[
        "model",
        "whatif",
        path.to_str().unwrap(),
        // One valid input override + one derived target → batch must fail.
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=22222"),
        "--set",
        &format!("{coord_clicks}=99999"),
        "--show",
        "Revenue",
        "--format",
        "json",
    ]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "atomic write must exit 1 when any override fails (handoff W3); stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    // No JSON envelope on stderr-only failure path; stdout should be empty.
    assert!(
        output.stdout.is_empty(),
        "failed atomic whatif must not emit a partial deltas envelope on stdout; got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// 6A.3 item 1 W5: `--dry-run` with multi-cell overrides. Reports
/// would-be deltas and never persists. Verified by reading the same
/// coord in a fresh process: it must equal the canonical (pre-dry-run)
/// value, not the dry-run override.
#[test]
fn test_whatif_dry_run_does_not_persist_overrides() {
    let path = acme_yaml();
    let path_str = path.to_str().unwrap();

    // Sanity: read the canonical Spend value first.
    let q1 = run_mc(&[
        "model",
        "query",
        path_str,
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert!(q1.status.success(), "pre-query failed");
    let pre = parse_json(&q1.stdout)
        .get("value")
        .and_then(|v| v.as_f64())
        .expect("pre value");

    // Multi-cell dry-run.
    let dry = run_mc(&[
        "model",
        "whatif",
        path_str,
        "--set",
        &format!("{COORD_SPEND_JAN_TAMPA}=99999"),
        "--set",
        &format!("{COORD_AOV_JAN_TAMPA}=42"),
        "--show",
        "Revenue",
        "--dry-run",
        "--format",
        "json",
    ]);
    assert!(
        dry.status.success(),
        "dry-run multi-cell must succeed: stderr={}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let dry_json = parse_json(&dry.stdout);
    assert_eq!(
        dry_json.get("dry_run").and_then(|v| v.as_bool()),
        Some(true)
    );
    let overrides = dry_json
        .get("overrides")
        .and_then(|o| o.as_array())
        .expect("dry-run overrides[] must be an array");
    assert_eq!(overrides.len(), 2);

    // Re-query the same coord — it must equal the pre value, not 99999.
    let q2 = run_mc(&[
        "model",
        "query",
        path_str,
        "--coord",
        COORD_SPEND_JAN_TAMPA,
        "--format",
        "json",
    ]);
    assert!(q2.status.success(), "post-query failed");
    let post = parse_json(&q2.stdout)
        .get("value")
        .and_then(|v| v.as_f64())
        .expect("post value");
    assert!(
        (post - pre).abs() < 1e-9,
        "dry-run must NOT persist; pre={pre}, post={post}"
    );
}
