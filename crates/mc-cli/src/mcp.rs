//! `mc mcp` — Mosaic MCP server (Phase 4A).
//!
//! Speaks JSON-RPC 2.0 over newline-delimited stdin/stdout per the MCP
//! protocol. Surfaces five tools (`mosaic.demo`, `mosaic.model.validate`,
//! `mosaic.model.inspect`, `mosaic.model.lint`, `mosaic.model.test`)
//! that wrap the existing `mc-cli` verb implementations.
//!
//! Phase 4A constraints (per [ADR-0008](../../../../docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md)
//! amendment H + the Phase 4A handoff scope item 9):
//!
//! - **No new dependencies.** Hand-rolled JSON tokenizer + emitter,
//!   reuses Phase 3B's diagnostic JSON envelope module via `mc-model`.
//! - **Sync only.** No tokio, no async, no threads. One request, one
//!   response, line-delimited.
//! - **Stdio only.** No HTTP, no websockets, no socket framing.
//! - **No `unwrap()` / `expect()` / `panic!()` in this module's hot
//!   path.** Errors flow back as JSON-RPC error responses.
//!
//! The diagnostic envelope produced for `mosaic.model.{validate,lint,
//! test}` uses Phase 3B's existing JSON serializer
//! ([`mc_model::diagnostics_to_json`]) verbatim — no envelope-shape
//! change.

use mc_model::{
    apply_canonical_inputs, apply_fixture, diagnostics_to_json, inspect_json, lint_with_file,
    resolve_inputs, sort_diagnostics, Diagnostic, ModelPath, Severity, ValidatedModel,
    ValidationError, SCHEMA_VERSION,
};
use std::io::{BufRead, Write};

// ============================================================================
// Public entry point — wired into `crates/mc-cli/src/main.rs`.
// ============================================================================

/// Run the MCP server: read JSON-RPC requests on stdin, write responses
/// on stdout, until stdin closes (EOF). Single-threaded, sync; one
/// request handled at a time.
pub fn run() -> ! {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut line = String::new();
    let mut reader = stdin.lock();

    loop {
        line.clear();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = handle_request(trimmed);
        // A notification (no id) returns None; don't write anything.
        if let Some(resp_text) = response {
            // Best-effort write; if stdout is closed, exit.
            if writeln!(stdout, "{resp_text}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    }
    std::process::exit(0);
}

// ============================================================================
// Request dispatch.
// ============================================================================

fn handle_request(raw: &str) -> Option<String> {
    let parsed = match parse_json(raw) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                &JsonValue::Null,
                -32700,
                &format!("parse error: {e}"),
            ))
        }
    };
    if parsed.as_object().is_none() {
        return Some(error_response(
            &JsonValue::Null,
            -32600,
            "request must be a JSON object",
        ));
    }
    // Validate the JSON-RPC 2.0 envelope.
    let jsonrpc = parsed
        .get("jsonrpc")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    if jsonrpc != "2.0" {
        let id = parsed.get("id").cloned().unwrap_or(JsonValue::Null);
        return Some(error_response(&id, -32600, "jsonrpc must be \"2.0\""));
    }
    let method_owned = match parsed.get("method").and_then(JsonValue::as_str_owned) {
        Some(m) => m,
        None => {
            let id = parsed.get("id").cloned().unwrap_or(JsonValue::Null);
            return Some(error_response(&id, -32600, "missing method"));
        }
    };
    let method = method_owned.as_str();
    let id = parsed.get("id").cloned();
    let params = parsed.get("params").cloned().unwrap_or(JsonValue::Null);

    // Notifications (no `id` field) get no response per JSON-RPC.
    let is_notification = id.is_none();
    let id = id.unwrap_or(JsonValue::Null);

    let result = match method {
        "initialize" => Ok(handle_initialize()),
        "notifications/initialized" | "initialized" => return None,
        "tools/list" => Ok(handle_tools_list()),
        "tools/call" => handle_tools_call(&params),
        "ping" => Ok(JsonValue::Object(vec![])),
        _ => Err((-32601, format!("method not found: {method}"))),
    };

    if is_notification {
        return None;
    }

    Some(match result {
        Ok(value) => success_response(&id, value),
        Err((code, msg)) => error_response(&id, code, &msg),
    })
}

fn handle_initialize() -> JsonValue {
    JsonValue::Object(vec![
        (
            "protocolVersion".into(),
            JsonValue::String("2025-03-26".into()),
        ),
        (
            "capabilities".into(),
            JsonValue::Object(vec![("tools".into(), JsonValue::Object(vec![]))]),
        ),
        (
            "serverInfo".into(),
            JsonValue::Object(vec![
                ("name".into(), JsonValue::String("mosaic".into())),
                ("version".into(), JsonValue::String("0.1.0".into())),
            ]),
        ),
    ])
}

