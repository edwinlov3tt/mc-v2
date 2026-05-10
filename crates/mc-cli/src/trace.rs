//! `mc model trace` — show the computation chain for one derived cell.
//!
//! Returns a hierarchical tree showing how a derived value was computed,
//! all the way down to input values. The "explainability" feature.
//!
//! Phase 6A.2 items 1.2 + 1.3 reshape the JSON envelope. The breaking
//! changes (documented in handoff §"Backward Compat Inventory"):
//!
//! - `inputs` becomes a JSON **array** (was an object keyed by measure
//!   name — duplicate keys silently dropped consolidated children).
//! - Every node carries `coord` (canonical `Dim=Elem,...` string),
//!   `child_count: usize`, and `formula: string|null`. `measure` and
//!   `rule` are removed (the canonical identifier is `coord`).
//! - Trace's envelope `schema_version` bumps from `"1.0"` to `"1.1"`.
//!   Other Phase 6A verbs stay at `"1.0"`.

use crate::query::{load_model, parse_coord_string, push_json_str, OutputFormat};
use mc_core::{CellCoordinate, DimensionKind, ScalarValue, TraceNode, TraceOp};
use mc_model::ModelRefs;
use std::collections::HashMap;
use std::fmt::Write;

pub struct TraceCommand {
    pub path: String,
    pub format: OutputFormat,
    pub coord: String,
    pub depth: Option<usize>,
    pub time_anchor: Option<String>,
    /// Phase 4D: enrich text output with measure descriptions.
    pub verbose: bool,
}

/// `schema_version` for the trace JSON envelope. Phase 6A.2 item 1.3
/// bumps this from "1.0" → "1.1" (breaking shape change for `inputs`).
const TRACE_SCHEMA_VERSION: &str = "1.1";

pub fn parse(args: &[String]) -> Result<TraceCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut coord: Option<String> = None;
    let mut depth: Option<usize> = None;
    let mut time_anchor: Option<String> = None;
    let mut verbose = false;

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
            "--verbose" | "-v" => verbose = true,
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
        verbose,
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
    let formulas = &loaded.formulas;
    let measure_descs = &loaded.measure_descriptions;

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

    let trace_tree = match &cell.trace {
        Some(trace) => build_trace_tree(
            &trace.root,
            &cube,
            refs,
            formulas,
            cmd.depth.unwrap_or(20),
            0,
        ),
        None => fallback_leaf_tree(&coord, &cell.value, &cube),
    };

    let output_str = match cmd.format {
        OutputFormat::Json => format_trace_json(&trace_tree),
        OutputFormat::Text => {
            let mut out = format_trace_text(&trace_tree, 0);
            // Phase 4D: verbose description at the root of the trace tree.
            if cmd.verbose {
                if let Some(measure_name) =
                    crate::verbose::measure_name_from_coord(&trace_tree.coord)
                {
                    if let Some(desc) =
                        crate::verbose::measure_description(measure_descs, measure_name)
                    {
                        let val_str = crate::query::format_scalar(&trace_tree.value);
                        out.push_str(&crate::verbose::format_description_line(
                            desc,
                            Some(&val_str),
                        ));
                    }
                }
            }
            out
        }
        OutputFormat::Csv => format_trace_flat_csv(&trace_tree),
    };
    (0, output_str)
}

// ---------------------------------------------------------------------------
// Internal trace tree representation
// ---------------------------------------------------------------------------

/// Phase 6A.2 schema 1.1 node shape. Every field is always present;
/// `formula` is null for inputs and consolidations, string for rules.
struct TraceTree {
    /// Canonical coordinate string `Dim1=Elem1,...,Measure=...`.
    coord: String,
    value: ScalarValue,
    /// `"input"` | `"rule"` | `"consolidation"` | `"default"` | `"null_poison"`.
    source: String,
    /// `inputs.len()` — emitted as a separate field so agents can read
    /// it without iterating the array.
    child_count: usize,
    /// Authored formula (rendered via `mc_model::formula::serialize`)
    /// for rule sources; `None` for input/consolidation.
    formula: Option<String>,
    /// Child trace nodes, in the kernel's emit order.
    inputs: Vec<TraceTree>,
}

