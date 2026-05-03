//! Phase 4A: smoke test for the `mc mcp` MCP server.
//!
//! Spawns `mc mcp` as a subprocess; pipes JSON-RPC requests on stdin;
//! asserts each response parses, has `jsonrpc: "2.0"`, the right `id`,
//! and a structurally-correct `result`. Does not replicate the full
//! MCP conformance suite — just the load-bearing lifecycle:
//!
//! - `initialize` returns server info + capabilities.
//! - `tools/list` returns the 5 expected tool names.
//! - `tools/call` against `mosaic.model.validate` returns the Phase 3B
//!   diagnostic envelope (empty diagnostics list for a clean Acme).
//! - `tools/call` against `mosaic.model.test` returns the goldens
//!   envelope shape.
//!
//! Per Phase 4A handoff: tests/mcp_smoke.rs is the smoke gate. The
//! exhaustive MCP conformance suite is out of scope.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Cargo provides `CARGO_BIN_EXE_<name>` for binaries in the same
/// package as the integration test. This avoids the
/// build-from-within-test dance and the parallel-test cargo lock
/// contention that comes with it.
fn mc_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mc"))
}

fn acme_yaml() -> PathBuf {
    let mut p = workspace_root();
    p.push("crates");
    p.push("mc-model");
    p.push("examples");
    p.push("acme.yaml");
    p
}

/// Send `requests` (one per line) to `mc mcp`'s stdin; collect stdout
/// lines; return them. Takes ownership of stdin so dropping it sends
/// EOF; without that, `mc mcp` blocks waiting for more input.
fn round_trip(requests: &[&str]) -> Vec<String> {
    let mut child = Command::new(mc_binary())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mc mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    for r in requests {
        stdin.write_all(r.as_bytes()).expect("write request");
        stdin.write_all(b"\n").expect("write newline");
    }
    drop(stdin); // close the pipe → server sees EOF and exits.
    let mut stdout = String::new();
    child
        .stdout
        .as_mut()
        .expect("stdout")
        .read_to_string(&mut stdout)
        .expect("read stdout");
    let _ = child.wait();
    stdout.lines().map(|l| l.to_string()).collect()
}

#[test]
fn mcp_initialize_returns_server_info() {
    let lines = round_trip(&[r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#]);
    assert_eq!(lines.len(), 1, "expected exactly one response line");
    let body = &lines[0];
    assert!(body.starts_with('{'), "response must be a JSON object");
    assert!(body.contains(r#""jsonrpc":"2.0""#));
    assert!(body.contains(r#""id":1"#));
    assert!(body.contains(r#""protocolVersion":"2025-03-26""#));
    assert!(body.contains(r#""serverInfo":{"name":"mosaic","version":"0.1.0"}"#));
    assert!(body.contains(r#""capabilities":{"tools":{}}"#));
}

#[test]
fn mcp_tools_list_returns_five_tools() {
    let lines = round_trip(&[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(body.contains(r#""name":"mosaic.demo""#));
    assert!(body.contains(r#""name":"mosaic.model.validate""#));
    assert!(body.contains(r#""name":"mosaic.model.inspect""#));
    assert!(body.contains(r#""name":"mosaic.model.lint""#));
    assert!(body.contains(r#""name":"mosaic.model.test""#));
}

#[test]
fn mcp_validate_clean_acme() {
    let path = acme_yaml().to_string_lossy().into_owned();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"mosaic.model.validate","arguments":{{"path":"{path}"}}}}}}"#
    );
    let lines = round_trip(&[&req]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(
        body.contains(r#""isError":false"#),
        "validate should succeed: {body}"
    );
    assert!(body.contains(r#""exit_code":0"#));
    assert!(
        body.contains(r#"\"schema_version\": \"1.0\""#),
        "envelope must carry schema_version 1.0: {body}"
    );
    assert!(
        body.contains(r#"\"diagnostics\": []"#),
        "Acme must validate clean: {body}"
    );
}

#[test]
fn mcp_lint_clean_acme() {
    let path = acme_yaml().to_string_lossy().into_owned();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"mosaic.model.lint","arguments":{{"path":"{path}"}}}}}}"#
    );
    let lines = round_trip(&[&req]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(body.contains(r#""isError":false"#));
    assert!(body.contains(r#"\"diagnostics\": []"#));
}

#[test]
fn mcp_test_runs_goldens() {
    let path = acme_yaml().to_string_lossy().into_owned();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"mosaic.model.test","arguments":{{"path":"{path}"}}}}}}"#
    );
    let lines = round_trip(&[&req]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(
        body.contains(r#""isError":false"#),
        "test should pass: {body}"
    );
    assert!(body.contains(r#""exit_code":0"#));
    assert!(body.contains(r#"\"goldens\""#));
    // 9 Acme goldens, all Pass. The response carries the goldens
    // envelope twice (once in content[0].text, once in `structured`),
    // so we expect 18 = 9 × 2 occurrences and zero Fail/Error.
    let pass_count = body.matches(r#"\"status\": \"Pass\""#).count();
    assert_eq!(
        pass_count, 18,
        "all 9 Acme goldens must pass twice over (content + structured): {body}"
    );
    assert!(
        !body.contains(r#"\"status\": \"Fail\""#),
        "no golden may fail: {body}"
    );
    assert!(
        !body.contains(r#"\"status\": \"Error\""#),
        "no golden may error: {body}"
    );
}

#[test]
fn mcp_unknown_tool_returns_error() {
    let req = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"mosaic.does.not.exist","arguments":{}}}"#;
    let lines = round_trip(&[req]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(body.contains(r#""error":{"code":-32601"#));
    assert!(body.contains("unknown tool"));
}

#[test]
fn mcp_malformed_request_returns_parse_error() {
    let lines = round_trip(&[r#"{ this is not json"#]);
    assert_eq!(lines.len(), 1);
    let body = &lines[0];
    assert!(
        body.contains(r#""error":{"code":-32700"#),
        "parse error code: {body}"
    );
}

#[test]
fn mcp_notification_produces_no_response() {
    // notifications/initialized is a notification (no `id` field). The
    // server must NOT respond to it. We follow it with a real request
    // to confirm the server is still alive.
    let lines = round_trip(&[
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#,
    ]);
    assert_eq!(lines.len(), 1, "notification must not produce a response");
    assert!(lines[0].contains(r#""id":7"#));
    assert!(lines[0].contains(r#""result":{}"#));
}
