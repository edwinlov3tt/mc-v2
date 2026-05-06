//! `mc model trace` — show the computation chain for one derived cell.
//!
//! Returns a hierarchical tree showing how a derived value was computed,
//! all the way down to input values. The "explainability" feature.

use crate::query::{
    load_model, parse_coord_string, push_json_envelope_header, push_json_str, OutputFormat,
};
use mc_core::{ScalarValue, TraceNode, TraceOp};
use std::fmt::Write;

pub struct TraceCommand {
    pub path: String,
    pub format: OutputFormat,
    pub coord: String,
    pub depth: Option<usize>,
    pub time_anchor: Option<String>,
}

pub fn parse(args: &[String]) -> Result<TraceCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut coord: Option<String> = None;
    let mut depth: Option<usize> = None;
    let mut time_anchor: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) if v == "csv" => format = OutputFormat::Csv,
                Some(v) => return Err(format!("--format must be text|json|csv, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--coord" => match iter.next() {
                Some(v) => coord = Some(v.clone()),
                None => return Err("--coord requires a coordinate string".into()),
            },
            "--depth" => match iter.next() {
                Some(v) => {
                    depth = Some(
                        v.parse::<usize>()
                            .map_err(|_| format!("--depth must be a number, got {v:?}"))?,
                    )
                }
                None => return Err("--depth requires a number".into()),
            },
            "--time-anchor" => match iter.next() {
                Some(v) => time_anchor = Some(v.clone()),
                None => return Err("--time-anchor requires an element name".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model trace` requires a YAML model path")?;
    let coord = coord.ok_or("--coord is required")?;
    Ok(TraceCommand {
        path,
        format,
        coord,
        depth,
        time_anchor,
    })
}

pub fn run(cmd: TraceCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the trace verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: TraceCommand) -> (i32, String) {
    let loaded = match load_model(&cmd.path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return (e.exit_code(), String::new());
        }
    };
    let mut cube = loaded.cube;
    let principal = loaded.root_principal;
    let refs = &loaded.refs;

    // Apply time-anchor override
    if let Some(anchor_name) = &cmd.time_anchor {
        let anchor_idx = cube.dimensions().iter().find_map(|dim| {
            dim.elements.iter().enumerate().find_map(|(idx, elem)| {
                if elem.name == *anchor_name {
                    Some(idx)
                } else {
                    None
                }
            })
        });
        match anchor_idx {
            Some(idx) => cube.reference_data.time_anchor_index = Some(idx),
            None => {
                eprintln!("error: --time-anchor '{anchor_name}' does not match any element");
                return (1, String::new());
            }
        }
    }

    // Resolve coord
    let coord_names = parse_coord_string(&cmd.coord);
    let coord = match refs.coord_from_names(&coord_names) {
        Some(c) => c,
        None => {
            eprintln!("error: could not resolve coordinate: {}", cmd.coord);
            return (1, String::new());
        }
    };

    // Read with trace
    let cell = match cube.read_with_trace(&coord, principal) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: read failed: {e}");
            return (1, String::new());
        }
    };

    // Get measure name from coord
    let measure_name = coord_names.get("Measure").cloned().unwrap_or_default();

    // Build trace tree from TraceNode if present
    let trace_tree = match &cell.trace {
        Some(trace) => build_trace_tree(&trace.root, &cube, refs, cmd.depth.unwrap_or(20), 0),
        None => {
            // No trace available — return a simple leaf node
            TraceTree {
                measure: measure_name.clone(),
                value: cell.value.clone(),
                source: "input".to_string(),
                rule: None,
                formula: None,
                inputs: Vec::new(),
            }
        }
    };

    let output_str = match cmd.format {
        OutputFormat::Json => format_trace_json(&trace_tree),
        OutputFormat::Text => format_trace_text(&trace_tree, 0),
        OutputFormat::Csv => format_trace_flat_csv(&trace_tree),
    };
    (0, output_str)
}

// ---------------------------------------------------------------------------
// Internal trace tree representation
// ---------------------------------------------------------------------------

struct TraceTree {
    measure: String,
    value: ScalarValue,
    source: String, // "input" | "rule" | "consolidation"
    rule: Option<String>,
    formula: Option<String>,
    inputs: Vec<(String, TraceTree)>,
}

fn build_trace_tree(
    node: &TraceNode,
    cube: &mc_core::Cube,
    refs: &mc_model::ModelRefs,
    max_depth: usize,
    current_depth: usize,
) -> TraceTree {
    // Get measure name from coord
    let measure_dim_idx = cube
        .dimensions()
        .iter()
        .position(|d| d.kind == mc_core::DimensionKind::Measure)
        .unwrap_or(0);
    let measure_dim = &cube.dimensions()[measure_dim_idx];
    let measure_elem_id = node.coord.elements()[measure_dim_idx];
    let measure_name = measure_dim
        .element(measure_elem_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| format!("?{:?}", measure_elem_id));

    let (source, rule_name, formula) = match &node.operation {
        TraceOp::InputLookup { .. } => ("input".to_string(), None, None),
        TraceOp::RuleEvaluation {
            rule_id,
            expr_summary,
        } => {
            let rule_name = refs
                .rules
                .iter()
                .find(|(_, &id)| id == *rule_id)
                .map(|(name, _)| name.clone());
            let formula = Some(format!("{:?}", expr_summary.op));
            ("rule".to_string(), rule_name, formula)
        }
        TraceOp::Consolidation { child_count, .. } => {
            (format!("consolidation({child_count} children)"), None, None)
        }
        TraceOp::DefaultFallback { reason, .. } => (format!("default({reason})"), None, None),
        TraceOp::NullPoison { .. } => ("null_poison".to_string(), None, None),
    };

    let inputs = if current_depth >= max_depth {
        Vec::new()
    } else {
        node.children
            .iter()
            .map(|child| {
                let child_measure_id = child.coord.elements()[measure_dim_idx];
                let child_name = measure_dim
                    .element(child_measure_id)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| "?".to_string());
                let child_tree = build_trace_tree(child, cube, refs, max_depth, current_depth + 1);
                (child_name, child_tree)
            })
            .collect()
    };

    TraceTree {
        measure: measure_name,
        value: node.value.clone(),
        source,
        rule: rule_name,
        formula,
        inputs,
    }
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

fn format_trace_json(tree: &TraceTree) -> String {
    let mut out = String::new();
    push_json_envelope_header(&mut out);
    out.push_str("\"trace\": ");
    write_trace_json_node(&mut out, tree, 1);
    out.push_str("\n}\n");
    out
}

fn write_trace_json_node(out: &mut String, tree: &TraceTree, indent: usize) {
    let pad = "  ".repeat(indent);
    out.push_str(&format!("{pad}{{\n"));
    let inner = "  ".repeat(indent + 1);

    out.push_str(&format!("{inner}\"measure\": "));
    push_json_str(out, &tree.measure);
    out.push_str(",\n");

    out.push_str(&format!("{inner}\"value\": "));
    match &tree.value {
        ScalarValue::F64(f) => {
            if *f == f.trunc() && f.abs() < 1e15 {
                let _ = write!(out, "{}", *f as i64);
            } else {
                let _ = write!(out, "{f}");
            }
        }
        ScalarValue::Null => out.push_str("null"),
        other => out.push_str(&format!("\"{}\"", crate::query::format_scalar(other))),
    }
    out.push_str(",\n");

    if let Some(rule) = &tree.rule {
        out.push_str(&format!("{inner}\"rule\": "));
        push_json_str(out, rule);
        out.push_str(",\n");
    }
    if let Some(formula) = &tree.formula {
        out.push_str(&format!("{inner}\"formula\": "));
        push_json_str(out, formula);
        out.push_str(",\n");
    }

    out.push_str(&format!("{inner}\"source\": "));
    push_json_str(out, &tree.source);

    if tree.inputs.is_empty() {
        out.push('\n');
    } else {
        out.push_str(",\n");
        out.push_str(&format!("{inner}\"inputs\": {{\n"));
        for (i, (name, child)) in tree.inputs.iter().enumerate() {
            let inner2 = "  ".repeat(indent + 2);
            out.push_str(&inner2.to_string());
            push_json_str(out, name);
            out.push_str(": ");
            write_trace_json_node(out, child, indent + 2);
            if i + 1 < tree.inputs.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&format!("{inner}}}\n"));
    }
    out.push_str(&format!("{pad}}}"));
}

fn format_trace_text(tree: &TraceTree, depth: usize) -> String {
    let mut out = String::new();
    write_trace_text_node(&mut out, tree, "", true, depth == 0);
    out
}

fn write_trace_text_node(
    out: &mut String,
    tree: &TraceTree,
    prefix: &str,
    is_last: bool,
    is_root: bool,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };
    let value_str = crate::query::format_scalar(&tree.value);
    let source_tag = if tree.source == "input" {
        " (input)"
    } else {
        ""
    };
    let _ = writeln!(
        out,
        "{prefix}{connector}{} = {value_str}{source_tag}",
        tree.measure
    );

    let child_prefix = if is_root {
        "".to_string()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    for (i, (_, child)) in tree.inputs.iter().enumerate() {
        let last = i + 1 == tree.inputs.len();
        write_trace_text_node(out, child, &child_prefix, last, false);
    }
}

fn format_trace_flat_csv(tree: &TraceTree) -> String {
    let mut out = String::from("measure,value,source,rule\n");
    write_trace_csv_row(&mut out, tree);
    out
}

fn write_trace_csv_row(out: &mut String, tree: &TraceTree) {
    let value_str = crate::query::format_scalar(&tree.value);
    let rule = tree.rule.as_deref().unwrap_or("");
    let _ = writeln!(out, "{},{value_str},{},{rule}", tree.measure, tree.source);
    for (_, child) in &tree.inputs {
        write_trace_csv_row(out, child);
    }
}
