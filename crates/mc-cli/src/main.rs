//! `mc` — smoke-test CLI for the Acme demo.
//!
//! Per phase-1-rust-kernel-build-brief.md §4.6 / §15 step 19. Runs the
//! Acme demo end-to-end and prints the values the integration test
//! suite asserts on. CI runs the test suite, not the CLI; the CLI is
//! a human-readable smoke check.
//!
//! `cargo run --release --bin mc -- demo`

use mc_core::{CellValue, ScalarValue, TraceNode, TraceOp, WriteIntent, WritebackRequest};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs, AcmeRefs};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("demo");
    match cmd {
        "demo" => run_demo(),
        "--help" | "-h" | "help" => print_help(),
        other => {
            eprintln!("unknown command: {other:?}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("mc — MarketingCubes CLI");
    println!();
    println!("USAGE:");
    println!("    mc demo");
    println!();
    println!("Runs the Acme demo end-to-end (per brief §4.6).");
}

fn run_demo() {
    println!("Building Acme cube...");
    let (mut cube, refs) = build_acme_cube().expect("acme fixture must build");
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
    // 2-decimal currency-style: 11_500.00. The brief uses 7-decimal
    // form for ratio measures (CPC/CVR/Close_Rate/COGS_Rate); call
    // sites pass `ratio=true` for those via `format_value`.
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
    // Trace nodes use amount format throughout per brief §4.6; only the
    // top-level "Reading consolidated cells" CPC line uses the 7-decimal
    // ratio form because we want to expose its tail digits.
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