fn build_trace_tree(
    node: &TraceNode,
    cube: &mc_core::Cube,
    refs: &ModelRefs,
    formulas: &HashMap<String, String>,
    max_depth: usize,
    current_depth: usize,
) -> TraceTree {
    let coord_str = coord_to_canonical_string(&node.coord, cube);

    let (source, formula) = match &node.operation {
        TraceOp::InputLookup { .. } => ("input".to_string(), None),
        TraceOp::RuleEvaluation { rule_id, .. } => {
            let formula = lookup_formula(*rule_id, refs, formulas);
            ("rule".to_string(), formula)
        }
        TraceOp::Consolidation { .. } => ("consolidation".to_string(), None),
        TraceOp::DefaultFallback { .. } => ("default".to_string(), None),
        TraceOp::NullPoison { .. } => ("null_poison".to_string(), None),
    };

    let inputs: Vec<TraceTree> = if current_depth >= max_depth {
        Vec::new()
    } else {
        node.children
            .iter()
            .map(|child| {
                build_trace_tree(child, cube, refs, formulas, max_depth, current_depth + 1)
            })
            .collect()
    };

    TraceTree {
        coord: coord_str,
        value: node.value.clone(),
        source,
        child_count: inputs.len(),
        formula,
        inputs,
    }
}

/// Construct a leaf node for the case where `Cube::read_with_trace`
/// returned no trace at the requested coordinate.
///
/// Phase 6A.2 item 1.3 (Codex M-5 bonus): the previous fallback
/// unconditionally emitted `source: "input"`. If the coord points at
/// a consolidated position (any non-Measure dim element that is not a
/// hierarchy leaf), label as `"consolidation"` and report the
/// expanded leaf count via `child_count`.
fn fallback_leaf_tree(
    coord: &CellCoordinate,
    value: &ScalarValue,
    cube: &mc_core::Cube,
) -> TraceTree {
    let coord_str = coord_to_canonical_string(coord, cube);
    let consolidated_count = consolidated_leaf_count(coord, cube);
    let (source, child_count) = match consolidated_count {
        Some(n) => ("consolidation".to_string(), n),
        None => ("input".to_string(), 0),
    };
    TraceTree {
        coord: coord_str,
        value: value.clone(),
        source,
        child_count,
        formula: None,
        inputs: Vec::new(),
    }
}

/// If `coord` points at a consolidated position, return the count of
/// leaf coords it expands to. Otherwise (every non-Measure dim element
/// is a leaf) return `None`.
fn consolidated_leaf_count(coord: &CellCoordinate, cube: &mc_core::Cube) -> Option<usize> {
    let dims = cube.dimensions();
    let elements = coord.elements();
    let mut count: usize = 1;
    let mut any_consolidated = false;
    for (dim_idx, dim) in dims.iter().enumerate() {
        if dim.kind == DimensionKind::Measure {
            continue;
        }
        let elem_id = elements[dim_idx];
        let hierarchy = dim.default_hierarchy();
        if hierarchy.edges.is_empty() {
            // Flat dimension — every element is a leaf; this dim
            // contributes a factor of 1 to the count.
            continue;
        }
        if hierarchy.is_leaf(elem_id) {
            continue;
        }
        any_consolidated = true;
        // Number of leaf descendants (the kernel uses (ElementId, weight)
        // pairs for `descendants`; for our count we just want the
        // distinct leaves under this element).
        let leaves_under: usize = hierarchy
            .descendants(elem_id)
            .into_iter()
            .filter(|(child_id, _)| hierarchy.is_leaf(*child_id))
            .count();
        // Defensive: an element with no leaf descendants would yield 0,
        // which would zero out the product. Treat that as a flat 1.
        count = count.saturating_mul(if leaves_under == 0 { 1 } else { leaves_under });
    }
    if any_consolidated {
        Some(count)
    } else {
        None
    }
}

/// Render a `CellCoordinate` as the canonical `"Dim1=Elem1,Dim2=Elem2,..."`
/// string used by `--coord` flags and by the trace envelope's `coord`
/// field.
fn coord_to_canonical_string(coord: &CellCoordinate, cube: &mc_core::Cube) -> String {
    let dims = cube.dimensions();
    let elements = coord.elements();
    let mut out = String::new();
    for (dim_idx, dim) in dims.iter().enumerate() {
        if dim_idx > 0 {
            out.push(',');
        }
        out.push_str(&dim.name);
        out.push('=');
        let elem_id = elements[dim_idx];
        let elem_name = dim.element(elem_id).map(|e| e.name.as_str()).unwrap_or("?");
        out.push_str(elem_name);
    }
    out
}

/// Look up the rendered formula string for a kernel `RuleId`.
fn lookup_formula(
    rule_id: mc_core::RuleId,
    refs: &ModelRefs,
    formulas: &HashMap<String, String>,
) -> Option<String> {
    // `refs.rules` is name → RuleId; reverse-lookup gives us the
    // authored rule name. The formulas map (built from
    // `mc_model::inspect::summarize`) is keyed by the same name.
    refs.rules
        .iter()
        .find(|(_, &id)| id == rule_id)
        .and_then(|(name, _)| formulas.get(name).cloned())
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

fn format_trace_json(tree: &TraceTree) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"");
    out.push_str(TRACE_SCHEMA_VERSION);
    out.push_str("\",\n  \"trace\": ");
    write_trace_json_node(&mut out, tree, 1);
    out.push_str("\n}\n");
    out
}