fn handle_tools_list() -> JsonValue {
    let tools = JsonValue::Array(vec![
        tool_descriptor(
            "mosaic.demo",
            "Run the Acme demo end-to-end. Optional `model_path` routes through mc_model::load instead of the Rust fixture path.",
            &[("model_path", "string", "Optional path to a Mosaic YAML model. If omitted, runs the canonical Rust fixture demo.", false)],
        ),
        tool_descriptor(
            "mosaic.model.validate",
            "Parse + structural + fixture/CSV validation. Returns the Phase 3B diagnostic JSON envelope.",
            &[("path", "string", "Path to a Mosaic YAML model.", true)],
        ),
        tool_descriptor(
            "mosaic.model.inspect",
            "Render the model summary (dim counts, measures, rules, hierarchies, golden count, canonical inputs row count, diagnostics).",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("format", "string", "Output format: 'text' (default) or 'json'.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.model.lint",
            "Run advisory MC3xxx lint rules. Returns the same diagnostic envelope shape as validate.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("deny_warnings", "boolean", "If true, exit_code is 1 when any warnings fire.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.model.test",
            "Run goldens. Returns the {schema_version, skipped, goldens[]} envelope.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("fixture", "string", "Optional --fixture <name> filter (filter-only semantic).", false),
            ],
        ),
        // Phase 6A: Agent-Ready CLI tools.
        // Phase 6A.2 item 1.4: numeric/integer/bool params advertise their
        // canonical types. The handlers also accept the legacy "string"
        // shape via coercion (handoff matrix W1) for backward compat
        // with clients that grew up against the original Phase 6A schema.
        tool_descriptor(
            "mosaic.model.query",
            "Query cells by coordinate filter. Returns matching rows with values. Supports --where filters, --show columns, --aggregate functions.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("where", "string", "Filter expression (e.g., \"EV_Per_Dollar > 0.03 and Game == 'LAL_at_BOS'\").", false),
                ("show", "string", "Comma-separated measure names to include in output.", false),
                ("coord", "string", "Single coordinate for exact lookup (e.g., \"Scenario=Base,Version=Working,...,Measure=EV_Per_Dollar\").", false),
                ("aggregate", "string", "Comma-separated aggregate functions (e.g., \"mean(Abs_Error),sum(Profit_Units)\").", false),
                ("format", "string", "Output format: 'json' (default), 'csv', or 'text'.", false),
                ("limit", "integer", "Max rows to return (default 10000). JSON numbers preferred; numeric strings also accepted for backward compat.", false),
                ("offset", "integer", "Skip the first N matches before applying limit (default 0). Use with `limit` for forward pagination.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.model.whatif",
            "Override one input cell, report deltas on dependent measures. Read-only operation (auto-rollbacks).",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("set_coord", "string", "Coordinate of the cell to override (e.g., \"Scenario=Base,...,Measure=Market_Line\").", true),
                ("value", "number", "New numeric value to set. JSON numbers preferred; numeric strings also accepted for backward compat.", true),
                ("show", "string", "Comma-separated measure names to report before/after/delta.", true),
            ],
        ),
        tool_descriptor(
            "mosaic.model.trace",
            "Show the hierarchical computation tree for one derived cell. Traces all the way to input values.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("coord", "string", "Coordinate of the cell to trace (e.g., \"Scenario=Base,...,Measure=EV_Per_Dollar\").", true),
                ("depth", "integer", "Max trace depth (default unlimited). JSON integers preferred; integer-valued strings also accepted for backward compat.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.model.sweep",
            "Parameter sensitivity analysis. Loop over a range of values, evaluate a metric at each point, report optimal.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("model", "string", "Fitted model name (for coefficient sweep).", false),
                ("coefficient", "string", "Coefficient feature name to sweep.", false),
                ("set", "string", "Cell coordinate to override (alternative to coefficient sweep).", false),
                ("range", "string", "Range spec: start:end:step (e.g., \"0:5:0.5\").", true),
                ("metric", "string", "Metric function (e.g., \"mean(Abs_Error)\").", true),
                ("goal", "string", "Optimization goal: 'minimize' or 'maximize'.", true),
            ],
        ),
        tool_descriptor(
            "mosaic.model.diff",
            "Compare two cube states. Reports cells where values differ, sorted by |delta|.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("left", "string", "Left state filter (e.g., \"Scenario=Base\").", true),
                ("right", "string", "Right state filter (e.g., \"Scenario=Forecast\").", true),
                ("limit", "integer", "Max changes to report (default 50). JSON integers preferred; numeric strings also accepted.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.model.write",
            "Set one cell value and persist to the .tessera/writes.jsonl sidecar log.",
            &[
                ("path", "string", "Path to a Mosaic YAML model.", true),
                ("coord", "string", "Coordinate of the cell to write.", true),
                ("value", "number", "New numeric value. JSON numbers preferred; numeric strings also accepted for backward compat.", true),
                ("dry_run", "boolean", "If true, show what would change without writing.", false),
            ],
        ),
        tool_descriptor(
            "mosaic.tessera.transform",
            "Convert raw data (file or URL) to model-compatible format using a recipe. Simple GET for URLs.",
            &[
                ("source", "string", "Source file path or URL.", true),
                ("recipe", "string", "Path to the transform recipe YAML.", true),
                ("output", "string", "Output file path. Omit for stdout.", false),
                ("format", "string", "Output format: 'csv' (default) or 'json'.", false),
                ("preview", "integer", "Preview N rows without writing output file. JSON integers preferred; numeric strings also accepted.", false),
            ],
        ),
    ]);
    JsonValue::Object(vec![("tools".into(), tools)])
}

fn tool_descriptor(name: &str, description: &str, args: &[(&str, &str, &str, bool)]) -> JsonValue {
    let mut props = Vec::new();
    let mut required = Vec::new();
    for (arg_name, arg_type, arg_desc, is_required) in args {
        props.push((
            (*arg_name).to_string(),
            JsonValue::Object(vec![
                ("type".into(), JsonValue::String((*arg_type).into())),
                ("description".into(), JsonValue::String((*arg_desc).into())),
            ]),
        ));
        if *is_required {
            required.push(JsonValue::String((*arg_name).into()));
        }
    }
    let input_schema = JsonValue::Object(vec![
        ("type".into(), JsonValue::String("object".into())),
        ("properties".into(), JsonValue::Object(props)),
        ("required".into(), JsonValue::Array(required)),
    ]);
    JsonValue::Object(vec![
        ("name".into(), JsonValue::String(name.into())),
        ("description".into(), JsonValue::String(description.into())),
        ("inputSchema".into(), input_schema),
    ])
}

fn handle_tools_call(params: &JsonValue) -> Result<JsonValue, (i64, String)> {
    if params.as_object().is_none() {
        return Err((-32602, "params must be an object".to_string()));
    }
    let name = params
        .get("name")
        .and_then(JsonValue::as_str_owned)
        .ok_or((-32602, "params.name is required".to_string()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| JsonValue::Object(vec![]));
    let outcome = match name.as_str() {
        "mosaic.demo" => tool_demo(&args),
        "mosaic.model.validate" => tool_validate(&args),
        "mosaic.model.inspect" => tool_inspect(&args),
        "mosaic.model.lint" => tool_lint(&args),
        "mosaic.model.test" => tool_test(&args),
        // Phase 6A tools
        "mosaic.model.query" => tool_query(&args),
        "mosaic.model.whatif" => tool_whatif(&args),
        "mosaic.model.trace" => tool_trace(&args),
        "mosaic.model.sweep" => tool_sweep(&args),
        "mosaic.model.diff" => tool_diff(&args),
        "mosaic.model.write" => tool_write(&args),
        "mosaic.tessera.transform" => tool_transform(&args),
        other => return Err((-32601, format!("unknown tool: {other}"))),
    };
    Ok(outcome.into_call_result())
}

// ============================================================================
// Tool implementations. Each reuses mc-model's existing functions; no
// behavior change vs the CLI verbs. The output is stuffed into a
// ToolOutcome which is rendered as the MCP `content` array + bundles
// the diagnostic envelope (when applicable) as a structured field.
// ============================================================================

struct ToolOutcome {
    exit_code: i32,
    stdout: String,
    /// When the tool produced a diagnostic envelope (validate, inspect,
    /// lint, test), it is stored here verbatim as the JSON string. This
    /// is the load-bearing payload for the LLM iteration loop.
    structured: Option<String>,
}

impl ToolOutcome {
    fn into_call_result(self) -> JsonValue {
        // MCP `tools/call` result shape: { content: [{type:"text", text:"..."}], isError: bool }.
        // We add `exit_code` and `structured` as extra fields the agent
        // can read structurally (the MCP spec allows extra fields on the
        // result object).
        let text = if self.stdout.is_empty() && self.structured.is_some() {
            self.structured.clone().unwrap_or_default()
        } else {
            self.stdout
        };
        let mut content_text = text;
        // Strip a single trailing newline so JSON envelopes stay
        // canonical when re-emitted.
        if content_text.ends_with('\n') {
            content_text.pop();
        }
        let content = JsonValue::Array(vec![JsonValue::Object(vec![
            ("type".into(), JsonValue::String("text".into())),
            ("text".into(), JsonValue::String(content_text.clone())),
        ])]);
        let mut fields = vec![
            ("content".into(), content),
            ("isError".into(), JsonValue::Bool(self.exit_code != 0)),
            ("exit_code".into(), JsonValue::Number(self.exit_code as f64)),
        ];
        if let Some(s) = self.structured {
            // Phase 6A.2 item 1.4 W3: emit `structured` as a parsed
            // JSON value when possible so agents don't have to double-
            // parse. Fall back to wrapping the raw text in a JSON
            // string for non-JSON outputs (e.g. inspect --format text).
            let parsed = parse_json(&s).unwrap_or(JsonValue::String(s));
            fields.push(("structured".into(), parsed));
        }
        JsonValue::Object(fields)
    }
}

fn tool_demo(args: &JsonValue) -> ToolOutcome {
    let model_path = args
        .get_field("model_path")
        .and_then(JsonValue::as_str_owned);
    // Demo emits free-form text; we don't try to capture stdout here
    // because the demo writes directly to the process stdout. Return
    // a structured note instead.
    let _ = model_path; // available for future expansion
    ToolOutcome {
        exit_code: 0,
        stdout: "demo runs from the CLI; in MCP mode call `mc demo` directly via the shell.".into(),
        structured: None,
    }
}

fn tool_validate(args: &JsonValue) -> ToolOutcome {
    let path = match args.get_field("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    match load_validated_quiet(&path) {
        Ok(_) => {
            let envelope = diagnostics_to_json(&[]);
            ToolOutcome {
                exit_code: 0,
                stdout: String::new(),
                structured: Some(envelope),
            }
        }
        Err(diags) => {
            let envelope = diagnostics_to_json(&diags);
            ToolOutcome {
                exit_code: 1,
                stdout: String::new(),
                structured: Some(envelope),
            }
        }
    }
}

fn tool_inspect(args: &JsonValue) -> ToolOutcome {
    let path = match args.get_field("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let format = args
        .get_field("format")
        .and_then(JsonValue::as_str_owned)
        .unwrap_or_else(|| "json".into());
    let model = match load_validated_quiet(&path) {
        Ok(m) => m,
        Err(diags) => {
            return ToolOutcome {
                exit_code: 1,
                stdout: String::new(),
                structured: Some(diagnostics_to_json(&diags)),
            }
        }
    };
    let mut diags = lint_with_file(&model, &path);
    sort_diagnostics(&mut diags);
    let model_dir = std::path::Path::new(&path).parent();
    let inputs = resolve_inputs(&model, model_dir).ok();
    let body = if format == "text" {
        mc_model::inspect_text_with_diagnostics(&model, &diags, inputs.as_ref())
    } else {
        inspect_json(&model, &diags, inputs.as_ref())
    };
    ToolOutcome {
        exit_code: 0,
        stdout: body.clone(),
        structured: Some(body),
    }
}

fn tool_lint(args: &JsonValue) -> ToolOutcome {
    let path = match args.get_field("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let deny = args
        .get_field("deny_warnings")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let model = match load_validated_quiet(&path) {
        Ok(m) => m,
        Err(diags) => {
            return ToolOutcome {
                exit_code: 1,
                stdout: String::new(),
                structured: Some(diagnostics_to_json(&diags)),
            }
        }
    };
    let mut diags = lint_with_file(&model, &path);
    sort_diagnostics(&mut diags);
    let envelope = diagnostics_to_json(&diags);
    let exit_code = if deny && !diags.is_empty() { 1 } else { 0 };
    ToolOutcome {
        exit_code,
        stdout: String::new(),
        structured: Some(envelope),
    }
}

fn tool_test(args: &JsonValue) -> ToolOutcome {
    let path = match args.get_field("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let fixture_filter = args.get_field("fixture").and_then(JsonValue::as_str_owned);
    let model = match load_validated_quiet(&path) {
        Ok(m) => m,
        Err(diags) => {
            return ToolOutcome {
                exit_code: 1,
                stdout: String::new(),
                structured: Some(diagnostics_to_json(&diags)),
            }
        }
    };
    let model_dir = std::path::Path::new(&path).parent();
    let inputs = match resolve_inputs(&model, model_dir) {
        Ok(i) => i,
        Err(errs) => {
            let diags = validation_errors_to_diagnostics(&errs, &path);
            return ToolOutcome {
                exit_code: 1,
                stdout: String::new(),
                structured: Some(diagnostics_to_json(&diags)),
            };
        }
    };
    let compiled = match mc_model::compile(model.clone()) {
        Ok(c) => c,
        Err(e) => {
            return error_outcome(&format!("compile error: {e}"));
        }
    };
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;
    if let Err(e) = apply_canonical_inputs(&mut cube, &compiled.refs, principal, &inputs) {
        return error_outcome(&format!("apply_canonical_inputs failed: {e}"));
    }
    let snap = cube.snapshot(None);
    let mut goldens_envelope = String::new();
    goldens_envelope.push_str("{\n  \"schema_version\": \"");
    goldens_envelope.push_str(SCHEMA_VERSION);
    goldens_envelope.push_str("\",\n  \"skipped\": ");
    let mut any_failed = false;
    let mut skipped = 0usize;
    let mut entries: Vec<String> = Vec::new();
    for golden in &model.parsed.golden_tests {
        if let Some(filter) = &fixture_filter {
            let m = golden
                .fixture
                .as_deref()
                .map(|n| n == filter)
                .unwrap_or(false);
            if !m {
                skipped += 1;
                continue;
            }
        }
        let mut mutated = false;
        if let Some(fname) = &golden.fixture {
            if let Some(fixture) = inputs.fixture(fname) {
                if let Err(e) = apply_fixture(&mut cube, &compiled.refs, principal, fixture) {
                    entries.push(golden_entry(
                        &golden.name,
                        "Error",
                        None,
                        None,
                        None,
                        None,
                        Some(&format!("fixture {fname:?} apply error: {e}")),
                    ));
                    any_failed = true;
                    let _ = cube.rollback_to(&snap);
                    continue;
                }
                mutated = true;
            }
        }
        let entry = run_one(golden, &compiled.refs, principal, &mut cube);
        if entry.failed {
            any_failed = true;
        }
        entries.push(entry.json);
        if mutated {
            let _ = cube.rollback_to(&snap);
        }
    }
    use std::fmt::Write as _;
    let _ = write!(goldens_envelope, "{skipped},\n  \"goldens\": [");
    if entries.is_empty() {
        goldens_envelope.push_str("]\n}\n");
    } else {
        goldens_envelope.push('\n');
        for (i, e) in entries.iter().enumerate() {
            goldens_envelope.push_str("    ");
            goldens_envelope.push_str(e);
            if i + 1 < entries.len() {
                goldens_envelope.push(',');
            }
            goldens_envelope.push('\n');
        }
        goldens_envelope.push_str("  ]\n}\n");
    }
    ToolOutcome {
        exit_code: if any_failed { 1 } else { 0 },
        stdout: String::new(),
        structured: Some(goldens_envelope),
    }
}

struct GoldenEntry {
    json: String,
    failed: bool,
}

// ==========================================================================
// Phase 6A tool implementations. These delegate to the CLI verb modules,
// capturing stdout into a ToolOutcome.
// ==========================================================================

fn tool_query(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let mut cli_args = vec![path];
    if let Some(w) = args.get("where").and_then(JsonValue::as_str_owned) {
        cli_args.push("--where".into());
        cli_args.push(w);
    }
    if let Some(s) = args.get("show").and_then(JsonValue::as_str_owned) {
        cli_args.push("--show".into());
        cli_args.push(s);
    }
    if let Some(c) = args.get("coord").and_then(JsonValue::as_str_owned) {
        cli_args.push("--coord".into());
        cli_args.push(c);
    }
    if let Some(a) = args.get("aggregate").and_then(JsonValue::as_str_owned) {
        cli_args.push("--aggregate".into());
        cli_args.push(a);
    }
    if let Some(l) = args.get("limit").and_then(JsonValue::as_integer_coerced) {
        cli_args.push("--limit".into());
        cli_args.push(l.to_string());
    }
    if let Some(o) = args.get("offset").and_then(JsonValue::as_integer_coerced) {
        cli_args.push("--offset".into());
        cli_args.push(o.to_string());
    }
    cli_args.push("--format".into());
    cli_args.push("json".into());
    run_cli_verb_json(|| {
        let cmd = crate::query::parse(&cli_args)?;
        Ok(crate::query::run_captured(cmd))
    })
}

fn tool_whatif(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let set_coord = match args.get("set_coord").and_then(JsonValue::as_str_owned) {
        Some(s) => s,
        None => return error_outcome("missing required argument: set_coord"),
    };
    let value = match args.get("value").and_then(JsonValue::as_number_coerced) {
        Some(v) => v,
        None => return error_outcome("missing required argument: value"),
    };
    let show = match args.get("show").and_then(JsonValue::as_str_owned) {
        Some(s) => s,
        None => return error_outcome("missing required argument: show"),
    };
    let cli_args = vec![
        path,
        "--set".into(),
        set_coord,
        "--value".into(),
        format_f64_arg(value),
        "--show".into(),
        show,
        "--format".into(),
        "json".into(),
    ];
    run_cli_verb_json(|| {
        let cmd = crate::whatif::parse(&cli_args)?;
        Ok(crate::whatif::run_captured(cmd))
    })
}

/// Render an `f64` for a CLI `--value`-style flag. Avoids scientific
/// notation for typical numeric inputs (the CLI parser accepts both).
fn format_f64_arg(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        // integer-valued — render without trailing ".0"
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn tool_trace(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let coord = match args.get("coord").and_then(JsonValue::as_str_owned) {
        Some(c) => c,
        None => return error_outcome("missing required argument: coord"),
    };
    let mut cli_args = vec![
        path,
        "--coord".into(),
        coord,
        "--format".into(),
        "json".into(),
    ];
    if let Some(d) = args.get("depth").and_then(JsonValue::as_integer_coerced) {
        cli_args.push("--depth".into());
        cli_args.push(d.to_string());
    }
    run_cli_verb_json(|| {
        let cmd = crate::trace::parse(&cli_args)?;
        Ok(crate::trace::run_captured(cmd))
    })
}

fn tool_sweep(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let range = match args.get("range").and_then(JsonValue::as_str_owned) {
        Some(r) => r,
        None => return error_outcome("missing required argument: range"),
    };
    let metric = match args.get("metric").and_then(JsonValue::as_str_owned) {
        Some(m) => m,
        None => return error_outcome("missing required argument: metric"),
    };
    let goal = match args.get("goal").and_then(JsonValue::as_str_owned) {
        Some(g) => g,
        None => return error_outcome("missing required argument: goal"),
    };
    let mut cli_args = vec![
        path,
        "--range".into(),
        range,
        "--metric".into(),
        metric,
        "--goal".into(),
        goal,
        "--format".into(),
        "json".into(),
    ];
    if let Some(m) = args.get("model").and_then(JsonValue::as_str_owned) {
        cli_args.push("--model".into());
        cli_args.push(m);
    }
    if let Some(c) = args.get("coefficient").and_then(JsonValue::as_str_owned) {
        cli_args.push("--coefficient".into());
        cli_args.push(c);
    }
    if let Some(s) = args.get("set").and_then(JsonValue::as_str_owned) {
        cli_args.push("--set".into());
        cli_args.push(s);
    }
    run_cli_verb_json(|| {
        let cmd = crate::sweep::parse(&cli_args)?;
        Ok(crate::sweep::run_captured(cmd))
    })
}

fn tool_diff(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let left = match args.get("left").and_then(JsonValue::as_str_owned) {
        Some(l) => l,
        None => return error_outcome("missing required argument: left"),
    };
    let right = match args.get("right").and_then(JsonValue::as_str_owned) {
        Some(r) => r,
        None => return error_outcome("missing required argument: right"),
    };
    let mut cli_args = vec![
        path,
        "--left".into(),
        left,
        "--right".into(),
        right,
        "--format".into(),
        "json".into(),
    ];
    if let Some(l) = args.get("limit").and_then(JsonValue::as_integer_coerced) {
        cli_args.push("--limit".into());
        cli_args.push(l.to_string());
    }
    run_cli_verb_json(|| {
        let cmd = crate::diff::parse(&cli_args)?;
        Ok(crate::diff::run_captured(cmd))
    })
}

fn tool_write(args: &JsonValue) -> ToolOutcome {
    let path = match args.get("path").and_then(JsonValue::as_str_owned) {
        Some(p) => p,
        None => return error_outcome("missing required argument: path"),
    };
    let coord = match args.get("coord").and_then(JsonValue::as_str_owned) {
        Some(c) => c,
        None => return error_outcome("missing required argument: coord"),
    };
    let value = match args.get("value").and_then(JsonValue::as_number_coerced) {
        Some(v) => v,
        None => return error_outcome("missing required argument: value"),
    };
    let dry_run = args
        .get("dry_run")
        .and_then(JsonValue::as_bool_coerced)
        .unwrap_or(false);
    let mut cli_args = vec![
        path,
        "--coord".into(),
        coord,
        "--value".into(),
        format_f64_arg(value),
        "--format".into(),
        "json".into(),
    ];
    if dry_run {
        cli_args.push("--dry-run".into());
    }
    run_cli_verb_json(|| {
        let cmd = crate::write::parse(&cli_args)?;
        Ok(crate::write::run_captured(cmd))
    })
}

fn tool_transform(args: &JsonValue) -> ToolOutcome {
    let source = match args.get("source").and_then(JsonValue::as_str_owned) {
        Some(s) => s,
        None => return error_outcome("missing required argument: source"),
    };
    let recipe = match args.get("recipe").and_then(JsonValue::as_str_owned) {
        Some(r) => r,
        None => return error_outcome("missing required argument: recipe"),
    };
    let mut cli_args = vec!["--source".into(), source, "--recipe".into(), recipe];
    let fmt = args
        .get("format")
        .and_then(JsonValue::as_str_owned)
        .unwrap_or_else(|| "csv".into());
    cli_args.push("--format".into());
    cli_args.push(fmt);
    if let Some(o) = args.get("output").and_then(JsonValue::as_str_owned) {
        cli_args.push("--output".into());
        cli_args.push(o);
    }
    if let Some(p) = args.get("preview").and_then(JsonValue::as_integer_coerced) {
        cli_args.push("--preview".into());
        cli_args.push(p.to_string());
    }
    run_cli_verb_json(|| {
        let cmd = crate::transform::parse(&cli_args)?;
        Ok(crate::transform::run_captured(cmd))
    })
}

/// Run a CLI verb, capturing its output into a ToolOutcome and
/// lifting the captured stdout into `structured` when the verb
/// succeeded — for Phase 6A verbs that emit JSON envelopes under MCP.
///
/// Phase 6A.1 MIN-5: closes the gap where `mosaic.model.query` and
/// siblings returned the JSON only via `stdout`, forcing agents to
/// double-parse. The legacy `validate` / `inspect` / `lint` tools
/// already populate `structured`; this brings the new verbs in line.
///
/// Phase 6A.2 item 1.4 W3: `structured` is parsed into a JsonValue
/// at output time (see `ToolOutcome::into_call_result`), so agents
/// receive a structured JSON object rather than a quoted JSON string.
fn run_cli_verb_json<F>(f: F) -> ToolOutcome
where
    F: FnOnce() -> Result<(i32, String), String>,
{
    match f() {
        Ok((exit_code, output)) => {
            let structured = if exit_code == 0 && !output.is_empty() {
                Some(output.clone())
            } else {
                None
            };
            ToolOutcome {
                exit_code,
                stdout: output,
                structured,
            }
        }
        Err(e) => error_outcome(&e),
    }
}

fn run_one(
    golden: &mc_model::ParsedGoldenTest,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    cube: &mut mc_core::Cube,
) -> GoldenEntry {
    let coord = match refs.coord_from_names(&golden.coord) {
        Some(c) => c,
        None => {
            return GoldenEntry {
                json: golden_entry(
                    &golden.name,
                    "Error",
                    None,
                    None,
                    None,
                    None,
                    Some("coord_from_names failed"),
                ),
                failed: true,
            };
        }
    };
    let read = cube.read(&coord, principal);
    match read {
        Ok(cell) => match cell.value {
            mc_core::ScalarValue::F64(actual) => {
                let (expected, epsilon) = match (golden.expect, &golden.expect_within_epsilon) {
                    (Some(v), _) => (v, 1e-9_f64),
                    (None, Some(e)) => (e.value, e.epsilon),
                    (None, None) => {
                        return GoldenEntry {
                            json: golden_entry(
                                &golden.name,
                                "Error",
                                None,
                                Some(actual),
                                None,
                                None,
                                Some("golden has neither expect nor expect_within_epsilon"),
                            ),
                            failed: true,
                        };
                    }
                };
                let delta = actual - expected;
                let passed = delta.abs() < epsilon;
                GoldenEntry {
                    json: golden_entry(
                        &golden.name,
                        if passed { "Pass" } else { "Fail" },
                        Some(expected),
                        Some(actual),
                        Some(delta),
                        Some(epsilon),
                        None,
                    ),
                    failed: !passed,
                }
            }
            other => GoldenEntry {
                json: golden_entry(
                    &golden.name,
                    "Error",
                    None,
                    None,
                    None,
                    None,
                    Some(&format!("expected F64, got {other:?}")),
                ),
                failed: true,
            },
        },
        Err(e) => GoldenEntry {
            json: golden_entry(
                &golden.name,
                "Error",
                None,
                None,
                None,
                None,
                Some(&format!("read error: {e}")),
            ),
            failed: true,
        },
    }
}

fn golden_entry(
    name: &str,
    status: &str,
    expected: Option<f64>,
    actual: Option<f64>,
    delta: Option<f64>,
    epsilon: Option<f64>,
    note: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str("{\"name\": ");
    json_emit_string(&mut s, name);
    s.push_str(", \"status\": ");
    json_emit_string(&mut s, status);
    s.push_str(", \"expected\": ");
    push_optional_number(&mut s, expected);
    s.push_str(", \"actual\": ");
    push_optional_number(&mut s, actual);
    s.push_str(", \"delta\": ");
    push_optional_number(&mut s, delta);
    s.push_str(", \"epsilon\": ");
    push_optional_number(&mut s, epsilon);
    s.push_str(", \"note\": ");
    match note {
        Some(n) => json_emit_string(&mut s, n),
        None => s.push_str("null"),
    }
    s.push('}');
    s
}

fn push_optional_number(out: &mut String, v: Option<f64>) {
    match v {
        Some(f) if f.is_finite() => {
            use std::fmt::Write as _;
            let _ = write!(out, "{f}");
        }
        _ => out.push_str("null"),
    }
}

// ============================================================================
// Shared helpers — load + render diagnostics.
// ============================================================================

/// Load + parse + validate + resolve_inputs without printing anything;
/// on failure return a Vec<Diagnostic> ready for envelope emission.
fn load_validated_quiet(path: &str) -> Result<ValidatedModel, Vec<Diagnostic>> {
    let yaml = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return Err(vec![io_diagnostic(path, &e.to_string())]),
    };
    let parsed = match mc_model::parse(&yaml, Some(path.to_string())) {
        Ok(p) => p,
        Err(e) => return Err(vec![parse_diagnostic(path, &e)]),
    };
    let validated = match mc_model::validate(parsed) {
        Ok(v) => v,
        Err(errs) => return Err(mixed_errors_to_diagnostics(&errs, path)),
    };
    let model_dir = std::path::Path::new(path).parent();
    if let Err(errs) = resolve_inputs(&validated, model_dir) {
        return Err(validation_errors_to_diagnostics(&errs, path));
    }
    Ok(validated)
}

fn io_diagnostic(path: &str, msg: &str) -> Diagnostic {
    Diagnostic {
        code: "MC0001",
        severity: Severity::Error,
        path: ModelPath::new(path, "/", "(io)"),
        message: format!("could not read model file: {msg}"),
        suggestion: None,
    }
}

fn parse_diagnostic(path: &str, e: &mc_model::ParseError) -> Diagnostic {
    let span = e.span();
    let dpath = ModelPath {
        file: path.into(),
        span: Some(mc_model::diagnostic::Span::new(span.line, span.column)),
        yaml_pointer: "/".into(),
        model_path: "(yaml)".into(),
    };
    Diagnostic {
        code: e.code(),
        severity: Severity::Error,
        path: dpath,
        message: e.to_string(),
        suggestion: None,
    }
}

fn mixed_errors_to_diagnostics(errs: &[mc_model::Error], path: &str) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = errs
        .iter()
        .map(|e| match e {
            mc_model::Error::Validation(v) => Diagnostic {
                code: v.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(model)"),
                message: v.to_string(),
                suggestion: None,
            },
            mc_model::Error::Parse(p) => Diagnostic {
                code: p.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(formula)"),
                message: p.to_string(),
                suggestion: None,
            },
            other => Diagnostic {
                code: other.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(model)"),
                message: other.to_string(),
                suggestion: None,
            },
        })
        .collect();
    sort_diagnostics(&mut diags);
    diags
}

fn validation_errors_to_diagnostics(errs: &[ValidationError], path: &str) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = errs
        .iter()
        .map(|v| Diagnostic {
            code: v.code(),
            severity: Severity::Error,
            path: ModelPath::new(path, "/", "(model)"),
            message: v.to_string(),
            suggestion: None,
        })
        .collect();
    sort_diagnostics(&mut diags);
    diags
}

fn error_outcome(msg: &str) -> ToolOutcome {
    ToolOutcome {
        exit_code: 1,
        stdout: msg.to_string(),
        structured: None,
    }
}

// ============================================================================
// JSON-RPC response shaping.
// ============================================================================

fn success_response(id: &JsonValue, result: JsonValue) -> String {
    let mut s = String::new();
    s.push_str("{\"jsonrpc\":\"2.0\",\"id\":");
    json_emit(&mut s, id);
    s.push_str(",\"result\":");
    json_emit(&mut s, &result);
    s.push('}');
    s
}

fn error_response(id: &JsonValue, code: i64, msg: &str) -> String {
    let mut s = String::new();
    s.push_str("{\"jsonrpc\":\"2.0\",\"id\":");
    json_emit(&mut s, id);
    s.push_str(",\"error\":{\"code\":");
    use std::fmt::Write as _;
    let _ = write!(s, "{code}");
    s.push_str(",\"message\":");
    json_emit_string(&mut s, msg);
    s.push_str("}}");
    s
}

// ============================================================================
// Hand-rolled minimal JSON parser + emitter.
//
// Goal: parse the subset of JSON that MCP requests and our internal
// tool arguments use — objects with string keys, strings (with escape
// sequences including \uXXXX), numbers (i64 or f64; both stored as
// f64), booleans, null, and arrays. Roughly the spec'd JSON subset
// with no extensions.
//
// SPEC QUESTION trigger #10 budget: the parser body (parse_json,
// JsonValue, ParseCursor + helpers) targets ~250 lines; if it grows
// past that the Phase 4A.1 fallback is the documented escape hatch.
// As of Phase 4A initial implementation the parser fits in ~200 lines.
// ============================================================================

#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_object(&self) -> Option<&Vec<(String, JsonValue)>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
    fn as_str_owned(&self) -> Option<String> {
        match self {
            JsonValue::String(s) => Some(s.clone()),
            _ => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// Phase 6A.2 item 1.4: accept either a JSON number OR a numeric
    /// string (legacy path — Phase 6A advertised numeric params as
    /// `"string"` in the JSON schema, so old clients have been sending
    /// strings). Both shapes coerce to f64.
    fn as_number_coerced(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            JsonValue::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
    /// Like [`as_number_coerced`] but rounds-and-checks for integers.
    /// Used for `limit`, `depth`, `preview`, `offset` MCP params.
    fn as_integer_coerced(&self) -> Option<i64> {
        match self {
            JsonValue::Number(n) => {
                if n.is_finite() && *n == n.trunc() {
                    Some(*n as i64)
                } else {
                    None
                }
            }
            JsonValue::String(s) => s.parse::<i64>().ok().or_else(|| {
                s.parse::<f64>()
                    .ok()
                    .filter(|f| *f == f.trunc())
                    .map(|f| f as i64)
            }),
            _ => None,
        }
    }
    /// Coerce a JSON bool, or a string `"true"`/`"false"`. Used by MCP
    /// boolean params that may have been advertised as `"string"`.
    fn as_bool_coerced(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            JsonValue::String(s) => match s.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }
    fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn get_field(&self, key: &str) -> Option<&JsonValue> {
        self.get(key)
    }
}

struct ParseCursor<'a> {
    src: &'a str,
    pos: usize,
}

fn parse_json(input: &str) -> Result<JsonValue, String> {
    let mut c = ParseCursor { src: input, pos: 0 };
    c.skip_ws();
    let v = c.parse_value()?;
    c.skip_ws();
    if c.pos < c.src.len() {
        return Err(format!("trailing garbage at byte {}", c.pos));
    }
    Ok(v)
}

impl<'a> ParseCursor<'a> {
    fn peek_byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }
    fn bump_byte(&mut self) -> Option<u8> {
        let b = self.peek_byte()?;
        self.pos += 1;
        Some(b)
    }
    /// Decode the next char (advancing past its UTF-8 bytes). Returns
    /// `Err` if the cursor is at EOF or sees an invalid UTF-8 boundary.
    fn next_char(&mut self) -> Result<char, String> {
        let rest = &self.src[self.pos..];
        let mut chars = rest.char_indices();
        let (_, ch) = chars.next().ok_or_else(|| "unexpected EOF".to_string())?;
        self.pos += ch.len_utf8();
        Ok(ch)
    }
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek_byte() {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn expect(&mut self, expected: u8) -> Result<(), String> {
        match self.bump_byte() {
            Some(b) if b == expected => Ok(()),
            Some(b) => Err(format!(
                "expected '{}', got '{}' at {}",
                expected as char,
                b as char,
                self.pos - 1
            )),
            None => Err(format!("expected '{}', got EOF", expected as char)),
        }
    }
    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(other) => Err(format!("unexpected byte 0x{:02x} at {}", other, self.pos)),
            None => Err("unexpected EOF parsing value".into()),
        }
    }
    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut out = Vec::new();
        if self.peek_byte() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(out));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            out.push((key, value));
            self.skip_ws();
            match self.bump_byte() {
                Some(b',') => continue,
                Some(b'}') => break,
                Some(b) => {
                    return Err(format!(
                        "expected ',' or '}}' in object, got '{}'",
                        b as char
                    ))
                }
                None => return Err("unexpected EOF in object".into()),
            }
        }
        Ok(JsonValue::Object(out))
    }
    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut out = Vec::new();
        if self.peek_byte() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(out));
        }
        loop {
            let value = self.parse_value()?;
            out.push(value);
            self.skip_ws();
            match self.bump_byte() {
                Some(b',') => continue,
                Some(b']') => break,
                Some(b) => {
                    return Err(format!("expected ',' or ']' in array, got '{}'", b as char))
                }
                None => return Err("unexpected EOF in array".into()),
            }
        }
        Ok(JsonValue::Array(out))
    }
    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            // Look at the next byte first so escape processing stays
            // ASCII-fast; non-ASCII bytes are decoded as a full char.
            match self.peek_byte() {
                None => return Err("unterminated string".into()),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1; // consume the backslash
                    match self.bump_byte() {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'b') => out.push('\x08'),
                        Some(b'f') => out.push('\x0c'),
                        Some(b'u') => {
                            let cp = self.read_hex4()?;
                            if (0xD800..=0xDBFF).contains(&cp) {
                                // high surrogate; expect a paired low
                                if self.bump_byte() != Some(b'\\') || self.bump_byte() != Some(b'u')
                                {
                                    return Err("invalid surrogate pair (missing low)".into());
                                }
                                let low = self.read_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err("invalid surrogate pair (low out of range)".into());
                                }
                                let scalar = 0x10000 + (((cp - 0xD800) << 10) | (low - 0xDC00));
                                if let Some(c) = char::from_u32(scalar) {
                                    out.push(c);
                                } else {
                                    return Err("invalid surrogate scalar".into());
                                }
                            } else if let Some(c) = char::from_u32(cp) {
                                out.push(c);
                            } else {
                                return Err("invalid \\u escape".into());
                            }
                        }
                        Some(other) => return Err(format!("unknown escape \\{}", other as char)),
                        None => return Err("EOF after backslash".into()),
                    }
                }
                Some(b) if b < 0x20 => {
                    return Err(format!("control byte 0x{:02x} in string", b));
                }
                Some(b) if b < 0x80 => {
                    self.pos += 1;
                    out.push(b as char);
                }
                Some(_) => {
                    // Multi-byte UTF-8 sequence — decode one char.
                    let ch = self.next_char()?;
                    out.push(ch);
                }
            }
        }
    }
    fn read_hex4(&mut self) -> Result<u32, String> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            match self.bump_byte() {
                Some(b @ b'0'..=b'9') => v = v * 16 + (b - b'0') as u32,
                Some(b @ b'a'..=b'f') => v = v * 16 + (10 + b - b'a') as u32,
                Some(b @ b'A'..=b'F') => v = v * 16 + (10 + b - b'A') as u32,
                _ => return Err("invalid \\u hex".into()),
            }
        }
        Ok(v)
    }
    fn parse_bool(&mut self) -> Result<JsonValue, String> {
        if self.peek_byte() == Some(b't') {
            for &b in b"true" {
                if self.bump_byte() != Some(b) {
                    return Err("expected 'true'".into());
                }
            }
            Ok(JsonValue::Bool(true))
        } else {
            for &b in b"false" {
                if self.bump_byte() != Some(b) {
                    return Err("expected 'false'".into());
                }
            }
            Ok(JsonValue::Bool(false))
        }
    }
    fn parse_null(&mut self) -> Result<JsonValue, String> {
        for &b in b"null" {
            if self.bump_byte() != Some(b) {
                return Err("expected 'null'".into());
            }
        }
        Ok(JsonValue::Null)
    }
    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek_byte() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.peek_byte() == Some(b'.') {
            self.pos += 1;
            while let Some(b) = self.peek_byte() {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek_byte(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek_byte(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while let Some(b) = self.peek_byte() {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let s = &self.src[start..self.pos];
        let v: f64 = s
            .parse()
            .map_err(|e| format!("invalid number {s:?}: {e}"))?;
        Ok(JsonValue::Number(v))
    }
}

// ============================================================================
// JSON emitter — produces compact JSON. Output is a single line per
// MCP message, no trailing newline (the framing layer adds it).
// ============================================================================

fn json_emit(out: &mut String, v: &JsonValue) {
    match v {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(true) => out.push_str("true"),
        JsonValue::Bool(false) => out.push_str("false"),
        JsonValue::Number(n) => {
            use std::fmt::Write as _;
            if n.is_finite() {
                // integers render as integers when whole
                if n.fract() == 0.0 && n.abs() < 1e16 {
                    let _ = write!(out, "{}", *n as i64);
                } else {
                    let _ = write!(out, "{n}");
                }
            } else {
                out.push_str("null");
            }
        }
        JsonValue::String(s) => json_emit_string(out, s),
        JsonValue::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_emit(out, item);
            }
            out.push(']');
        }
        JsonValue::Object(o) => {
            out.push('{');
            for (i, (k, v)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_emit_string(out, k);
                out.push(':');
                json_emit(out, v);
            }
            out.push('}');
        }
    }
}

fn json_emit_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
