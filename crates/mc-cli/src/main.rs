//! `mc` — Mosaic CLI (renamed from "MarketingCubes" on 2026-05-03;
//! the `mc` binary name stays as a backronym for "Mosaic CLI").
//!
//! Phase 1A: `mc demo` ran the Acme cube end-to-end (brief §4.6).
//! Phase 3A: added `--model <path>` so the demo could route through
//!           `mc_model::load`. Stdout is byte-for-byte identical.
//! Phase 3B: adds the `mc model {validate, inspect, lint, test}` group
//!           plus a `--format text|json` modifier and a
//!           `--deny-warnings` modifier on `mc model lint` only.
//!
//! Per [ADR-0005](../../../docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md)
//! amendment #12: `mc demo --model` does **not** run goldens. Goldens
//! are exclusively `mc model test`'s job. The two responsibilities are
//! kept separate so a CI job that just wants "did the demo execute?"
//! does not trip on golden-test failures unrelated to demo execution.

use mc_core::{CellValue, ScalarValue, TraceNode, TraceOp, WriteIntent, WritebackRequest};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs, AcmeRefs};
use mc_model::{
    apply_canonical_inputs, apply_fixture, diagnostics_to_json, diagnostics_to_text, inspect_json,
    inspect_text_with_diagnostics, lint_with_file, resolve_inputs, sort_diagnostics, Diagnostic,
    ModelPath, Severity, ValidatedModel, ValidationError, SCHEMA_VERSION,
};