fn write_trace_json_node(out: &mut String, tree: &TraceTree, indent: usize) {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);

    out.push_str(&format!("{pad}{{\n"));

    out.push_str(&format!("{inner}\"coord\": "));
    push_json_str(out, &tree.coord);
    out.push_str(",\n");

    out.push_str(&format!("{inner}\"value\": "));
    push_value_json(out, &tree.value);
    out.push_str(",\n");

    out.push_str(&format!("{inner}\"source\": "));
    push_json_str(out, &tree.source);
    out.push_str(",\n");

    let _ = write!(out, "{inner}\"child_count\": {}", tree.child_count);
    out.push_str(",\n");

    out.push_str(&format!("{inner}\"formula\": "));
    match &tree.formula {
        Some(f) => push_json_str(out, f),
        None => out.push_str("null"),
    }
    out.push_str(",\n");

    out.push_str(&format!("{inner}\"inputs\": "));
    if tree.inputs.is_empty() {
        out.push_str("[]");
    } else {
        out.push_str("[\n");
        for (i, child) in tree.inputs.iter().enumerate() {
            write_trace_json_node(out, child, indent + 2);
            if i + 1 < tree.inputs.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&format!("{inner}]"));
    }
    out.push('\n');
    out.push_str(&format!("{pad}}}"));
}

fn push_value_json(out: &mut String, v: &ScalarValue) {
    match v {
        ScalarValue::F64(f) => {
            if *f == f.trunc() && f.abs() < 1e15 {
                let _ = write!(out, "{}", *f as i64);
            } else {
                let _ = write!(out, "{f}");
            }
        }
        ScalarValue::Null => out.push_str("null"),
        other => {
            let s = crate::query::format_scalar(other);
            push_json_str(out, &s);
        }
    }
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
    let source_tag = match tree.source.as_str() {
        "input" => " (input)",
        "consolidation" => " (consolidation)",
        _ => "",
    };
    let _ = writeln!(
        out,
        "{prefix}{connector}{} = {value_str}{source_tag}",
        tree.coord
    );

    let child_prefix = if is_root {
        "".to_string()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    for (i, child) in tree.inputs.iter().enumerate() {
        let last = i + 1 == tree.inputs.len();
        write_trace_text_node(out, child, &child_prefix, last, false);
    }
}

fn format_trace_flat_csv(tree: &TraceTree) -> String {
    let mut out = String::from("coord,value,source,child_count,formula\n");
    write_trace_csv_row(&mut out, tree);
    out
}

fn write_trace_csv_row(out: &mut String, tree: &TraceTree) {
    let value_str = crate::query::format_scalar(&tree.value);
    let formula = tree.formula.as_deref().unwrap_or("");
    let _ = writeln!(
        out,
        "{},{value_str},{},{},{}",
        tree.coord, tree.source, tree.child_count, formula
    );
    for child in &tree.inputs {
        write_trace_csv_row(out, child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::CellCoordinate;
    use mc_fixtures::build_acme_cube;

    /// Phase 6A.2 item 1.3 (Codex M-5 bonus): the no-trace fallback at
    /// a consolidated coord must label `source: "consolidation"` rather
    /// than the previous `source: "input"`. Q1_2026/Paid_Media/Florida
    /// expands to 27 leaf children.
    #[test]
    fn test_trace_fallback_at_consolidated_coord_labels_consolidation() {
        let (cube, refs) = build_acme_cube().expect("acme cube");
        let coord = CellCoordinate::from_parts(
            cube.id,
            vec![
                refs.scen_baseline,
                refs.ver_working,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.spend,
            ],
        );
        let tree = fallback_leaf_tree(&coord, &mc_core::ScalarValue::Null, &cube);
        assert_eq!(tree.source, "consolidation");
        assert_eq!(tree.child_count, 27);
        assert!(tree.formula.is_none());
        assert!(tree.inputs.is_empty());
    }

    #[test]
    fn test_trace_fallback_at_input_coord_labels_input() {
        let (cube, refs) = build_acme_cube().expect("acme cube");
        let coord = CellCoordinate::from_parts(
            cube.id,
            vec![
                refs.scen_baseline,
                refs.ver_working,
                refs.jan_2026,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            ],
        );
        let tree = fallback_leaf_tree(&coord, &mc_core::ScalarValue::F64(10500.0), &cube);
        assert_eq!(tree.source, "input");
        assert_eq!(tree.child_count, 0);
    }
}
