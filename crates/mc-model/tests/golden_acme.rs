//! Inline-goldens runner — the 10th row of ADR-0004 Decision 6.
//!
//! Loads `examples/acme.yaml`, compiles to a `Cube`, writes the canonical
//! 2,520 input cells via `mc_fixtures::write_canonical_inputs` (using
//! the YAML-loaded cube's `ModelRefs` to construct an `AcmeRefs`), then
//! reads each `golden_tests:` entry and asserts the cube's value matches
//! the declared expectation.
//!
//! `expect: <f64>` is exact; `expect_within_epsilon: { value, epsilon }`
//! uses `(actual - expected).abs() < epsilon`. Per CLAUDE.md §4.3 the
//! canonical tolerance is `1e-9`.

use mc_core::ScalarValue;
use mc_fixtures::{write_canonical_inputs, AcmeRefs};
use mc_model::load;

#[test]
fn acme_inline_goldens_pass() {
    let compiled = load("examples/acme.yaml").unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("model error: {e}");
        }
        panic!("acme.yaml failed to load");
    });
    let mut cube = compiled.cube;
    let refs = build_acme_refs(&compiled.refs, compiled.root_principal);
    let written = write_canonical_inputs(&mut cube, &refs).expect("write canonical inputs");
    assert_eq!(written, 2520, "canonical input cell count");

    // Re-parse to see the goldens (load() consumed them via validate);
    // include_str lets us run them off the on-disk YAML directly.
    let yaml = include_str!("../examples/acme.yaml");
    let parsed = mc_model::parse(yaml, Some("examples/acme.yaml".into()))
        .expect("acme.yaml parse must succeed");
    assert!(
        !parsed.golden_tests.is_empty(),
        "acme.yaml must declare at least one golden_test"
    );

    for golden in &parsed.golden_tests {
        // Convert BTreeMap<String, String> coord into ElementId slots.
        let coord = compiled
            .refs
            .coord_from_names(&golden.coord)
            .unwrap_or_else(|| panic!("golden {:?}: coord_from_names failed", golden.name));
        let actual = cube
            .read(&coord, refs.root_principal)
            .unwrap_or_else(|e| panic!("golden {:?}: read failed: {e}", golden.name));
        let actual_f = match actual.value {
            ScalarValue::F64(v) => v,
            other => panic!("golden {:?}: expected F64, got {:?}", golden.name, other),
        };
        if let Some(expect) = golden.expect {
            assert!(
                (actual_f - expect).abs() < 1e-9,
                "golden {:?}: actual {actual_f}, expected {expect}",
                golden.name
            );
        } else if let Some(eps) = &golden.expect_within_epsilon {
            assert!(
                (actual_f - eps.value).abs() < eps.epsilon,
                "golden {:?}: actual {actual_f}, expected {} ± {}",
                golden.name,
                eps.value,
                eps.epsilon
            );
        } else {
            panic!(
                "golden {:?}: validator should have caught missing expect/expect_within_epsilon",
                golden.name
            );
        }
    }
}

/// Build an `AcmeRefs` from a `ModelRefs` so the existing canonical
/// input writer in `mc-fixtures` works against the YAML-loaded cube.
fn build_acme_refs(refs: &mc_model::ModelRefs, root_principal: mc_core::PrincipalId) -> AcmeRefs {
    let r = |dim: &str, name: &str| -> mc_core::ElementId {
        refs.element(dim, name)
            .unwrap_or_else(|| panic!("acme.yaml missing element {name:?} in dim {dim:?}"))
    };
    let dim = |name: &str| -> mc_core::DimensionId {
        refs.dimensions
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("acme.yaml missing dim {name:?}"))
    };
    let rule = |name: &str| -> mc_core::RuleId {
        refs.rules
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("acme.yaml missing rule {name:?}"))
    };
    AcmeRefs {
        root_principal,
        scenario_dim: dim("Scenario"),
        version_dim: dim("Version"),
        time_dim: dim("Time"),
        channel_dim: dim("Channel"),
        market_dim: dim("Market"),
        measure_dim: dim("Measure"),
        time_hierarchy: mc_core::HierarchyId(0),
        channel_hierarchy: mc_core::HierarchyId(0),
        market_hierarchy: mc_core::HierarchyId(0),
        scen_baseline: r("Scenario", "Baseline"),
        scen_aggressive: r("Scenario", "Aggressive"),
        scen_conservative: r("Scenario", "Conservative"),
        ver_working: r("Version", "Working"),
        ver_submitted: r("Version", "Submitted"),
        ver_approved: r("Version", "Approved"),
        jan_2026: r("Time", "Jan_2026"),
        feb_2026: r("Time", "Feb_2026"),
        mar_2026: r("Time", "Mar_2026"),
        apr_2026: r("Time", "Apr_2026"),
        may_2026: r("Time", "May_2026"),
        jun_2026: r("Time", "Jun_2026"),
        jul_2026: r("Time", "Jul_2026"),
        aug_2026: r("Time", "Aug_2026"),
        sep_2026: r("Time", "Sep_2026"),
        oct_2026: r("Time", "Oct_2026"),
        nov_2026: r("Time", "Nov_2026"),
        dec_2026: r("Time", "Dec_2026"),
        q1_2026: r("Time", "Q1_2026"),
        q2_2026: r("Time", "Q2_2026"),
        q3_2026: r("Time", "Q3_2026"),
        q4_2026: r("Time", "Q4_2026"),
        fy_2026: r("Time", "FY_2026"),
        paid_search: r("Channel", "Paid_Search"),
        paid_social: r("Channel", "Paid_Social"),
        display: r("Channel", "Display"),
        email: r("Channel", "Email"),
        organic: r("Channel", "Organic"),
        paid_media: r("Channel", "Paid_Media"),
        owned_earned: r("Channel", "Owned_Earned"),
        all_channels: r("Channel", "All_Channels"),
        tampa: r("Market", "Tampa"),
        orlando: r("Market", "Orlando"),
        miami: r("Market", "Miami"),
        atlanta: r("Market", "Atlanta"),
        charlotte: r("Market", "Charlotte"),
        new_york_city: r("Market", "New_York_City"),
        boston: r("Market", "Boston"),
        florida: r("Market", "Florida"),
        georgia: r("Market", "Georgia"),
        north_carolina: r("Market", "North_Carolina"),
        new_york_state: r("Market", "New_York_State"),
        massachusetts: r("Market", "Massachusetts"),
        southeast: r("Market", "Southeast"),
        northeast: r("Market", "Northeast"),
        usa: r("Market", "USA"),
        spend: r("Measure", "Spend"),
        cpc: r("Measure", "CPC"),
        cvr: r("Measure", "CVR"),
        close_rate: r("Measure", "Close_Rate"),
        aov: r("Measure", "AOV"),
        cogs_rate: r("Measure", "COGS_Rate"),
        clicks: r("Measure", "Clicks"),
        leads: r("Measure", "Leads"),
        customers: r("Measure", "Customers"),
        revenue: r("Measure", "Revenue"),
        gross_profit: r("Measure", "Gross_Profit"),
        rule_clicks: rule("rule_clicks"),
        rule_leads: rule("rule_leads"),
        rule_customers: rule("rule_customers"),
        rule_revenue: rule("rule_revenue"),
        rule_gross_profit: rule("rule_gross_profit"),
    }
}