mod build_benchmarks;
mod diff;
mod ledger_export;
mod loader;
mod mcp;
mod narrate;
mod narrate_trends;
mod query;
mod query_ledger;
mod show_benchmarks;
mod sweep;
mod tessera;
mod trace;
mod transform;
mod whatif;
mod write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        run_demo(None);
        return;
    }
    match args[1].as_str() {
        "demo" => match parse_demo_args(&args[2..]) {
            Ok(model_path) => run_demo(model_path.as_deref()),
            Err(e) => fatal(&e),
        },
        "model" => {
            // Phase 6A: new verbs dispatched before legacy ModelCommand parsing.
            if args.len() > 2 {
                match args[2].as_str() {
                    "query" => match query::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(query::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "whatif" => match whatif::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(whatif::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "trace" => match trace::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(trace::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "sweep" => match sweep::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(sweep::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "diff" => match diff::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(diff::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "write" => match write::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(write::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "narrate" => match narrate::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(narrate::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "narrate-trends" => match narrate_trends::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(narrate_trends::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "query-ledger" => match query_ledger::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(query_ledger::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "ledger-export" => match ledger_export::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(ledger_export::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "build-benchmarks" => match build_benchmarks::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(build_benchmarks::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    "show-benchmarks" => match show_benchmarks::parse(&args[3..]) {
                        Ok(cmd) => std::process::exit(show_benchmarks::run(cmd)),
                        Err(e) => fatal(&e),
                    },
                    _ => {} // Fall through to legacy parse
                }
            }
            match parse_model_args(&args[2..]) {
                Ok(cmd) => run_model(cmd),
                Err(e) => fatal(&e),
            }
        }
        "tessera" => {
            // Phase 6A: intercept "transform" verb before tessera::parse
            if args.len() > 2 && args[2] == "transform" {
                match transform::parse(&args[3..]) {
                    Ok(cmd) => std::process::exit(transform::run(cmd)),
                    Err(e) => fatal(&e),
                }
            }
            match tessera::parse(&args[2..]) {
                Ok(cmd) => std::process::exit(tessera::run(cmd)),
                Err(e) => fatal(&e),
            }
        }
        "mcp" => mcp::run(),
        "start" => run_start(&args[2..]),
        "--help" | "-h" | "help" => print_help(),
        other => {
            eprintln!("unknown command: {other:?}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

fn run_start(args: &[String]) {
    let mut port: u16 = 8080;
    let mut static_dir: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--port" => match iter.next() {
                Some(p) => {
                    port = p.parse().unwrap_or_else(|_| {
                        eprintln!("error: invalid port: {p}");
                        std::process::exit(2);
                    });
                }
                None => fatal("--port requires a number"),
            },
            "--static" => match iter.next() {
                Some(d) => static_dir = Some(d.clone()),
                None => fatal("--static requires a directory path"),
            },
            other => fatal(&format!("unknown argument to `mc start`: {other:?}")),
        }
    }
    mc_demo_server::run(port, static_dir.as_deref());
}

fn print_help() {
    println!("mc — Mosaic CLI");
    println!();
    println!("USAGE:");
    println!("    mc demo [--model <path>]               # Run the Acme demo");
    println!("    mc start [--port N] [--static <dir>]   # Start the demo server + open browser");
    println!();
    println!("    mc model validate <path> [--format text|json]");
    println!("    mc model inspect  <path> [--format text|json]");
    println!("    mc model lint     <path> [--format text|json] [--deny-warnings]");
    println!("    mc model test     <path> [--format text|json] [--fixture <name>]");
    println!();
    println!(
        "    mc model query  <path> [--where <expr>] [--show <measures>] [--format text|json|csv]"
    );
    println!("                           [--coord <coord>] [--aggregate <fns>] [--output <file>]");
    println!("                           [--group-by <Dim>] (repeatable; requires --aggregate)");
    println!(
        "    mc model whatif <path> --set <coord>=<n> [--set ...] --show <measures> [--format ...]"
    );
    println!(
        "                           (legacy single-cell form: --set <coord> --value <n>; --set is repeatable)"
    );
    println!("    mc model trace  <path> --coord <coord> [--depth <n>] [--format text|json|csv]");
    println!("    mc model sweep  <path> --range <start:end:step> --metric <fn> --goal <min|max>");
    println!("                           [--model <name> --coefficient <name>] [--set <coord>]");
    println!("                           [--metric-where <expr>] [--format text|json|csv]");
    println!(
        "    mc model diff   <path> --left <filter> --right <filter> [--format text|json|csv]"
    );
    println!("    mc model write  <path> --coord <coord> --value <n> [--dry-run] [--format ...]");
    println!("    mc model narrate <path> [--templates <dir>] [--format text|json|markdown] [--save-ledger]");
    println!("    mc model query-ledger <model-dir> [--severity <s>] [--template <id>] [--since <period>]");
    println!("                           [--scope <k=v>] [--repeated <n>] [--format text|json]");
    println!("    mc model ledger-export <model-dir> [--format jsonl|csv] [--since <period>]");
    println!();
    println!("    mc tessera apply      <recipe.yaml>            [--format text|json]");
    println!("    mc tessera dry-run    <recipe.yaml>            [--format text|json]");
    println!("    mc tessera propose    --source <path> --model <path>");
    println!("    mc tessera transform  --source <path|url> --recipe <path> [--output <file>]");
    println!(
        "                           [--format csv|json|text] [--preview <n>] [--timeout-secs <n>]"
    );
    println!("    mc tessera history    <model_dir>              [--format text|json]");
    println!("    mc tessera rollback   <import_id> --model-dir <path> [--format text|json]");
    println!("    mc tessera audit      <model_dir>              [--format text|json]");
    println!();
    println!("    mc mcp                                  # MCP server (stdio JSON-RPC)");
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_demo_args(args: &[String]) -> Result<Option<String>, String> {
    let mut model_path: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--model" => match iter.next() {
                Some(p) => model_path = Some(p.clone()),
                None => return Err("--model requires a path argument".into()),
            },
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    Ok(model_path)
}

#[derive(Debug)]
#[allow(dead_code)]
struct ModelCommand {
    verb: ModelVerb,
    path: String,
    format: OutputFormat,
    deny_warnings: bool,
    /// Phase 3C `--fixture <name>` filter for `mc model test`. Filter
    /// semantics per ADR-0006 Decision 7: when set, only goldens whose
    /// `fixture:` field equals this name are run; the rest are reported
    /// as skipped.
    fixture_filter: Option<String>,
    /// Phase 5C `--time-anchor <element>` override. Per ADR-0014
    /// Decision 4, overrides the YAML time_anchor default.
    time_anchor: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelVerb {
    Validate,
    Inspect,
    Lint,
    Test,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

fn parse_model_args(args: &[String]) -> Result<ModelCommand, String> {
    if args.is_empty() {
        return Err("`mc model` requires a verb (validate|inspect|lint|test)".into());
    }
    let verb = match args[0].as_str() {
        "validate" => ModelVerb::Validate,
        "inspect" => ModelVerb::Inspect,
        "lint" => ModelVerb::Lint,
        "test" => ModelVerb::Test,
        other => return Err(format!("unknown model verb: {other:?}")),
    };

    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut deny_warnings = false;
    let mut fixture_filter: Option<String> = None;
    let mut time_anchor: Option<String> = None;
    let mut iter = args[1..].iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) => return Err(format!("--format must be `text` or `json`, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--deny-warnings" => {
                if verb != ModelVerb::Lint {
                    return Err("--deny-warnings is only valid for `mc model lint`".into());
                }
                deny_warnings = true;
            }
            "--fixture" => {
                if verb != ModelVerb::Test {
                    return Err("--fixture is only valid for `mc model test`".into());
                }
                match iter.next() {
                    Some(v) => fixture_filter = Some(v.clone()),
                    None => return Err("--fixture requires a name argument".into()),
                }
            }
            "--time-anchor" => {
                if verb != ModelVerb::Test {
                    return Err("--time-anchor is only valid for `mc model test`".into());
                }
                match iter.next() {
                    Some(v) => time_anchor = Some(v.clone()),
                    None => return Err("--time-anchor requires an element name argument".into()),
                }
            }
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or_else(|| format!("`mc model {verb:?}` requires a YAML path"))?;
    Ok(ModelCommand {
        verb,
        path,
        format,
        deny_warnings,
        fixture_filter,
        time_anchor,
    })
}

// ---------------------------------------------------------------------------
// Model subcommand dispatch
// ---------------------------------------------------------------------------

fn run_model(cmd: ModelCommand) {
    match cmd.verb {
        ModelVerb::Validate => run_validate(&cmd.path, cmd.format),
        ModelVerb::Inspect => run_inspect(&cmd.path, cmd.format),
        ModelVerb::Lint => run_lint(&cmd.path, cmd.format, cmd.deny_warnings),
        ModelVerb::Test => run_test(
            &cmd.path,
            cmd.format,
            cmd.fixture_filter.as_deref(),
            cmd.time_anchor.as_deref(),
        ),
    }
}

/// Load `path` as a `ValidatedModel`. Bypasses the compile stage so we
/// don't pay kernel construction cost on the validate/inspect/lint paths.
/// On parse, validation, or resolve-inputs error, prints diagnostics
/// in the requested format and exits non-zero.
///
/// Phase 3C: also runs the resolve-inputs stage so the
/// `mc model validate` user surface catches MC2012–MC2025 fixture/CSV
/// errors. Per the project owner's architectural clarification, this
/// is a named stage between `validate()` and `compile()`; it produces
/// `ValidationError` diagnostics because they are model-invalidating
/// fixture/input errors, but `validate()` itself remains
/// filesystem-free.
fn load_validated(path: &str, format: OutputFormat) -> ValidatedModel {
    let yaml = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            print_io_error(path, &e.to_string(), format);
            std::process::exit(1);
        }
    };
    let parsed = match mc_model::parse(&yaml, Some(path.to_string())) {
        Ok(p) => p,
        Err(e) => {
            print_parse_error(path, &e, format);
            std::process::exit(1);
        }
    };
    let validated = match mc_model::validate(parsed) {
        Ok(v) => v,
        Err(errs) => {
            // Phase 3D: validate now returns Vec<Error> mixing
            // ParseError (MC1003-MC1006) with ValidationError (MC2xxx).
            print_mixed_errors(path, &errs, format);
            std::process::exit(1);
        }
    };
    // Resolve-inputs stage. Discards the resolved data; only the
    // validation side-effects matter at this layer (`mc model test`
    // re-runs resolve_inputs to actually use the data).
    let model_dir = std::path::Path::new(path).parent();
    if let Err(errs) = resolve_inputs(&validated, model_dir) {
        print_validation_errors(path, &errs, format);
        std::process::exit(1);
    }
    validated
}

fn run_validate(path: &str, format: OutputFormat) {
    let _ = load_validated(path, format);
    if let OutputFormat::Json = format {
        // Empty diagnostics envelope on success keeps the JSON contract
        // uniform with lint's empty-case output.
        print!("{}", diagnostics_to_json(&[]));
    }
    // Text format: silent on success per ADR-0005 Decision 3.
}

fn run_inspect(path: &str, format: OutputFormat) {
    let model = load_validated(path, format);
    // Inspect runs lint to populate the "Diagnostics: N errors, ..." line
    // — purely informational; lint never blocks inspect.
    let mut diags = lint_with_file(&model, path);
    sort_diagnostics(&mut diags);
    // Phase 3C: also resolve_inputs so the summary can show row counts
    // for canonical_inputs / test_fixtures. load_validated already
    // pre-cleared resolve-inputs errors, so this call is for data
    // extraction only.
    let model_dir = std::path::Path::new(path).parent();
    let inputs = resolve_inputs(&model, model_dir).ok();
    match format {
        OutputFormat::Text => print!(
            "{}",
            inspect_text_with_diagnostics(&model, &diags, inputs.as_ref())
        ),
        OutputFormat::Json => print!("{}", inspect_json(&model, &diags, inputs.as_ref())),
    }
}

fn run_lint(path: &str, format: OutputFormat, deny_warnings: bool) {
    let model = load_validated(path, format);
    let mut diags = lint_with_file(&model, path);
    sort_diagnostics(&mut diags);
    match format {
        OutputFormat::Text => {
            if !diags.is_empty() {
                print!("{}", diagnostics_to_text(&diags));
            }
        }
        OutputFormat::Json => print!("{}", diagnostics_to_json(&diags)),
    }
    if deny_warnings && !diags.is_empty() {
        std::process::exit(1);
    }
}

fn run_test(
    path: &str,
    format: OutputFormat,
    fixture_filter: Option<&str>,
    time_anchor: Option<&str>,
) {
    // Phase 3C: load + resolve_inputs + compile, then apply
    // canonical_inputs and run goldens against the model-owned data.
    // Generic flow — no metadata.name special cases.
    let model = load_validated(path, format);

    // Re-run resolve_inputs to obtain the typed row data. The first
    // call (inside load_validated) handled validation; the second call
    // is pure data extraction. CSV is read twice; at Acme size that's
    // microseconds — well within the < 500 ms perf gate.
    let model_dir = std::path::Path::new(path).parent();
    let inputs = match resolve_inputs(&model, model_dir) {
        Ok(i) => i,
        Err(errs) => {
            // Should not happen — load_validated already pre-cleared.
            print_validation_errors(path, &errs, format);
            std::process::exit(1);
        }
    };

    // Compile to a Cube. The validator pre-cleared every kernel surface
    // that returns a structured error, so this should not normally fail.
    let compiled = match mc_model::compile(model.clone()) {
        Ok(c) => c,
        Err(e) => {
            print_compile_error(path, &e.to_string(), format);
            std::process::exit(1);
        }
    };
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;

    // Per ADR-0014 Decision 4: CLI --time-anchor overrides the YAML default.
    if let Some(anchor_name) = time_anchor {
        // Find the element index first (immutable borrow), then assign (mutable).
        let anchor_idx = cube.dimensions().iter().find_map(|dim| {
            dim.elements.iter().enumerate().find_map(|(idx, elem)| {
                if elem.name == anchor_name {
                    Some(idx)
                } else {
                    None
                }
            })
        });
        match anchor_idx {
            Some(idx) => cube.reference_data.time_anchor_index = Some(idx),
            None => {
                print_compile_error(
                    path,
                    &format!(
                        "--time-anchor '{anchor_name}' does not match any element in any dimension"
                    ),
                    format,
                );
                std::process::exit(1);
            }
        }
    }

    // Apply canonical_inputs (no-op if the model didn't declare any).
    if let Err(e) = apply_canonical_inputs(&mut cube, &compiled.refs, principal, &inputs) {
        print_compile_error(path, &format!("apply_canonical_inputs failed: {e}"), format);
        std::process::exit(1);
    }

    // Per ADR-0006 amendment #17: snapshot once after canonical_inputs
    // load; rollback only between goldens that mutated the cube via a
    // fixture overlay. Read-only goldens (no `fixture:`) skip rollback
    // since the cube state is unchanged across them.
    let snap = cube.snapshot(None);

    let mut results: Vec<GoldenResult> = Vec::with_capacity(model.parsed.golden_tests.len());
    let mut skipped_count = 0usize;
    let mut any_failed = false;

    for golden in &model.parsed.golden_tests {
        // --fixture <name> filter: skip goldens that don't reference
        // the named fixture. Filter-only semantic per ADR-0006
        // Decision 7 + amendment (g).
        if let Some(filter) = fixture_filter {
            let matches = golden
                .fixture
                .as_deref()
                .map(|n| n == filter)
                .unwrap_or(false);
            if !matches {
                skipped_count += 1;
                continue;
            }
        }

        // Apply this golden's fixture overlay (override semantic over
        // canonical_inputs). Track whether we mutated so we know to
        // rollback after the check.
        let mut mutated = false;
        if let Some(fname) = &golden.fixture {
            // resolve_inputs already validated this reference (MC2017),
            // so the lookup is total; the unwrap_or branch is a
            // belt-and-suspenders.
            if let Some(fixture) = inputs.fixture(fname) {
                if let Err(e) = apply_fixture(&mut cube, &compiled.refs, principal, fixture) {
                    results.push(GoldenResult {
                        name: golden.name.clone(),
                        status: GoldenStatus::Error,
                        expected: golden
                            .expect
                            .or(golden.expect_within_epsilon.as_ref().map(|e| e.value)),
                        actual: None,
                        delta: None,
                        epsilon: golden.expect_within_epsilon.as_ref().map(|e| e.epsilon),
                        note: Some(format!("fixture {fname:?} apply error: {e}")),
                    });
                    any_failed = true;
                    // Best-effort rollback before continuing.
                    let _ = cube.rollback_to(&snap);
                    continue;
                }
                mutated = true;
            }
        }

        let result = run_one_golden(golden, &compiled.refs, principal, &mut cube);
        if !matches!(result.status, GoldenStatus::Pass) {
            any_failed = true;
        }
        results.push(result);

        if mutated {
            if let Err(e) = cube.rollback_to(&snap) {
                eprintln!("warning: between-goldens rollback failed: {e}");
            }
        }
    }

    match format {
        OutputFormat::Text => print_goldens_text(&results, skipped_count),
        OutputFormat::Json => print_goldens_json(&results, skipped_count),
    }
    if any_failed {
        std::process::exit(1);
    }
}

fn run_one_golden(
    golden: &mc_model::ParsedGoldenTest,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    cube: &mut mc_core::Cube,
) -> GoldenResult {
    let coord = match refs.coord_from_names(&golden.coord) {
        Some(c) => c,
        None => {
            return GoldenResult {
                name: golden.name.clone(),
                status: GoldenStatus::Error,
                expected: golden
                    .expect
                    .or(golden.expect_within_epsilon.as_ref().map(|e| e.value)),
                actual: None,
                delta: None,
                epsilon: golden.expect_within_epsilon.as_ref().map(|e| e.epsilon),
                note: Some(
                    "coord_from_names failed — coord references unknown dim/element name(s)"
                        .to_string(),
                ),
            };
        }
    };
    match cube.read(&coord, principal) {
        Ok(cell) => match cell.value {
            ScalarValue::F64(actual) => {
                let (expected, epsilon) = match (golden.expect, &golden.expect_within_epsilon) {
                    (Some(v), _) => (v, 1e-9_f64),
                    (None, Some(e)) => (e.value, e.epsilon),
                    (None, None) => {
                        return GoldenResult {
                            name: golden.name.clone(),
                            status: GoldenStatus::Error,
                            expected: None,
                            actual: Some(actual),
                            delta: None,
                            epsilon: None,
                            note: Some(
                                "golden has neither `expect` nor `expect_within_epsilon`".into(),
                            ),
                        };
                    }
                };
                let delta = actual - expected;
                let passed = delta.abs() < epsilon;
                GoldenResult {
                    name: golden.name.clone(),
                    status: if passed {
                        GoldenStatus::Pass
                    } else {
                        GoldenStatus::Fail
                    },
                    expected: Some(expected),
                    actual: Some(actual),
                    delta: Some(delta),
                    epsilon: Some(epsilon),
                    note: None,
                }
            }
            other => GoldenResult {
                name: golden.name.clone(),
                status: GoldenStatus::Error,
                expected: golden
                    .expect
                    .or(golden.expect_within_epsilon.as_ref().map(|e| e.value)),
                actual: None,
                delta: None,
                epsilon: golden.expect_within_epsilon.as_ref().map(|e| e.epsilon),
                note: Some(format!("expected F64, got {other:?}")),
            },
        },
        Err(e) => GoldenResult {
            name: golden.name.clone(),
            status: GoldenStatus::Error,
            expected: golden
                .expect
                .or(golden.expect_within_epsilon.as_ref().map(|e| e.value)),
            actual: None,
            delta: None,
            epsilon: golden.expect_within_epsilon.as_ref().map(|e| e.epsilon),
            note: Some(format!("read error: {e}")),
        },
    }
}

#[derive(Clone, Copy, Debug)]
enum GoldenStatus {
    Pass,
    Fail,
    Error,
}

impl GoldenStatus {
    fn label(self) -> &'static str {
        match self {
            GoldenStatus::Pass => "Pass",
            GoldenStatus::Fail => "Fail",
            GoldenStatus::Error => "Error",
        }
    }
}

#[derive(Debug)]
struct GoldenResult {
    name: String,
    status: GoldenStatus,
    expected: Option<f64>,
    actual: Option<f64>,
    delta: Option<f64>,
    epsilon: Option<f64>,
    note: Option<String>,
}

fn print_goldens_text(results: &[GoldenResult], skipped: usize) {
    let total = results.len();
    let passed = results
        .iter()
        .filter(|r| matches!(r.status, GoldenStatus::Pass))
        .count();
    let failed = total - passed;
    for r in results {
        match r.status {
            GoldenStatus::Pass => println!(
                "PASS {} (expected {:?}, actual {:?})",
                r.name, r.expected, r.actual
            ),
            GoldenStatus::Fail => println!(
                "FAIL {} (expected {:?}, actual {:?}, Δ {:?}, ε {:?})",
                r.name, r.expected, r.actual, r.delta, r.epsilon
            ),
            GoldenStatus::Error => println!(
                "ERROR {} ({})",
                r.name,
                r.note.as_deref().unwrap_or("(no note)")
            ),
        }
    }
    println!();
    if skipped > 0 {
        println!("Goldens: {passed}/{total} passed, {failed} failed, {skipped} skipped (filtered)");
    } else {
        println!("Goldens: {passed}/{total} passed, {failed} failed");
    }
}

fn print_goldens_json(results: &[GoldenResult], skipped: usize) {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"");
    out.push_str(SCHEMA_VERSION);
    out.push_str("\",\n  \"skipped\": ");
    out.push_str(&skipped.to_string());
    out.push_str(",\n  \"goldens\": [");
    if results.is_empty() {
        out.push_str("]\n}\n");
        print!("{out}");
        return;
    }
    out.push('\n');
    for (i, r) in results.iter().enumerate() {
        out.push_str("    {\"name\": ");
        write_json_str(&mut out, &r.name);
        out.push_str(", \"status\": ");
        write_json_str(&mut out, r.status.label());
        out.push_str(", \"expected\": ");
        push_optional_number(&mut out, r.expected);
        out.push_str(", \"actual\": ");
        push_optional_number(&mut out, r.actual);
        out.push_str(", \"delta\": ");
        push_optional_number(&mut out, r.delta);
        out.push_str(", \"epsilon\": ");
        push_optional_number(&mut out, r.epsilon);
        out.push_str(", \"note\": ");
        match &r.note {
            Some(n) => write_json_str(&mut out, n),
            None => out.push_str("null"),
        }
        out.push('}');
        if i + 1 < results.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    print!("{out}");
}

fn push_optional_number(out: &mut String, v: Option<f64>) {
    match v {
        Some(f) if f.is_finite() => {
            use std::fmt::Write;
            let _ = write!(out, "{f}");
        }
        _ => out.push_str("null"),
    }
}

fn write_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------
// Error rendering for parse / validate / compile errors. Reuses the
// Diagnostic shape so JSON output stays envelope-uniform.
// ---------------------------------------------------------------------------

fn print_io_error(path: &str, message: &str, format: OutputFormat) {
    let diag = Diagnostic {
        code: "MC0001",
        severity: Severity::Error,
        path: ModelPath::new(path, "/", "(io)"),
        message: format!("could not read model file: {message}"),
        suggestion: None,
    };
    emit_error_diags(&[diag], format);
}

fn print_parse_error(path: &str, e: &mc_model::ParseError, format: OutputFormat) {
    let span = e.span();
    let dpath = ModelPath {
        file: path.into(),
        span: Some(mc_model::diagnostic::Span::new(span.line, span.column)),
        yaml_pointer: "/".into(),
        model_path: "(yaml)".into(),
    };
    let diag = Diagnostic {
        code: e.code(),
        severity: Severity::Error,
        path: dpath,
        message: e.to_string(),
        suggestion: None,
    };
    emit_error_diags(&[diag], format);
}

fn print_validation_errors(path: &str, errs: &[ValidationError], format: OutputFormat) {
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
    emit_error_diags(&diags, format);
}

/// Phase 3D: render the mixed `Vec<Error>` returned by `validate()`.
/// Splits the vec by variant — formula parse errors (MC1003–MC1006) flow
/// through `print_parse_error`'s diagnostic shape, semantic-validation
/// errors stay on the `print_validation_errors` path. The diagnostic
/// envelope shape (Phase 3B) is unchanged: same five fields, same
/// `schema_version: "1.0"`.
fn print_mixed_errors(path: &str, errs: &[mc_model::Error], format: OutputFormat) {
    let mut diags: Vec<Diagnostic> = Vec::with_capacity(errs.len());
    for e in errs {
        match e {
            mc_model::Error::Validation(v) => diags.push(Diagnostic {
                code: v.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(model)"),
                message: v.to_string(),
                suggestion: None,
            }),
            mc_model::Error::Parse(p) => diags.push(Diagnostic {
                code: p.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(formula)"),
                message: p.to_string(),
                suggestion: None,
            }),
            other => diags.push(Diagnostic {
                code: other.code(),
                severity: Severity::Error,
                path: ModelPath::new(path, "/", "(model)"),
                message: other.to_string(),
                suggestion: None,
            }),
        }
    }
    sort_diagnostics(&mut diags);
    emit_error_diags(&diags, format);
}

fn print_compile_error(path: &str, message: &str, format: OutputFormat) {
    let diag = Diagnostic {
        code: "MC0002",
        severity: Severity::Error,
        path: ModelPath::new(path, "/", "(compile)"),
        message: message.to_string(),
        suggestion: None,
    };
    emit_error_diags(&[diag], format);
}

fn emit_error_diags(diags: &[Diagnostic], format: OutputFormat) {
    match format {
        OutputFormat::Text => eprint!("{}", diagnostics_to_text(diags)),
        OutputFormat::Json => print!("{}", diagnostics_to_json(diags)),
    }
}

// ---------------------------------------------------------------------------
// Acme demo (Phase 1A / 3A path) — preserved verbatim except that the
// `--model` flow now runs through a typed loader that doesn't touch
// goldens (per amendment #12).
// ---------------------------------------------------------------------------

fn run_demo(model_path: Option<&str>) {
    println!("Building Acme cube...");
    let (mut cube, refs) = match model_path {
        // Per amendment #12: --model loads + validates + compiles +
        // runs. NEVER runs goldens. Use mc model test for goldens.
        Some(path) => load_acme_from_yaml(path),
        None => build_acme_cube().expect("acme fixture must build"),
    };
    let dims = cube.dimensions().len();
    let hierarchies = cube
        .dimensions()
        .iter()
        .filter(|d| !d.default_hierarchy().edges.is_empty())
        .count();
    let measures = cube.measure_dimension().elements.len();
    let rules = cube.rules().len();
    println!("  {dims} dimensions, {hierarchies} hierarchies, {measures} measures, {rules} rules");
    let count = write_canonical_inputs(&mut cube, &refs).expect("canonical inputs");
    println!("  Loaded {count} input cells in 1 scenario × 1 version");
    println!();

    let cube_id = cube.id;
    let principal = refs.root_principal;

    let leaf = |measure| {
        coord_at(
            cube_id,
            &refs,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            measure,
        )
    };
    let read = |c: &mc_core::CellCoordinate, cube: &mut mc_core::Cube| -> CellValue {
        cube.read(c, principal).expect("read")
    };

    println!("Reading sample cells:");
    let labels = [
        ("Spend", refs.spend),
        ("Clicks", refs.clicks),
        ("Leads", refs.leads),
        ("Customers", refs.customers),
        ("Revenue", refs.revenue),
        ("Gross_Profit", refs.gross_profit),
    ];
    for (label, m) in &labels {
        let v = read(&leaf(*m), &mut cube);
        println!(
            "  (Baseline, Working, Mar_2026, Paid_Search, Tampa, {:<12}) = {:>14}",
            label,
            format_f64(&v.value)
        );
    }
    println!();

    println!("Reading consolidated cells:");
    let consolidated_cells: [(&str, bool, mc_core::CellCoordinate); 5] = [
        (
            "Q1_2026,  Paid_Search, Tampa,   Spend",
            false,
            coord_at(
                cube_id,
                &refs,
                refs.q1_2026,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            ),
        ),
        (
            "Mar_2026, Paid_Search, Florida, Spend",
            false,
            coord_at(
                cube_id,
                &refs,
                refs.mar_2026,
                refs.paid_search,
                refs.florida,
                refs.spend,
            ),
        ),
        (
            "Mar_2026, Paid_Media,  Tampa,   Spend",
            false,
            coord_at(
                cube_id,
                &refs,
                refs.mar_2026,
                refs.paid_media,
                refs.tampa,
                refs.spend,
            ),
        ),
        (
            "Q1_2026,  Paid_Media,  Florida, Spend",
            false,
            coord_at(
                cube_id,
                &refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.spend,
            ),
        ),
        (
            "Q1_2026,  Paid_Search, Florida, CPC  ",
            true,
            coord_at(
                cube_id,
                &refs,
                refs.q1_2026,
                refs.paid_search,
                refs.florida,
                refs.cpc,
            ),
        ),
    ];
    for (label, is_ratio, c) in consolidated_cells.iter() {
        let v = read(c, &mut cube);
        println!(
            "  (Baseline, Working, {label}) = {:>14}",
            format_value(&v.value, *is_ratio)
        );
    }
    println!();

    println!("Trace for (Mar_2026, Paid_Search, Tampa, Revenue):");
    let revenue_leaf = leaf(refs.revenue);
    let v = cube
        .read_with_trace(&revenue_leaf, principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace requested");
    let measure_dim_pos = cube.dimensions().len() - 1;
    let measure_dim = cube.measure_dimension();
    print_trace_root(&trace.root, measure_dim, measure_dim_pos);
    println!();

    println!("Writing Spend(Mar_2026, Paid_Search, Tampa) = 50_000:");
    let revision_before = cube.revision();
    let result = cube
        .write(WritebackRequest {
            coord: leaf(refs.spend),
            new_value: ScalarValue::F64(50_000.0),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write ok");
    println!(
        "  Written. Revision {} → {}.",
        revision_before.0, result.revision_after.0
    );
    println!(
        "  {} dependent cells dirtied. (bounded per brief §8)",
        result.invalidated.len()
    );
    println!();

    println!("Re-reading Revenue:");
    let post_revenue = read(&leaf(refs.revenue), &mut cube)
        .value
        .as_f64()
        .expect("F64");
    let post_gp = read(&leaf(refs.gross_profit), &mut cube)
        .value
        .as_f64()
        .expect("F64");
    println!(
        "  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue)      = {:>14}   (was 3_066.67)",
        format_amount(post_revenue)
    );
    println!(
        "  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Gross_Profit) = {:>14}   (was 2_146.67)",
        format_amount(post_gp)
    );
    println!();

    println!("Rejecting write to Revenue (derived):");
    let err = cube
        .write(WritebackRequest {
            coord: leaf(refs.revenue),
            new_value: ScalarValue::F64(99.0),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("derived must reject");
    println!("  Error: {err}");
    println!();

    println!("Rejecting write to Q1_2026 Spend (consolidated):");
    let err = cube
        .write(WritebackRequest {
            coord: coord_at(
                cube_id,
                &refs,
                refs.q1_2026,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            ),
            new_value: ScalarValue::F64(50_000.0),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("consolidated must reject");
    println!("  Error: {err}");
    println!();

    println!("Done.");
}

fn load_acme_from_yaml(path: &str) -> (mc_core::Cube, AcmeRefs) {
    let compiled = mc_model::load(path).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("model error: {e}");
        }
        std::process::exit(1);
    });
    let refs = build_acme_refs_or_die(&compiled.refs, compiled.root_principal, path);
    (compiled.cube, refs)
}

fn try_build_acme_refs(
    refs: &mc_model::ModelRefs,
    root_principal: mc_core::PrincipalId,
) -> Option<AcmeRefs> {
    // Gracefully attempts construction; returns None on first missing
    // ref. The `mc model test` path uses this to skip canonical-inputs
    // for non-Acme YAMLs that happen to share the metadata.name match.
    use mc_core::{DimensionId, ElementId, RuleId};
    let r = |dim: &str, name: &str| -> Option<ElementId> { refs.element(dim, name) };
    let dim = |name: &str| -> Option<DimensionId> { refs.dimensions.get(name).copied() };
    let rule = |name: &str| -> Option<RuleId> { refs.rules.get(name).copied() };
    Some(AcmeRefs {
        root_principal,
        scenario_dim: dim("Scenario")?,
        version_dim: dim("Version")?,
        time_dim: dim("Time")?,
        channel_dim: dim("Channel")?,
        market_dim: dim("Market")?,
        measure_dim: dim("Measure")?,
        time_hierarchy: mc_core::HierarchyId(0),
        channel_hierarchy: mc_core::HierarchyId(0),
        market_hierarchy: mc_core::HierarchyId(0),
        scen_baseline: r("Scenario", "Baseline")?,
        scen_aggressive: r("Scenario", "Aggressive")?,
        scen_conservative: r("Scenario", "Conservative")?,
        ver_working: r("Version", "Working")?,
        ver_submitted: r("Version", "Submitted")?,
        ver_approved: r("Version", "Approved")?,
        jan_2026: r("Time", "Jan_2026")?,
        feb_2026: r("Time", "Feb_2026")?,
        mar_2026: r("Time", "Mar_2026")?,
        apr_2026: r("Time", "Apr_2026")?,
        may_2026: r("Time", "May_2026")?,
        jun_2026: r("Time", "Jun_2026")?,
        jul_2026: r("Time", "Jul_2026")?,
        aug_2026: r("Time", "Aug_2026")?,
        sep_2026: r("Time", "Sep_2026")?,
        oct_2026: r("Time", "Oct_2026")?,
        nov_2026: r("Time", "Nov_2026")?,
        dec_2026: r("Time", "Dec_2026")?,
        q1_2026: r("Time", "Q1_2026")?,
        q2_2026: r("Time", "Q2_2026")?,
        q3_2026: r("Time", "Q3_2026")?,
        q4_2026: r("Time", "Q4_2026")?,
        fy_2026: r("Time", "FY_2026")?,
        paid_search: r("Channel", "Paid_Search")?,
        paid_social: r("Channel", "Paid_Social")?,
        display: r("Channel", "Display")?,
        email: r("Channel", "Email")?,
        organic: r("Channel", "Organic")?,
        paid_media: r("Channel", "Paid_Media")?,
        owned_earned: r("Channel", "Owned_Earned")?,
        all_channels: r("Channel", "All_Channels")?,
        tampa: r("Market", "Tampa")?,
        orlando: r("Market", "Orlando")?,
        miami: r("Market", "Miami")?,
        atlanta: r("Market", "Atlanta")?,
        charlotte: r("Market", "Charlotte")?,
        new_york_city: r("Market", "New_York_City")?,
        boston: r("Market", "Boston")?,
        florida: r("Market", "Florida")?,
        georgia: r("Market", "Georgia")?,
        north_carolina: r("Market", "North_Carolina")?,
        new_york_state: r("Market", "New_York_State")?,
        massachusetts: r("Market", "Massachusetts")?,
        southeast: r("Market", "Southeast")?,
        northeast: r("Market", "Northeast")?,
        usa: r("Market", "USA")?,
        spend: r("Measure", "Spend")?,
        cpc: r("Measure", "CPC")?,
        cvr: r("Measure", "CVR")?,
        close_rate: r("Measure", "Close_Rate")?,
        aov: r("Measure", "AOV")?,
        cogs_rate: r("Measure", "COGS_Rate")?,
        clicks: r("Measure", "Clicks")?,
        leads: r("Measure", "Leads")?,
        customers: r("Measure", "Customers")?,
        revenue: r("Measure", "Revenue")?,
        gross_profit: r("Measure", "Gross_Profit")?,
        rule_clicks: rule("rule_clicks")?,
        rule_leads: rule("rule_leads")?,
        rule_customers: rule("rule_customers")?,
        rule_revenue: rule("rule_revenue")?,
        rule_gross_profit: rule("rule_gross_profit")?,
    })
}

fn build_acme_refs_or_die(
    refs: &mc_model::ModelRefs,
    root_principal: mc_core::PrincipalId,
    path: &str,
) -> AcmeRefs {
    try_build_acme_refs(refs, root_principal).unwrap_or_else(|| {
        eprintln!("error: {path:?} is missing one or more dimensions/elements/rules required by the Acme demo");
        std::process::exit(1);
    })
}

fn coord_at(
    cube_id: mc_core::CubeId,
    refs: &AcmeRefs,
    time: mc_core::ElementId,
    channel: mc_core::ElementId,
    market: mc_core::ElementId,
    measure: mc_core::ElementId,
) -> mc_core::CellCoordinate {
    coord(
        cube_id,
        refs,
        refs.scen_baseline,
        refs.ver_working,
        time,
        channel,
        market,
        measure,
    )
}

fn format_f64(v: &ScalarValue) -> String {
    format_value(v, /* ratio */ false)
}

fn format_value(v: &ScalarValue, ratio: bool) -> String {
    match v {
        ScalarValue::F64(f) => {
            if ratio {
                format!("{f:.7}")
            } else {
                format_amount(*f)
            }
        }
        ScalarValue::Null => "Null".to_string(),
        other => format!("{other:?}"),
    }
}

fn format_amount(v: f64) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    let v = v.abs();
    let int = v.trunc() as i64;
    let frac = ((v - int as f64) * 100.0).round() as i64;
    format!("{sign}{}.{:02}", with_underscores(int), frac)
}

fn with_underscores(mut n: i64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut digits: Vec<u8> = Vec::new();
    while n > 0 {
        digits.push((n % 10) as u8);
        n /= 10;
    }
    let mut out = String::new();
    for (i, d) in digits.iter().rev().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('_');
        }
        out.push((b'0' + d) as char);
    }
    out
}

fn print_trace_root(node: &TraceNode, measure_dim: &mc_core::Dimension, measure_pos: usize) {
    println!("  {}", trace_label(node, measure_dim, measure_pos));
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let last = i + 1 == n;
        print_trace_child(child, "  ", last, measure_dim, measure_pos);
    }
}

fn print_trace_child(
    node: &TraceNode,
    prefix: &str,
    is_last: bool,
    measure_dim: &mc_core::Dimension,
    measure_pos: usize,
) {
    let connector = if is_last { "└── " } else { "├── " };
    println!(
        "{prefix}{connector}{}",
        trace_label(node, measure_dim, measure_pos)
    );
    let new_prefix = if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let last = i + 1 == n;
        print_trace_child(child, &new_prefix, last, measure_dim, measure_pos);
    }
}

fn trace_label(node: &TraceNode, measure_dim: &mc_core::Dimension, measure_pos: usize) -> String {
    let measure_id = node.coord.element_at(measure_pos);
    let measure_name = measure_dim
        .element(measure_id)
        .map(|e| e.name.as_str())
        .unwrap_or("?");
    let value = format_f64(&node.value);
    let op_label = match &node.operation {
        TraceOp::InputLookup { .. } => "Input".to_string(),
        TraceOp::RuleEvaluation {
            rule_id,
            expr_summary,
        } => format!("Rule {rule_id:?}: {:?}", expr_summary.op),
        TraceOp::Consolidation { child_count, .. } => {
            format!("Consolidation × {child_count}")
        }
        TraceOp::DefaultFallback { reason, .. } => format!("Default ({reason})"),
        TraceOp::NullPoison { upstream } => format!("NullPoison from {upstream:?}"),
    };
    format!("{measure_name} = {value} ({op_label})")
}
