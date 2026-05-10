//! `mc workspace {init, validate, lint, test, inspect}` CLI verbs.
//!
//! Phase 4C: workspace manifest + CLI orchestration layer above
//! the existing per-cube pipeline. Per ADR-0026 + Phase 4C handoff.

use std::path::{Path, PathBuf};

/// Workspace subcommand parsed from CLI args.
#[derive(Debug)]
pub enum WorkspaceCommand {
    Init {
        name: String,
        domain: Option<String>,
        path: Option<PathBuf>,
    },
    Validate {
        path: PathBuf,
        format: OutputFormat,
    },
    Lint {
        path: PathBuf,
        format: OutputFormat,
    },
    Test {
        path: PathBuf,
        format: OutputFormat,
    },
    Inspect {
        path: PathBuf,
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Parse `mc workspace <verb> [args]`.
pub fn parse(args: &[String]) -> Result<WorkspaceCommand, String> {
    if args.is_empty() {
        return Err("`mc workspace` requires a verb (init|validate|lint|test|inspect)".into());
    }

    match args[0].as_str() {
        "init" => parse_init(&args[1..]),
        "validate" => parse_path_verb(&args[1..], "validate"),
        "lint" => parse_path_verb(&args[1..], "lint"),
        "test" => parse_path_verb(&args[1..], "test"),
        "inspect" => parse_path_verb(&args[1..], "inspect"),
        other => Err(format!(
            "unknown workspace verb: {other:?} (expected init|validate|lint|test|inspect)"
        )),
    }
}

fn parse_init(args: &[String]) -> Result<WorkspaceCommand, String> {
    let mut name: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut path: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--domain" => match iter.next() {
                Some(d) => domain = Some(d.clone()),
                None => return Err("--domain requires an argument".into()),
            },
            "--path" => match iter.next() {
                Some(p) => path = Some(PathBuf::from(p)),
                None => return Err("--path requires an argument".into()),
            },
            other if !other.starts_with("--") && name.is_none() => {
                name = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let name = name.ok_or_else(|| "`mc workspace init` requires a workspace name".to_string())?;
    Ok(WorkspaceCommand::Init { name, domain, path })
}

fn parse_path_verb(args: &[String], verb: &str) -> Result<WorkspaceCommand, String> {
    let mut path: Option<PathBuf> = None;
    let mut format = OutputFormat::Text;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--path" => match iter.next() {
                Some(p) => path = Some(PathBuf::from(p)),
                None => return Err("--path requires an argument".into()),
            },
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) => return Err(format!("--format must be `text` or `json`, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.unwrap_or_else(|| PathBuf::from("."));
    match verb {
        "validate" => Ok(WorkspaceCommand::Validate { path, format }),
        "lint" => Ok(WorkspaceCommand::Lint { path, format }),
        "test" => Ok(WorkspaceCommand::Test { path, format }),
        "inspect" => Ok(WorkspaceCommand::Inspect { path, format }),
        _ => unreachable!(),
    }
}

/// Execute a workspace command.
pub fn run(cmd: WorkspaceCommand) -> i32 {
    match cmd {
        WorkspaceCommand::Init { name, domain, path } => {
            run_init(&name, domain.as_deref(), path.as_deref())
        }
        WorkspaceCommand::Validate { path, format } => run_validate(&path, format),
        WorkspaceCommand::Lint { path, format } => run_lint(&path, format),
        WorkspaceCommand::Test { path, format } => run_test(&path, format),
        WorkspaceCommand::Inspect { path, format } => run_inspect(&path, format),
    }
}

fn run_init(name: &str, domain: Option<&str>, path: Option<&Path>) -> i32 {
    let dir = path.map_or_else(|| PathBuf::from(name), PathBuf::from);
    match mc_workspace::init_workspace(name, &dir, domain) {
        Ok(()) => {
            println!("Created workspace {name:?} at {}", dir.display());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn run_validate(path: &Path, format: OutputFormat) -> i32 {
    let workspace = match mc_workspace::parse_workspace(path) {
        Ok(ws) => ws,
        Err(e) => {
            print_error(&e, format);
            return 1;
        }
    };

    let mut diags = mc_workspace::validate_workspace(&workspace, path);
    mc_workspace::sort_diagnostics(&mut diags);

    let has_errors = mc_workspace::has_errors(&diags);
    match format {
        OutputFormat::Text => {
            if !diags.is_empty() {
                eprint!("{}", mc_workspace::diagnostics_to_text(&diags));
            }
        }
        OutputFormat::Json => {
            print!("{}", mc_workspace::diagnostics_to_json(&diags));
        }
    }
    if has_errors {
        1
    } else {
        0
    }
}

fn run_lint(path: &Path, format: OutputFormat) -> i32 {
    let workspace = match mc_workspace::parse_workspace(path) {
        Ok(ws) => ws,
        Err(e) => {
            print_error(&e, format);
            return 1;
        }
    };

    // Run validate first to catch hard errors.
    let mut diags = mc_workspace::validate_workspace(&workspace, path);
    if mc_workspace::has_errors(&diags) {
        mc_workspace::sort_diagnostics(&mut diags);
        match format {
            OutputFormat::Text => eprint!("{}", mc_workspace::diagnostics_to_text(&diags)),
            OutputFormat::Json => print!("{}", mc_workspace::diagnostics_to_json(&diags)),
        }
        return 1;
    }

    // Run lint on top of validation.
    let mut lint_diags = mc_workspace::lint_workspace(&workspace, path);
    diags.append(&mut lint_diags);
    mc_workspace::sort_diagnostics(&mut diags);

    match format {
        OutputFormat::Text => {
            if !diags.is_empty() {
                print!("{}", mc_workspace::diagnostics_to_text(&diags));
            }
        }
        OutputFormat::Json => {
            print!("{}", mc_workspace::diagnostics_to_json(&diags));
        }
    }
    0
}

fn run_test(path: &Path, format: OutputFormat) -> i32 {
    let workspace = match mc_workspace::parse_workspace(path) {
        Ok(ws) => ws,
        Err(e) => {
            print_error(&e, format);
            return 1;
        }
    };

    // Validate workspace first.
    let diags = mc_workspace::validate_workspace(&workspace, path);
    if mc_workspace::has_errors(&diags) {
        let mut sorted = diags;
        mc_workspace::sort_diagnostics(&mut sorted);
        match format {
            OutputFormat::Text => eprint!("{}", mc_workspace::diagnostics_to_text(&sorted)),
            OutputFormat::Json => print!("{}", mc_workspace::diagnostics_to_json(&sorted)),
        }
        return 1;
    }

    // Run each cube's tests. Single pipeline per cube:
    // parse → validate → resolve_inputs (with file context) → compile → apply inputs → run goldens.
    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failed = 0usize;

    for entry in &workspace.cubes {
        let cube_path = path.join(&entry.path);
        let cube_name = entry
            .name
            .clone()
            .unwrap_or_else(|| entry.path.display().to_string());

        // Read and resolve $refs.
        let yaml = match std::fs::read_to_string(&cube_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error reading {}: {e}", cube_path.display());
                failed += 1;
                total += 1;
                continue;
            }
        };

        let resolved = if mc_workspace::has_refs(&yaml) {
            match mc_workspace::resolve_refs(&yaml, &workspace, path) {
                Ok(r) => r,
                Err(errs) => {
                    for e in &errs {
                        eprintln!("error in {cube_name}: {e}");
                    }
                    failed += 1;
                    total += 1;
                    continue;
                }
            }
        } else {
            yaml
        };

        // Single parse + validate pass.
        let parsed = match mc_model::parse(&resolved, Some(cube_path.display().to_string())) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error in {cube_name}: {e}");
                failed += 1;
                total += 1;
                continue;
            }
        };
        let validated = match mc_model::validate(parsed) {
            Ok(v) => v,
            Err(errs) => {
                for e in &errs {
                    eprintln!("error in {cube_name}: {e}");
                }
                failed += 1;
                total += 1;
                continue;
            }
        };

        // Resolve inputs with the cube's actual directory as file context,
        // so CSV-sourced canonical_inputs resolve correctly.
        let cube_dir = cube_path.parent();
        let inputs = match mc_model::resolve_inputs(&validated, cube_dir) {
            Ok(i) => Some(i),
            Err(errs) => {
                for e in &errs {
                    eprintln!("error in {cube_name}: {e}");
                }
                // Non-fatal: cube may have no canonical_inputs declared,
                // or the CSV may be absent. Continue with empty inputs.
                None
            }
        };

        // Compile to a Cube.
        let compiled = match mc_model::compile(validated.clone()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error in {cube_name}: compile: {e}");
                failed += 1;
                total += 1;
                continue;
            }
        };

        let mut cube = compiled.cube;
        let principal = compiled.root_principal;

        // Apply canonical inputs if available.
        if let Some(ref resolved_inputs) = inputs {
            if let Err(e) = mc_model::apply_canonical_inputs(
                &mut cube,
                &compiled.refs,
                principal,
                resolved_inputs,
            ) {
                eprintln!("warning: {cube_name}: apply_canonical_inputs: {e}");
            }
        }

        // Run goldens.
        let goldens = &validated.parsed.golden_tests;
        for golden in goldens {
            total += 1;
            let coord = match compiled.refs.coord_from_names(&golden.coord) {
                Some(c) => c,
                None => {
                    if format == OutputFormat::Text {
                        println!("ERROR {cube_name}::{} — invalid coord", golden.name);
                    }
                    failed += 1;
                    continue;
                }
            };
            match cube.read(&coord, principal) {
                Ok(cell) => match cell.value {
                    mc_core::ScalarValue::F64(actual) => {
                        let (expected, epsilon) =
                            match (golden.expect, &golden.expect_within_epsilon) {
                                (Some(v), _) => (v, 1e-9_f64),
                                (None, Some(e)) => (e.value, e.epsilon),
                                (None, None) => {
                                    failed += 1;
                                    continue;
                                }
                            };
                        let delta = (actual - expected).abs();
                        if delta < epsilon {
                            passed += 1;
                            if format == OutputFormat::Text {
                                println!("PASS {cube_name}::{}", golden.name);
                            }
                        } else {
                            failed += 1;
                            if format == OutputFormat::Text {
                                println!(
                                    "FAIL {cube_name}::{} (expected {expected}, got {actual}, delta {delta})",
                                    golden.name
                                );
                            }
                        }
                    }
                    _ => {
                        failed += 1;
                    }
                },
                Err(e) => {
                    if format == OutputFormat::Text {
                        println!("ERROR {cube_name}::{} — {e}", golden.name);
                    }
                    failed += 1;
                }
            }
        }
    }

    match format {
        OutputFormat::Text => {
            println!();
            println!("Workspace test: {passed}/{total} passed, {failed} failed");
        }
        OutputFormat::Json => {
            println!("{{\"total\": {total}, \"passed\": {passed}, \"failed\": {failed}}}");
        }
    }

    if failed > 0 {
        1
    } else {
        0
    }
}

fn run_inspect(path: &Path, format: OutputFormat) -> i32 {
    let workspace = match mc_workspace::parse_workspace(path) {
        Ok(ws) => ws,
        Err(e) => {
            print_error(&e, format);
            return 1;
        }
    };

    let mut diags = mc_workspace::validate_workspace(&workspace, path);
    let mut lint_diags = mc_workspace::lint_workspace(&workspace, path);
    diags.append(&mut lint_diags);
    mc_workspace::sort_diagnostics(&mut diags);

    let summary = mc_workspace::inspect_workspace(&workspace, path);

    match format {
        OutputFormat::Text => {
            print!("{}", mc_workspace::inspect_text(&summary, &diags));
        }
        OutputFormat::Json => {
            print!("{}", mc_workspace::inspect_json(&summary, &diags));
        }
    }
    0
}

fn print_error(e: &mc_workspace::WorkspaceError, format: OutputFormat) {
    let diag = mc_workspace::WorkspaceDiagnostic::from_error(e);
    match format {
        OutputFormat::Text => {
            eprint!("{}", mc_workspace::diagnostics_to_text(&[diag]));
        }
        OutputFormat::Json => {
            print!("{}", mc_workspace::diagnostics_to_json(&[diag]));
        }
    }
}
