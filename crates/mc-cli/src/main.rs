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
    // Crude but sufficient arg parser. Phase 3A's only new flag is
    // `--model <path>`; we accept it after the subcommand.
    //   mc demo                       — Rust fixture (unchanged)
    //   mc demo --model <path>        — YAML-loaded cube via mc-model::load
    let mut iter = args.iter().skip(1);
    let cmd = iter
        .next()
        .map(String::as_str)
        .unwrap_or("demo")
        .to_string();
    let mut model_path: Option<String> = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--model" => match iter.next() {
                Some(p) => model_path = Some(p.clone()),
                None => {
                    eprintln!("--model requires a path argument");
                    std::process::exit(1);
                }
            },
            other => {
                eprintln!("unknown argument: {other:?}");
                std::process::exit(1);
            }
        }
    }
    match cmd.as_str() {
        "demo" => run_demo(model_path.as_deref()),
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
    println!("    mc demo                          # Rust fixture (canonical)");
    println!("    mc demo --model <path>           # YAML model via mc-model::load");
    println!();
    println!("Runs the Acme demo end-to-end (per brief §4.6).");
}

fn run_demo(model_path: Option<&str>) {
    println!("Building Acme cube...");
    let (mut cube, refs) = match model_path {
        // Per ADR-0004 + handoff: --model routes through mc_model::load,
        // which goes YAML → ParsedModel → ValidatedModel → Cube. Stdout
        // is byte-for-byte identical to the Rust fixture path.
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

/// Translate a `mc_model::CompiledCube` into the `(Cube, AcmeRefs)`
/// pair the rest of the demo expects. Every named ID on `AcmeRefs` is
/// resolved by name from the YAML-loaded cube's `ModelRefs`. Any name
/// the YAML doesn't carry is a programming error in `acme.yaml` —
/// surface via expect so the CLI fails loudly instead of producing a
/// silently-different output (which would defeat the byte-for-byte gate).
fn load_acme_from_yaml(path: &str) -> (mc_core::Cube, AcmeRefs) {
    let compiled = mc_model::load(path).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("model error: {e}");
        }
        std::process::exit(1);
    });
    let resolve = |dim: &str, name: &str| -> mc_core::ElementId {
        compiled
            .refs
            .element(dim, name)
            .unwrap_or_else(|| panic!("acme.yaml missing element {name:?} in dim {dim:?}"))
    };
    let dim = |name: &str| -> mc_core::DimensionId {
        compiled
            .refs
            .dimensions
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("acme.yaml missing dimension {name:?}"))
    };
    let rule = |name: &str| -> mc_core::RuleId {
        compiled
            .refs
            .rules
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("acme.yaml missing rule {name:?}"))
    };
    let refs = AcmeRefs {
        root_principal: compiled.root_principal,
        scenario_dim: dim("Scenario"),
        version_dim: dim("Version"),
        time_dim: dim("Time"),
        channel_dim: dim("Channel"),
        market_dim: dim("Market"),
        measure_dim: dim("Measure"),
        // Hierarchies aren't named in ModelRefs (the kernel's
        // HierarchyId isn't part of any user-visible flow path); the
        // demo doesn't read these fields. Use HierarchyId(0) as a
        // sentinel so AcmeRefs is constructible.
        time_hierarchy: mc_core::HierarchyId(0),
        channel_hierarchy: mc_core::HierarchyId(0),
        market_hierarchy: mc_core::HierarchyId(0),
        scen_baseline: resolve("Scenario", "Baseline"),
        scen_aggressive: resolve("Scenario", "Aggressive"),
        scen_conservative: resolve("Scenario", "Conservative"),
        ver_working: resolve("Version", "Working"),
        ver_submitted: resolve("Version", "Submitted"),
        ver_approved: resolve("Version", "Approved"),
        jan_2026: resolve("Time", "Jan_2026"),
        feb_2026: resolve("Time", "Feb_2026"),
        mar_2026: resolve("Time", "Mar_2026"),
        apr_2026: resolve("Time", "Apr_2026"),
        may_2026: resolve("Time", "May_2026"),
        jun_2026: resolve("Time", "Jun_2026"),
        jul_2026: resolve("Time", "Jul_2026"),
        aug_2026: resolve("Time", "Aug_2026"),
        sep_2026: resolve("Time", "Sep_2026"),
        oct_2026: resolve("Time", "Oct_2026"),
        nov_2026: resolve("Time", "Nov_2026"),
        dec_2026: resolve("Time", "Dec_2026"),
        q1_2026: resolve("Time", "Q1_2026"),
        q2_2026: resolve("Time", "Q2_2026"),
        q3_2026: resolve("Time", "Q3_2026"),
        q4_2026: resolve("Time", "Q4_2026"),
        fy_2026: resolve("Time", "FY_2026"),
        paid_search: resolve("Channel", "Paid_Search"),
        paid_social: resolve("Channel", "Paid_Social"),
        display: resolve("Channel", "Display"),
        email: resolve("Channel", "Email"),
        organic: resolve("Channel", "Organic"),
        paid_media: resolve("Channel", "Paid_Media"),
        owned_earned: resolve("Channel", "Owned_Earned"),
        all_channels: resolve("Channel", "All_Channels"),
        tampa: resolve("Market", "Tampa"),
        orlando: resolve("Market", "Orlando"),
        miami: resolve("Market", "Miami"),
        atlanta: resolve("Market", "Atlanta"),
        charlotte: resolve("Market", "Charlotte"),
        new_york_city: resolve("Market", "New_York_City"),
        boston: resolve("Market", "Boston"),
        florida: resolve("Market", "Florida"),
        georgia: resolve("Market", "Georgia"),
        north_carolina: resolve("Market", "North_Carolina"),
        new_york_state: resolve("Market", "New_York_State"),
        massachusetts: resolve("Market", "Massachusetts"),
        southeast: resolve("Market", "Southeast"),
        northeast: resolve("Market", "Northeast"),
        usa: resolve("Market", "USA"),
        spend: resolve("Measure", "Spend"),
        cpc: resolve("Measure", "CPC"),
        cvr: resolve("Measure", "CVR"),
        close_rate: resolve("Measure", "Close_Rate"),
        aov: resolve("Measure", "AOV"),
        cogs_rate: resolve("Measure", "COGS_Rate"),
        clicks: resolve("Measure", "Clicks"),
        leads: resolve("Measure", "Leads"),
        customers: resolve("Measure", "Customers"),
        revenue: resolve("Measure", "Revenue"),
        gross_profit: resolve("Measure", "Gross_Profit"),
        rule_clicks: rule("rule_clicks"),
        rule_leads: rule("rule_leads"),
        rule_customers: rule("rule_customers"),
        rule_revenue: rule("rule_revenue"),
        rule_gross_profit: rule("rule_gross_profit"),
    };
    (compiled.cube, refs)
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
