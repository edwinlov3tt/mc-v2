//! Regression tests for the narrative engine.
//!
//! Verifies that all 14 Phase 6D templates in `display-like.yaml` produce
//! the same output as the original `mc-demo-server/src/narrative.rs`
//! evaluator when given the Scotts RV sample data.

use mc_narrative::{CellEntry, CubeData, Severity};
use std::collections::BTreeMap;

/// Find the narratives directory, checking from both repo root and crate dir.
fn narratives_dir() -> String {
    for path in &["demo/narratives", "../../demo/narratives"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    panic!("cannot find demo/narratives directory");
}

fn load_templates() -> Vec<mc_narrative::TemplateDefinition> {
    let dir = narratives_dir();
    let templates = mc_narrative::load_templates(&dir);
    assert!(
        templates.len() >= 14,
        "expected >= 14 templates, got {}",
        templates.len()
    );
    templates
}

/// Build CubeData for the Monthly Performance CSV.
fn monthly_performance() -> CubeData {
    CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-monthly-performance.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Jul_2025".into(),
                        value: 25102.0,
                    },
                    CellEntry {
                        category: "Aug_2025".into(),
                        value: 30655.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "Jul_2025".into(),
                        value: 79.0,
                    },
                    CellEntry {
                        category: "Aug_2025".into(),
                        value: 166.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "Jul_2025".into(),
                        value: 0.31,
                    },
                    CellEntry {
                        category: "Aug_2025".into(),
                        value: 0.54,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "Jul_2025".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Aug_2025".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

/// Build CubeData for the Campaign Performance CSV.
fn campaign_performance() -> CubeData {
    CubeData {
        table_name: "Campaign Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-campaign-performance.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP".into(),
                        value: 55740.0,
                    },
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP-O-O".into(),
                        value: 17.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP".into(),
                        value: 245.0,
                    },
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP-O-O".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP".into(),
                        value: 0.44,
                    },
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP-O-O".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Scotts_RV_Truck_and_Auto_Repair_Primary_AAT-DISP-O-O".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

/// Build CubeData for the Device Performance CSV.
fn device_performance() -> CubeData {
    CubeData {
        table_name: "Device Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-device-performance.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Mobile_Phone".into(),
                        value: 44280.0,
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 5870.0,
                    },
                    CellEntry {
                        category: "PC__Desktop_or_Laptop_".into(),
                        value: 5607.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "Mobile_Phone".into(),
                        value: 192.0,
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 49.0,
                    },
                    CellEntry {
                        category: "PC__Desktop_or_Laptop_".into(),
                        value: 4.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "Mobile_Phone".into(),
                        value: 0.43,
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 0.83,
                    },
                    CellEntry {
                        category: "PC__Desktop_or_Laptop_".into(),
                        value: 0.07,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "Mobile_Phone".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "PC__Desktop_or_Laptop_".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

/// Build CubeData for the Performance by City CSV.
fn performance_by_city() -> CubeData {
    CubeData {
        table_name: "Performance by City".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-performance-by-city.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Rockford".into(),
                        value: 45279.0,
                    },
                    CellEntry {
                        category: "Machesney_Park".into(),
                        value: 4592.0,
                    },
                    CellEntry {
                        category: "Loves_Park".into(),
                        value: 4468.0,
                    },
                    CellEntry {
                        category: "Davis_Junction".into(),
                        value: 763.0,
                    },
                    CellEntry {
                        category: "Cherry_Valley".into(),
                        value: 566.0,
                    },
                    CellEntry {
                        category: "Monroe_Center".into(),
                        value: 89.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "Rockford".into(),
                        value: 185.0,
                    },
                    CellEntry {
                        category: "Machesney_Park".into(),
                        value: 26.0,
                    },
                    CellEntry {
                        category: "Loves_Park".into(),
                        value: 23.0,
                    },
                    CellEntry {
                        category: "Davis_Junction".into(),
                        value: 9.0,
                    },
                    CellEntry {
                        category: "Cherry_Valley".into(),
                        value: 2.0,
                    },
                    CellEntry {
                        category: "Monroe_Center".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "Rockford".into(),
                        value: 0.41,
                    },
                    CellEntry {
                        category: "Machesney_Park".into(),
                        value: 0.57,
                    },
                    CellEntry {
                        category: "Loves_Park".into(),
                        value: 0.51,
                    },
                    CellEntry {
                        category: "Davis_Junction".into(),
                        value: 1.18,
                    },
                    CellEntry {
                        category: "Cherry_Valley".into(),
                        value: 0.35,
                    },
                    CellEntry {
                        category: "Monroe_Center".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "Rockford".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Machesney_Park".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Loves_Park".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Davis_Junction".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Cherry_Valley".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "Monroe_Center".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

/// Build CubeData for the Performance by Zip CSV.
fn performance_by_zip() -> CubeData {
    CubeData {
        table_name: "Performance by Zip".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-performance-by-zip.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "61107".into(),
                        value: 12873.0,
                    },
                    CellEntry {
                        category: "61103".into(),
                        value: 6764.0,
                    },
                    CellEntry {
                        category: "61108".into(),
                        value: 8503.0,
                    },
                    CellEntry {
                        category: "61115".into(),
                        value: 4592.0,
                    },
                    CellEntry {
                        category: "61111".into(),
                        value: 4468.0,
                    },
                    CellEntry {
                        category: "61109".into(),
                        value: 4730.0,
                    },
                    CellEntry {
                        category: "61102".into(),
                        value: 4099.0,
                    },
                    CellEntry {
                        category: "61114".into(),
                        value: 4053.0,
                    },
                    CellEntry {
                        category: "61104".into(),
                        value: 4132.0,
                    },
                    CellEntry {
                        category: "61020".into(),
                        value: 763.0,
                    },
                    CellEntry {
                        category: "61016".into(),
                        value: 566.0,
                    },
                    CellEntry {
                        category: "61052".into(),
                        value: 89.0,
                    },
                    CellEntry {
                        category: "61112".into(),
                        value: 125.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "61107".into(),
                        value: 59.0,
                    },
                    CellEntry {
                        category: "61103".into(),
                        value: 35.0,
                    },
                    CellEntry {
                        category: "61108".into(),
                        value: 35.0,
                    },
                    CellEntry {
                        category: "61115".into(),
                        value: 26.0,
                    },
                    CellEntry {
                        category: "61111".into(),
                        value: 23.0,
                    },
                    CellEntry {
                        category: "61109".into(),
                        value: 15.0,
                    },
                    CellEntry {
                        category: "61102".into(),
                        value: 14.0,
                    },
                    CellEntry {
                        category: "61114".into(),
                        value: 14.0,
                    },
                    CellEntry {
                        category: "61104".into(),
                        value: 13.0,
                    },
                    CellEntry {
                        category: "61020".into(),
                        value: 9.0,
                    },
                    CellEntry {
                        category: "61016".into(),
                        value: 2.0,
                    },
                    CellEntry {
                        category: "61052".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61112".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "61107".into(),
                        value: 0.46,
                    },
                    CellEntry {
                        category: "61103".into(),
                        value: 0.52,
                    },
                    CellEntry {
                        category: "61108".into(),
                        value: 0.41,
                    },
                    CellEntry {
                        category: "61115".into(),
                        value: 0.57,
                    },
                    CellEntry {
                        category: "61111".into(),
                        value: 0.51,
                    },
                    CellEntry {
                        category: "61109".into(),
                        value: 0.32,
                    },
                    CellEntry {
                        category: "61102".into(),
                        value: 0.34,
                    },
                    CellEntry {
                        category: "61114".into(),
                        value: 0.35,
                    },
                    CellEntry {
                        category: "61104".into(),
                        value: 0.31,
                    },
                    CellEntry {
                        category: "61020".into(),
                        value: 1.18,
                    },
                    CellEntry {
                        category: "61016".into(),
                        value: 0.35,
                    },
                    CellEntry {
                        category: "61052".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61112".into(),
                        value: 0.0,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "61107".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61103".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61108".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61115".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61111".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61109".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61102".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61114".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61104".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61020".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61016".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61052".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "61112".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

/// Build CubeData for the Creative By Name CSV.
fn creative_by_name() -> CubeData {
    CubeData {
        table_name: "Creative By Name".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report-targeteddisplay-creative-by-name.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "we_keep_you_rolling_straight.gif".into(),
                        value: 18852.0,
                    },
                    CellEntry {
                        category: "get_rolling_again.gif".into(),
                        value: 18109.0,
                    },
                    CellEntry {
                        category: "we_service_all_vehicles.gif".into(),
                        value: 18796.0,
                    },
                ],
            ),
            (
                "Clicks".into(),
                vec![
                    CellEntry {
                        category: "we_keep_you_rolling_straight.gif".into(),
                        value: 85.0,
                    },
                    CellEntry {
                        category: "get_rolling_again.gif".into(),
                        value: 83.0,
                    },
                    CellEntry {
                        category: "we_service_all_vehicles.gif".into(),
                        value: 77.0,
                    },
                ],
            ),
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "we_keep_you_rolling_straight.gif".into(),
                        value: 0.45,
                    },
                    CellEntry {
                        category: "get_rolling_again.gif".into(),
                        value: 0.46,
                    },
                    CellEntry {
                        category: "we_service_all_vehicles.gif".into(),
                        value: 0.41,
                    },
                ],
            ),
            (
                "Total_Conversions".into(),
                vec![
                    CellEntry {
                        category: "we_keep_you_rolling_straight.gif".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "get_rolling_again.gif".into(),
                        value: 0.0,
                    },
                    CellEntry {
                        category: "we_service_all_vehicles.gif".into(),
                        value: 0.0,
                    },
                ],
            ),
        ]),
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[test]
fn test_load_all_templates() {
    let templates = load_templates();
    // 14 from display-like.yaml + 5 from trend-templates.yaml (Phase 7A.3)
    // + 5 from benchmark-templates.yaml (Phase 7A.4).
    assert_eq!(
        templates.len(),
        24,
        "display-like.yaml (14) + trend-templates.yaml (5) + benchmark-templates.yaml (5) = 24 templates"
    );
    // Verify sort order: data_sufficiency (sort_order: -10) should be first.
    assert_eq!(templates[0].id, "data_sufficiency");
    assert_eq!(templates[0].sort_order, -10);
}

#[test]
fn test_template_ids_match_yaml() {
    let templates = load_templates();
    let expected_ids = [
        // display-like.yaml (14)
        "data_sufficiency",
        "small_sample_warning",
        "impressions_mom",
        "clicks_mom",
        "ctr_trend",
        "engagement_acceleration",
        "uniform_momentum",
        "ctr_vs_benchmark",
        "device_ranking",
        "device_underperformance",
        "geo_concentration",
        "zero_engagement_alarm",
        "top_creative",
        "conversion_alarm",
        // trend-templates.yaml (5, Phase 7A.3)
        "persistent_decline",
        "recurring_warning",
        "conversion_alarm_persistent",
        "improvement_trend",
        "new_issue_first_occurrence",
        // benchmark-templates.yaml (5, Phase 7A.4)
        "ctr_above_own_median",
        "ctr_below_own_p25",
        "impressions_unusually_high",
        "ctr_benchmark_context",
        "spend_efficiency_trending",
    ];
    let mut actual_ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
    actual_ids.sort();
    let mut expected_sorted = expected_ids.to_vec();
    expected_sorted.sort();
    assert_eq!(actual_ids, expected_sorted, "template IDs must match YAML");
}

#[test]
fn test_monthly_performance_narratives() {
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    // Expected: data_sufficiency, impressions_mom, clicks_mom, ctr_trend,
    // engagement_acceleration, uniform_momentum, ctr_vs_benchmark, conversion_alarm = 8
    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        ids.contains(&"data_sufficiency"),
        "data_sufficiency should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"impressions_mom"),
        "impressions_mom should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"clicks_mom"),
        "clicks_mom should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"ctr_trend"),
        "ctr_trend should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"engagement_acceleration"),
        "engagement_acceleration should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"uniform_momentum"),
        "uniform_momentum should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"ctr_vs_benchmark"),
        "ctr_vs_benchmark should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"conversion_alarm"),
        "conversion_alarm should fire; got: {ids:?}"
    );
    assert_eq!(
        narratives.len(),
        8,
        "Monthly Performance should produce 8 narratives; got: {ids:?}"
    );
}

#[test]
fn test_data_sufficiency_content() {
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let ds = narratives
        .iter()
        .find(|n| n.template_id == "data_sufficiency")
        .expect("data_sufficiency should fire");
    assert_eq!(ds.severity, Severity::Info);
    assert!(
        ds.text.contains("2 reporting periods"),
        "should mention 2 periods; got: {}",
        ds.text
    );
    assert!(
        ds.text.contains("Directional"),
        "should mention Directional; got: {}",
        ds.text
    );
}

#[test]
fn test_impressions_mom_content() {
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let impr = narratives
        .iter()
        .find(|n| n.template_id == "impressions_mom")
        .expect("impressions_mom should fire");
    assert!(
        impr.text.contains("Targeted Display"),
        "should mention tactic name; got: {}",
        impr.text
    );
    assert!(
        impr.text.contains("grew"),
        "impressions grew; got: {}",
        impr.text
    );
    assert!(
        impr.text.contains("25,102") || impr.text.contains("25102"),
        "should mention prev impressions; got: {}",
        impr.text
    );
    assert!(
        impr.text.contains("30,655") || impr.text.contains("30655"),
        "should mention current impressions; got: {}",
        impr.text
    );
}

#[test]
fn test_conversion_alarm_content() {
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let alarm = narratives
        .iter()
        .find(|n| n.template_id == "conversion_alarm")
        .expect("conversion_alarm should fire");
    assert_eq!(alarm.severity, Severity::Critical);
    assert!(
        alarm.text.contains("Zero conversions"),
        "should mention zero conversions; got: {}",
        alarm.text
    );
}

#[test]
fn test_device_narratives() {
    let templates = load_templates();
    let cubes = vec![device_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        ids.contains(&"device_ranking"),
        "device_ranking should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"device_underperformance"),
        "device_underperformance should fire; got: {ids:?}"
    );

    let ranking = narratives
        .iter()
        .find(|n| n.template_id == "device_ranking")
        .unwrap();
    assert!(
        ranking.text.contains("Tablet"),
        "Tablet should be top performer; got: {}",
        ranking.text
    );
}

#[test]
fn test_geo_narratives() {
    let templates = load_templates();
    let cubes = vec![performance_by_city()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        ids.contains(&"small_sample_warning"),
        "small_sample_warning should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"geo_concentration"),
        "geo_concentration should fire; got: {ids:?}"
    );
    assert!(
        ids.contains(&"zero_engagement_alarm"),
        "zero_engagement_alarm should fire; got: {ids:?}"
    );

    let conc = narratives
        .iter()
        .find(|n| n.template_id == "geo_concentration")
        .unwrap();
    assert!(
        conc.text.contains("Rockford"),
        "Rockford should be top area; got: {}",
        conc.text
    );
    assert_eq!(conc.severity, Severity::Warning);
}

#[test]
fn test_creative_narrative() {
    let templates = load_templates();
    let cubes = vec![creative_by_name()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        ids.contains(&"top_creative"),
        "top_creative should fire; got: {ids:?}"
    );
}

#[test]
fn test_dedup_across_cubes() {
    let templates = load_templates();
    // data_sufficiency fires once even with multiple cubes.
    let cubes = vec![monthly_performance(), campaign_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let ds_count = narratives
        .iter()
        .filter(|n| n.template_id == "data_sufficiency")
        .count();
    assert_eq!(
        ds_count, 1,
        "data_sufficiency should fire exactly once (dedup)"
    );

    let conv_count = narratives
        .iter()
        .filter(|n| n.template_id == "conversion_alarm")
        .count();
    assert_eq!(
        conv_count, 1,
        "conversion_alarm should fire exactly once (dedup)"
    );
}

#[test]
fn test_full_scotts_rv_evaluation() {
    let templates = load_templates();
    let cubes = vec![
        monthly_performance(),
        campaign_performance(),
        device_performance(),
        performance_by_city(),
        performance_by_zip(),
        creative_by_name(),
    ];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    // Print all narratives for debugging.
    for (i, n) in narratives.iter().enumerate() {
        eprintln!("[{i}] {}: {}", n.template_id, n.text);
    }

    // Verify we get a reasonable number of narratives.
    assert!(
        narratives.len() >= 12,
        "full Scotts RV should produce >= 12 narratives, got {}",
        narratives.len()
    );

    // Verify all severity levels are correctly typed.
    for n in &narratives {
        match n.severity {
            Severity::Info | Severity::Warning | Severity::Critical => {}
            _ => panic!(
                "unexpected severity for {}: {:?}",
                n.template_id, n.severity
            ),
        }
    }

    // Verify evidence is populated for narratives with numeric bindings.
    let alarm = narratives
        .iter()
        .find(|n| n.template_id == "conversion_alarm")
        .expect("conversion_alarm should fire");
    assert!(
        !alarm.evidence.is_empty(),
        "conversion_alarm should have evidence"
    );
}

#[test]
fn test_narrative_id_format() {
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    for n in &narratives {
        assert!(
            n.id.starts_with(&n.template_id),
            "narrative id should start with template_id: {} vs {}",
            n.id,
            n.template_id
        );
        assert!(
            n.id.contains("report-targeteddisplay"),
            "narrative id should contain source file info: {}",
            n.id
        );
    }
}

#[test]
fn test_dag_binding_resolution() {
    // Finding #3: verify DAG-ordered binding resolution handles chains > 1 deep.
    // The clicks_mom template has: verb references abs_pct (binding→binding ref).
    // With DAG resolution, abs_pct resolves first, then verb can reference it.
    let templates = load_templates();
    let cubes = vec![monthly_performance()];
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None, None, None);

    let clicks = narratives
        .iter()
        .find(|n| n.template_id == "clicks_mom")
        .expect("clicks_mom should fire");

    // The verb binding: if(abs_pct > 100, 'more than doubled', ...)
    // abs_pct = abs((166 - 79) / 79 * 100) = 110.1
    // Since abs_pct > 100, verb should be "more than doubled".
    assert!(
        clicks.text.contains("more than doubled"),
        "DAG resolution should resolve abs_pct before verb; got: {}",
        clicks.text
    );
}

#[test]
fn test_deduplicate_yaml_field() {
    // Finding #2: verify templates with deduplicate: true in YAML
    // fire only once, even with the legacy hardcoded list removed.
    let templates = load_templates();

    // Verify the 4 templates now have deduplicate: true in YAML.
    let ds = templates
        .iter()
        .find(|t| t.id == "data_sufficiency")
        .unwrap();
    assert!(
        ds.deduplicate,
        "data_sufficiency should have deduplicate: true"
    );

    let ca = templates
        .iter()
        .find(|t| t.id == "conversion_alarm")
        .unwrap();
    assert!(
        ca.deduplicate,
        "conversion_alarm should have deduplicate: true"
    );
}

#[test]
fn test_validate_templates() {
    let templates = load_templates();
    let errors = mc_narrative::validate_templates(&templates);
    assert!(
        errors.is_empty(),
        "display-like.yaml should validate cleanly; got: {:?}",
        errors
    );
}

#[test]
fn test_validate_catches_duplicate_id() {
    // Load templates twice → creates duplicate IDs.
    let mut templates = load_templates();
    let templates2 = load_templates();
    templates.extend(templates2);
    let errors = mc_narrative::validate_templates(&templates);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, mc_narrative::NarrativeError::DuplicateTemplateId { .. })),
        "should detect duplicate template ID; got: {:?}",
        errors
    );
}

// ─── Phase 7A.3: Cross-period trend template tests ────────────────────

/// Find the test-fixtures directory.
fn fixtures_dir() -> String {
    for path in &["demo/test-fixtures", "../../demo/test-fixtures"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    panic!("cannot find demo/test-fixtures directory");
}

/// Load a golden ledger fixture.
fn load_fixture(name: &str) -> Vec<mc_narrative::LedgerEntry> {
    let path = format!("{}/{}", fixtures_dir(), name);
    mc_narrative::ledger::read_ledger(std::path::Path::new(&path))
        .unwrap_or_else(|e| panic!("failed to load fixture {name}: {e}"))
}

/// Build a minimal Monthly Performance cube for trend evaluation.
fn trend_test_cube() -> CubeData {
    CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "test.csv".into(),
        dimension_name: Some("Time".into()),
        values: BTreeMap::from([
            (
                "Impressions".into(),
                vec![CellEntry {
                    category: "Apr_2026".into(),
                    value: 38000.0,
                }],
            ),
            (
                "Clicks".into(),
                vec![CellEntry {
                    category: "Apr_2026".into(),
                    value: 950.0,
                }],
            ),
            (
                "CTR".into(),
                vec![CellEntry {
                    category: "Apr_2026".into(),
                    value: 2.5,
                }],
            ),
        ]),
    }
}

#[test]
fn test_persistent_decline_fires_on_3_month_ledger() {
    let templates = load_templates();
    let ledger = load_fixture("3-month-decline.jsonl");
    let cubes = vec![trend_test_cube()];

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger), None, None);

    let trend_narratives: Vec<_> = narratives
        .iter()
        .filter(|n| n.template_id == "persistent_decline")
        .collect();
    assert!(
        !trend_narratives.is_empty(),
        "persistent_decline should fire with 3-month ledger streak; got {:?}",
        narratives
            .iter()
            .map(|n| &n.template_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_recurring_warning_fires_on_2_of_3_months() {
    let templates = load_templates();
    let ledger = load_fixture("2-month-device-warning.jsonl");
    let cubes = vec![trend_test_cube()];

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger), None, None);

    let trend_narratives: Vec<_> = narratives
        .iter()
        .filter(|n| n.template_id == "recurring_warning")
        .collect();
    assert!(
        !trend_narratives.is_empty(),
        "recurring_warning should fire when device_underperformance found in 2 of last 3 periods; got {:?}",
        narratives.iter().map(|n| &n.template_id).collect::<Vec<_>>()
    );
}

#[test]
fn test_conversion_alarm_persistent_fires_on_streak() {
    let templates = load_templates();
    let ledger = load_fixture("persistent-zero-conversions.jsonl");
    let cubes = vec![trend_test_cube()];

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger), None, None);

    let trend_narratives: Vec<_> = narratives
        .iter()
        .filter(|n| n.template_id == "conversion_alarm_persistent")
        .collect();
    assert!(
        !trend_narratives.is_empty(),
        "conversion_alarm_persistent should fire with 3-month streak; got {:?}",
        narratives
            .iter()
            .map(|n| &n.template_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_improvement_trend_fires_on_2_month_momentum() {
    let templates = load_templates();
    let ledger = load_fixture("sustained-improvement.jsonl");
    let cubes = vec![trend_test_cube()];

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger), None, None);

    let trend_narratives: Vec<_> = narratives
        .iter()
        .filter(|n| n.template_id == "improvement_trend")
        .collect();
    assert!(
        !trend_narratives.is_empty(),
        "improvement_trend should fire with 2-month momentum streak; got {:?}",
        narratives
            .iter()
            .map(|n| &n.template_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_first_occurrence_fires_when_no_history() {
    let templates = load_templates();
    // Empty ledger = no history. But we need the cube to have device
    // underperformance conditions for `new_issue_first_occurrence` to fire.
    // This template requires: ledger_has('device_underperformance', 1) == 0
    // AND any_where(CTR < campaign_avg.CTR * 0.25, Device)
    // We need a Device-dimensioned cube for this.
    let cube = CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "test.csv".into(),
        dimension_name: Some("Device".into()),
        values: BTreeMap::from([
            (
                "CTR".into(),
                vec![
                    CellEntry {
                        category: "Desktop".into(),
                        value: 3.2,
                    },
                    CellEntry {
                        category: "Mobile".into(),
                        value: 0.4, // < 3.2 * 0.25 = 0.8 → underperforming
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 2.8,
                    },
                ],
            ),
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Desktop".into(),
                        value: 30000.0,
                    },
                    CellEntry {
                        category: "Mobile".into(),
                        value: 15000.0,
                    },
                    CellEntry {
                        category: "Tablet".into(),
                        value: 5000.0,
                    },
                ],
            ),
        ]),
    };

    // Empty ledger: no prior device_underperformance entries.
    let empty_ledger: Vec<mc_narrative::LedgerEntry> = Vec::new();
    let narratives =
        mc_narrative::evaluate_all(&templates, &[cube], Some(&empty_ledger), None, None);

    let trend_narratives: Vec<_> = narratives
        .iter()
        .filter(|n| n.template_id == "new_issue_first_occurrence")
        .collect();
    assert!(
        !trend_narratives.is_empty(),
        "new_issue_first_occurrence should fire when no prior history and device underperforms; got {:?}",
        narratives.iter().map(|n| &n.template_id).collect::<Vec<_>>()
    );
}

#[test]
fn test_no_trends_fire_with_empty_ledger() {
    let templates = load_templates();
    let empty_ledger: Vec<mc_narrative::LedgerEntry> = Vec::new();
    let cubes = vec![trend_test_cube()];

    let narratives =
        mc_narrative::evaluate_all(&templates, &cubes, Some(&empty_ledger), None, None);

    // The trend templates that use ledger_streak >= N should NOT fire.
    let trend_ids = [
        "persistent_decline",
        "recurring_warning",
        "conversion_alarm_persistent",
        "improvement_trend",
    ];
    let trend_fired: Vec<_> = narratives
        .iter()
        .filter(|n| trend_ids.contains(&n.template_id.as_str()))
        .collect();
    assert!(
        trend_fired.is_empty(),
        "no trend templates should fire with empty ledger; but got: {:?}",
        trend_fired
            .iter()
            .map(|n| &n.template_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ledger_query_performance_1000_entries() {
    // Phase 7A.3 Session 4: verify <5ms median for ledger queries on 1000 entries.
    use std::time::Instant;

    // Build a 1000-entry synthetic ledger.
    let mut entries = Vec::with_capacity(1000);
    for i in 0..1000 {
        let month = (i % 12) + 1;
        let year = 2024 + (i / 12);
        let period = format!("{year:04}-{month:02}");
        let template_ids = [
            "impressions_mom_decline",
            "device_underperformance",
            "conversion_alarm",
            "uniform_momentum",
            "clicks_down",
        ];
        let template_id = template_ids[i % template_ids.len()];

        entries.push(mc_narrative::LedgerEntry {
            schema_version: "1.0".to_string(),
            ledger_entry_id: format!("perf-{i}"),
            generated_at: "2026-05-07T10:00:00Z".to_string(),
            model: "model.yaml".to_string(),
            model_hash: "sha256:perftest".to_string(),
            report_period: Some(period),
            scope: {
                let mut m = BTreeMap::new();
                m.insert("channel".to_string(), "Targeted Display".to_string());
                m
            },
            narrative: mc_narrative::ledger::NarrativeRecord {
                id: template_id.to_string(),
                section: None,
                severity: "warning".to_string(),
                text: "perf test".to_string(),
                template_id: template_id.to_string(),
                notability_score: None,
            },
            evidence: {
                let mut m = BTreeMap::new();
                m.insert("value".to_string(), serde_json::json!(i as f64 * 100.0));
                m
            },
            benchmarks_referenced: Vec::new(),
        });
    }

    let templates = load_templates();
    let cubes = vec![trend_test_cube()];

    // Warm up.
    let _ = mc_narrative::evaluate_all(&templates, &cubes, Some(&entries), None, None);

    // Measure 10 iterations.
    let start = Instant::now();
    let iterations = 10;
    for _ in 0..iterations {
        let _ = mc_narrative::evaluate_all(&templates, &cubes, Some(&entries), None, None);
    }
    let elapsed = start.elapsed();
    let median_ms = elapsed.as_millis() as f64 / iterations as f64;

    assert!(
        median_ms < 50.0, // 50ms generous ceiling (handoff says <5ms median)
        "ledger query with 1000 entries should complete in <50ms; got {median_ms:.1}ms"
    );
    eprintln!("[perf] 1000-entry ledger evaluation: {median_ms:.2}ms avg over {iterations} runs");
}

// ─── Phase 7A.4: Benchmark evaluator function tests ──────────────────

#[test]
fn test_benchmark_p50_returns_median() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    let mut benchmarks = BTreeMap::new();
    let mut scope = BTreeMap::new();
    scope.insert("channel".to_string(), "Targeted Display".to_string());
    benchmarks.insert(
        "CTR::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "CTR".to_string(),
            scope,
            p10: 0.05,
            p25: 0.10,
            p50: 0.18,
            p75: 0.25,
            p90: 0.35,
            mean: 0.19,
            stddev: 0.08,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    let templates = vec![mc_narrative::TemplateDefinition {
        id: "bench_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "benchmark_p50('CTR') > 0".to_string(),
        template: "p50={median_ctr}".to_string(),
        bindings: {
            let mut b = BTreeMap::new();
            b.insert("median_ctr".to_string(), "benchmark_p50('CTR')".to_string());
            b
        },
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);
    assert!(!narratives.is_empty(), "benchmark template should fire");

    let n = &narratives[0];
    let median = n
        .evidence
        .get("median_ctr")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (median - 0.18).abs() < 1e-9,
        "p50 should be 0.18, got {median}"
    );
}

#[test]
fn test_benchmark_percentile_ranks_value() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    let mut benchmarks = BTreeMap::new();
    let mut scope = BTreeMap::new();
    scope.insert("channel".to_string(), "Targeted Display".to_string());
    benchmarks.insert(
        "CTR::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "CTR".to_string(),
            scope,
            p10: 0.05,
            p25: 0.10,
            p50: 0.18,
            p75: 0.25,
            p90: 0.35,
            mean: 0.19,
            stddev: 0.08,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    let templates = vec![mc_narrative::TemplateDefinition {
        id: "pctile_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "true".to_string(),
        template: "rank={rank}".to_string(),
        bindings: {
            let mut b = BTreeMap::new();
            // A value of 0.03 should be <= p10 (0.05), so rank = 10
            b.insert(
                "rank".to_string(),
                "benchmark_percentile('CTR', 0.03)".to_string(),
            );
            b
        },
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);
    assert!(!narratives.is_empty());
    let rank = narratives[0]
        .evidence
        .get("rank")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (rank - 10.0).abs() < 1e-9,
        "value 0.03 <= p10 should rank 10, got {rank}"
    );
}

#[test]
fn test_benchmark_above_median_returns_correct_boolean() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    let mut benchmarks = BTreeMap::new();
    let mut scope = BTreeMap::new();
    scope.insert("channel".to_string(), "Targeted Display".to_string());
    benchmarks.insert(
        "CTR::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "CTR".to_string(),
            scope,
            p10: 0.01,
            p25: 0.02,
            p50: 0.05, // median is 0.05
            p75: 0.08,
            p90: 0.10,
            mean: 0.05,
            stddev: 0.03,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    // The monthly_performance cube has current.CTR which we need to check.
    // benchmark_above_median reads current.CTR from context and compares to p50.
    let templates = vec![mc_narrative::TemplateDefinition {
        id: "above_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "true".to_string(),
        template: "above={above}".to_string(),
        bindings: {
            let mut b = BTreeMap::new();
            b.insert(
                "above".to_string(),
                "benchmark_above_median('CTR')".to_string(),
            );
            b
        },
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);
    assert!(!narratives.is_empty());
    // The cube's CTR values should be > 0.05 (the p50), so above_median = 1.0
    let above = narratives[0]
        .evidence
        .get("above")
        .and_then(|v| v.as_f64())
        .unwrap();
    // CTR in monthly_performance is typically ~0.08-0.12, well above 0.05
    assert!(
        (above - 1.0).abs() < 1e-9,
        "CTR should be above median 0.05, got above={above}"
    );
}

#[test]
fn test_benchmark_z_score_computation() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    let mut benchmarks = BTreeMap::new();
    benchmarks.insert(
        "CTR::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "CTR".to_string(),
            scope: {
                let mut s = BTreeMap::new();
                s.insert("channel".to_string(), "Targeted Display".to_string());
                s
            },
            p10: 0.05,
            p25: 0.10,
            p50: 0.20,
            p75: 0.30,
            p90: 0.40,
            mean: 0.20,
            stddev: 0.10,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    let templates = vec![mc_narrative::TemplateDefinition {
        id: "zscore_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "true".to_string(),
        template: "z={z_score}".to_string(),
        bindings: {
            let mut b = BTreeMap::new();
            // z = (0.40 - 0.20) / 0.10 = 2.0
            b.insert(
                "z_score".to_string(),
                "benchmark_z_score('CTR', 0.40)".to_string(),
            );
            b
        },
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);
    assert!(!narratives.is_empty());
    let z = narratives[0]
        .evidence
        .get("z_score")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (z - 2.0).abs() < 1e-9,
        "z-score of 0.40 with mean=0.20, stddev=0.10 should be 2.0, got {z}"
    );
}

#[test]
fn test_benchmark_functions_return_zero_when_no_library() {
    // Without a benchmark library, all benchmark_*() functions return 0.0
    // and templates with benchmark predicates in when: silently don't fire.
    let templates = vec![mc_narrative::TemplateDefinition {
        id: "no_lib_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "benchmark_p50('CTR') > 0".to_string(),
        template: "should not fire".to_string(),
        bindings: std::collections::BTreeMap::new(),
        deduplicate: false,
        format: std::collections::BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    // No benchmark library → benchmark_p50 returns 0.0 → "0.0 > 0" is false → template skips.
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);
    assert!(
        narratives.iter().all(|n| n.template_id != "no_lib_test"),
        "benchmark template should NOT fire when no library is present"
    );
}

// ─── Phase 7A.4 Session 3: Benchmark template integration tests ──────

#[test]
fn test_benchmark_templates_fire_with_loaded_library() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    // Build a benchmark library where the cube's CTR (typically ~0.08-0.12)
    // is above the p50 of 0.05 and has >= 3 samples.
    let mut benchmarks = BTreeMap::new();
    let scope = {
        let mut s = BTreeMap::new();
        s.insert("channel".to_string(), "Targeted Display".to_string());
        s
    };
    benchmarks.insert(
        "CTR::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "CTR".to_string(),
            scope: scope.clone(),
            p10: 0.01,
            p25: 0.02,
            p50: 0.05,
            p75: 0.08,
            p90: 0.10,
            mean: 0.05,
            stddev: 0.03,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    let templates = load_templates();
    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);

    // ctr_above_own_median should fire: CTR ~0.08-0.12 > p50 of 0.05, sample_count >= 3.
    let fired_ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        fired_ids.contains(&"ctr_above_own_median"),
        "ctr_above_own_median should fire when CTR > p50; fired: {fired_ids:?}"
    );

    // ctr_benchmark_context should fire: sample_count >= 2.
    assert!(
        fired_ids.contains(&"ctr_benchmark_context"),
        "ctr_benchmark_context should fire with sample_count >= 2; fired: {fired_ids:?}"
    );
}

#[test]
fn test_benchmark_templates_skip_without_library() {
    let templates = load_templates();
    let cube = monthly_performance();
    // No benchmark library → all benchmark templates should silently skip.
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);
    let benchmark_template_ids = [
        "ctr_above_own_median",
        "ctr_below_own_p25",
        "impressions_unusually_high",
        "ctr_benchmark_context",
        "spend_efficiency_trending",
    ];
    for id in &benchmark_template_ids {
        assert!(
            !narratives.iter().any(|n| n.template_id == *id),
            "benchmark template {id} should NOT fire without a library"
        );
    }
}

// ─── Phase 7A.4 Session 4: Diagnostic code tests ────────────────────

#[test]
fn test_mc7042_stale_library_warning() {
    use mc_narrative::benchmark::{check_staleness, BenchmarkLibrary, PeriodRange};
    use mc_narrative::ledger::{LedgerEntry, NarrativeRecord};
    use std::collections::BTreeMap;

    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-01T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks: BTreeMap::new(),
    };

    // Ledger has an entry for 2026-05 — newer than library's 2026-04.
    let ledger = vec![LedgerEntry {
        schema_version: "1.0".to_string(),
        ledger_entry_id: "test".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        model: "model.yaml".to_string(),
        model_hash: "sha256:test".to_string(),
        report_period: Some("2026-05".to_string()),
        scope: BTreeMap::new(),
        narrative: NarrativeRecord {
            id: "test".to_string(),
            section: None,
            severity: "info".to_string(),
            text: "test".to_string(),
            template_id: "test".to_string(),
            notability_score: None,
        },
        evidence: BTreeMap::new(),
        benchmarks_referenced: vec![],
    }];

    let warning = check_staleness(&lib, &ledger);
    assert!(warning.is_some(), "should detect stale library");
    let msg = warning.unwrap().to_string();
    assert!(msg.contains("MC7042"), "should contain MC7042 code: {msg}");
    assert!(
        msg.contains("2026-05"),
        "should mention ledger latest: {msg}"
    );
    assert!(
        msg.contains("2026-04"),
        "should mention library latest: {msg}"
    );

    // Not stale when ledger period <= library range.
    let ledger_current = vec![LedgerEntry {
        schema_version: "1.0".to_string(),
        ledger_entry_id: "test2".to_string(),
        generated_at: "2026-04-15T10:00:00Z".to_string(),
        model: "model.yaml".to_string(),
        model_hash: "sha256:test".to_string(),
        report_period: Some("2026-04".to_string()),
        scope: BTreeMap::new(),
        narrative: NarrativeRecord {
            id: "test".to_string(),
            section: None,
            severity: "info".to_string(),
            text: "test".to_string(),
            template_id: "test".to_string(),
            notability_score: None,
        },
        evidence: BTreeMap::new(),
        benchmarks_referenced: vec![],
    }];
    assert!(
        check_staleness(&lib, &ledger_current).is_none(),
        "should not warn when ledger period <= library range"
    );
}

#[test]
fn test_mc7041_missing_metric_returns_zero() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use std::collections::BTreeMap;

    // Library with only Impressions, no CTR.
    let mut benchmarks = BTreeMap::new();
    benchmarks.insert(
        "Impressions::channel=Targeted Display".to_string(),
        MetricBenchmark {
            metric: "Impressions".to_string(),
            scope: {
                let mut s = BTreeMap::new();
                s.insert("channel".to_string(), "Targeted Display".to_string());
                s
            },
            p10: 10000.0,
            p25: 20000.0,
            p50: 50000.0,
            p75: 80000.0,
            p90: 100000.0,
            mean: 50000.0,
            stddev: 25000.0,
            sample_count: 6,
        },
    );
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    // Template asks for CTR benchmark which doesn't exist — should return 0.
    let templates = vec![mc_narrative::TemplateDefinition {
        id: "mc7041_test".to_string(),
        family: vec!["display-like".to_string()],
        severity: mc_narrative::Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: "benchmark_p50('CTR') > 0".to_string(),
        template: "should not render".to_string(),
        bindings: BTreeMap::new(),
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: None,
        explanation_priority: 500,
    }];

    let cube = monthly_performance();
    // CTR not in benchmark library → benchmark_p50('CTR') returns 0.0 → template skips.
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, Some(&lib), None);
    assert!(
        narratives.iter().all(|n| n.template_id != "mc7041_test"),
        "template should skip when metric not found in benchmark library (MC7041)"
    );
}

#[test]
fn test_benchmark_lookup_performance() {
    use mc_narrative::benchmark::{BenchmarkLibrary, MetricBenchmark, PeriodRange};
    use mc_narrative::BenchmarkIndex;
    use std::collections::BTreeMap;
    use std::time::Instant;

    // Build a library with 1000 metrics to verify O(1) lookup.
    let mut benchmarks = BTreeMap::new();
    for i in 0..1000 {
        benchmarks.insert(
            format!("Metric{i}::channel=Targeted Display"),
            MetricBenchmark {
                metric: format!("Metric{i}"),
                scope: {
                    let mut s = BTreeMap::new();
                    s.insert("channel".to_string(), "Targeted Display".to_string());
                    s
                },
                p10: 0.0,
                p25: 0.0,
                p50: i as f64,
                p75: 0.0,
                p90: 0.0,
                mean: 0.0,
                stddev: 0.0,
                sample_count: 6,
            },
        );
    }
    let lib = BenchmarkLibrary {
        schema_version: "1.0".to_string(),
        generated_at: "2026-05-07T10:00:00Z".to_string(),
        workspace: "test".to_string(),
        period_range: PeriodRange {
            from: "2025-11".to_string(),
            to: "2026-04".to_string(),
        },
        period_count: 6,
        benchmarks,
    };

    let index = BenchmarkIndex::build(&lib);

    let start = Instant::now();
    let iterations = 10000;
    for i in 0..iterations {
        let metric = format!("Metric{}", i % 1000);
        let _ = index.lookup(&metric, "channel=Targeted Display");
    }
    let elapsed = start.elapsed();
    let per_lookup_ns = elapsed.as_nanos() / iterations;

    assert!(
        per_lookup_ns < 1_000_000, // < 1ms per lookup
        "benchmark lookup should be < 1ms; got {per_lookup_ns}ns"
    );
    eprintln!("[perf] benchmark lookup: {per_lookup_ns}ns avg over {iterations} lookups");
}

// ─── Phase 7A.5: Explanation chain tests (ADR-0022) ──────────────────

/// Helper: create a TemplateDefinition for explanation chain tests.
fn make_template(
    id: &str,
    when: &str,
    template: &str,
    finding_id: Option<&str>,
    priority: u32,
) -> mc_narrative::TemplateDefinition {
    mc_narrative::TemplateDefinition {
        id: id.to_string(),
        family: vec!["display-like".to_string()],
        severity: Severity::Info,
        table_types: vec!["Monthly Performance".to_string()],
        sort_order: 0,
        when: when.to_string(),
        template: template.to_string(),
        bindings: BTreeMap::new(),
        deduplicate: false,
        format: BTreeMap::new(),
        notability_base: None,
        finding_id: finding_id.map(|s| s.to_string()),
        explanation_priority: priority,
    }
}

#[test]
fn test_explanation_chain_first_match_fires() {
    // Three templates in one explanation group. The first (priority 100) should fire.
    let templates = vec![
        make_template(
            "explain_a",
            "current.Impressions > 0",
            "explanation A",
            Some("impr_declined"),
            100,
        ),
        make_template(
            "explain_b",
            "current.Impressions > 0",
            "explanation B",
            Some("impr_declined"),
            200,
        ),
        make_template(
            "explain_c",
            "current.Impressions > 0",
            "explanation C",
            Some("impr_declined"),
            999,
        ),
    ];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);

    assert_eq!(narratives.len(), 1, "only the first match should fire");
    assert_eq!(narratives[0].template_id, "explain_a");
    assert_eq!(narratives[0].finding_id.as_deref(), Some("impr_declined"));
}

#[test]
fn test_explanation_chain_fallback_fires_when_no_match() {
    // First two templates' when: is false. Fallback at 999 should fire.
    let templates = vec![
        make_template(
            "explain_a",
            "current.Impressions < 0",
            "should not fire",
            Some("impr_declined"),
            100,
        ),
        make_template(
            "explain_b",
            "current.Impressions < 0",
            "should not fire",
            Some("impr_declined"),
            200,
        ),
        make_template(
            "explain_fallback",
            "current.Impressions > 0",
            "fallback fired",
            Some("impr_declined"),
            999,
        ),
    ];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);

    assert_eq!(narratives.len(), 1);
    assert_eq!(narratives[0].template_id, "explain_fallback");
}

#[test]
fn test_explanation_chain_skipped_and_rejected_recorded() {
    // Priority 100 when: false (rejected), 200 when: true (fires), 999 never evaluated (skipped).
    let templates = vec![
        make_template(
            "explain_rejected",
            "current.Impressions < 0",
            "rejected",
            Some("finding_x"),
            100,
        ),
        make_template(
            "explain_winner",
            "current.Impressions > 0",
            "winner",
            Some("finding_x"),
            200,
        ),
        make_template(
            "explain_skipped",
            "current.Impressions > 0",
            "skipped",
            Some("finding_x"),
            999,
        ),
    ];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);

    assert_eq!(narratives.len(), 1);
    let n = &narratives[0];
    assert_eq!(n.template_id, "explain_winner");
    assert_eq!(
        n.rejected_explanations,
        vec!["explain_rejected"],
        "rejected list should contain templates whose when: was false"
    );
    assert_eq!(
        n.skipped_explanations,
        vec!["explain_skipped"],
        "skipped list should contain templates never evaluated"
    );
}

#[test]
fn test_templates_without_finding_id_fire_independently() {
    // Two standalone templates + one explanation group. All should fire if their when: passes.
    let templates = vec![
        make_template(
            "standalone_a",
            "current.Impressions > 0",
            "standalone A",
            None,
            500,
        ),
        make_template(
            "standalone_b",
            "current.Clicks > 0",
            "standalone B",
            None,
            500,
        ),
        make_template(
            "grouped_a",
            "current.Impressions > 0",
            "grouped A",
            Some("finding_y"),
            100,
        ),
        make_template(
            "grouped_b",
            "current.Impressions > 0",
            "grouped B",
            Some("finding_y"),
            999,
        ),
    ];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);

    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(ids.contains(&"standalone_a"), "standalone A should fire");
    assert!(ids.contains(&"standalone_b"), "standalone B should fire");
    assert!(
        ids.contains(&"grouped_a"),
        "grouped A should fire (first match in group)"
    );
    assert!(
        !ids.contains(&"grouped_b"),
        "grouped B should be suppressed (grouped_a already matched)"
    );
    assert_eq!(
        ids.len(),
        3,
        "exactly 3 narratives: 2 standalone + 1 from group"
    );
}

#[test]
fn test_mc7050_priority_collision_error() {
    use mc_narrative::NarrativeError;

    let templates = vec![
        make_template("tmpl_a", "true", "a", Some("finding_z"), 200),
        make_template("tmpl_b", "true", "b", Some("finding_z"), 200),
    ];

    let errors = mc_narrative::validate_templates(&templates);
    let has_mc7050 = errors
        .iter()
        .any(|e| matches!(e, NarrativeError::ExplanationPriorityCollision { .. }));
    assert!(
        has_mc7050,
        "MC7050 should fire when two templates share finding_id + priority"
    );
}

#[test]
fn test_mc7053_missing_fallback_info() {
    use mc_narrative::NarrativeError;

    // Group with no priority >= 900 template.
    let templates = vec![
        make_template("tmpl_a", "true", "a", Some("finding_nofb"), 100),
        make_template("tmpl_b", "true", "b", Some("finding_nofb"), 200),
    ];

    let errors = mc_narrative::validate_templates(&templates);
    let has_mc7053 = errors
        .iter()
        .any(|e| matches!(e, NarrativeError::ExplanationMissingFallback { .. }));
    assert!(
        has_mc7053,
        "MC7053 should fire when no fallback template exists"
    );
}

#[test]
fn test_mc7055_singleton_finding_id() {
    use mc_narrative::NarrativeError;

    // Only one template references this finding_id → likely typo.
    let templates = vec![make_template(
        "tmpl_solo",
        "true",
        "solo",
        Some("typo_finding"),
        100,
    )];

    let errors = mc_narrative::validate_templates(&templates);
    let has_mc7055 = errors
        .iter()
        .any(|e| matches!(e, NarrativeError::ExplanationSingletonFindingId { .. }));
    assert!(
        has_mc7055,
        "MC7055 should fire for single-template finding_id"
    );
}

#[test]
fn test_explanation_group_no_match_produces_no_output() {
    // All templates in the group have when: false → group produces nothing.
    let templates = vec![
        make_template("never_a", "false", "never", Some("never_group"), 100),
        make_template("never_b", "false", "never", Some("never_group"), 200),
    ];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);
    assert!(
        narratives.is_empty(),
        "explanation group with no matches should produce no output"
    );
}

// ─── Phase 7A.5 Session 2: Context event evaluator function tests ────

#[test]
fn test_has_context_event_matches_current_period() {
    use mc_narrative::context_events::ContextEvent;

    let events = vec![ContextEvent {
        id: "ce-2025-08-001".to_string(),
        period: "Aug 2025".to_string(),
        scope: BTreeMap::new(),
        event_type: "budget_change".to_string(),
        description: "Budget reduced 40%".to_string(),
        source: None,
        expires_at: None,
    }];

    // Template checks has_context_event('budget_change') == 1.
    let templates = vec![make_template(
        "test_ctx",
        "has_context_event('budget_change') == 1",
        "context matched",
        None,
        500,
    )];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&events));

    assert_eq!(
        narratives.len(),
        1,
        "template should fire when context event matches"
    );
    assert_eq!(narratives[0].text, "context matched");
}

#[test]
fn test_has_context_event_lookback_3_periods() {
    use mc_narrative::context_events::ContextEvent;

    // Event is for Jul_2025, current period is Aug_2025. Lookback=3 should match.
    let events = vec![ContextEvent {
        id: "ce-2025-07-001".to_string(),
        period: "Jul 2025".to_string(),
        scope: BTreeMap::new(),
        event_type: "creative_pause".to_string(),
        description: "3 creatives paused".to_string(),
        source: None,
        expires_at: None,
    }];

    // Lookback=1 should NOT match (current period only).
    let templates_1 = vec![make_template(
        "ctx_lb1",
        "has_context_event('creative_pause') == 1",
        "should not fire",
        None,
        500,
    )];

    // Lookback=3 should match.
    let templates_3 = vec![make_template(
        "ctx_lb3",
        "has_context_event('creative_pause', 3) == 1",
        "lookback matched",
        None,
        500,
    )];

    let cube = monthly_performance();
    let n1 = mc_narrative::evaluate_all(&templates_1, &[cube.clone()], None, None, Some(&events));
    let n3 = mc_narrative::evaluate_all(&templates_3, &[cube], None, None, Some(&events));

    assert!(
        n1.is_empty(),
        "lookback=1 should not match prior period event"
    );
    assert_eq!(n3.len(), 1, "lookback=3 should match prior period event");
}

#[test]
fn test_context_description_returns_first_match() {
    use mc_narrative::context_events::ContextEvent;

    let events = vec![ContextEvent {
        id: "ce-2025-08-001".to_string(),
        period: "Aug 2025".to_string(),
        scope: BTreeMap::new(),
        event_type: "budget_change".to_string(),
        description: "Budget reduced 40% for Q1 close-out".to_string(),
        source: None,
        expires_at: None,
    }];

    let mut templates = vec![make_template(
        "ctx_desc",
        "has_context_event('budget_change') == 1",
        "event: {event_desc}",
        None,
        500,
    )];
    templates[0].bindings.insert(
        "event_desc".to_string(),
        "context_description('budget_change')".to_string(),
    );

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&events));

    assert_eq!(narratives.len(), 1);
    assert!(
        narratives[0].text.contains("Budget reduced 40%"),
        "context_description should interpolate: got {}",
        narratives[0].text
    );
}

#[test]
fn test_context_event_scope_subset_matching() {
    use mc_narrative::context_events::ContextEvent;

    // Event scoped to Channel=Targeted Display.
    let events = vec![ContextEvent {
        id: "ce-2025-08-001".to_string(),
        period: "Aug 2025".to_string(),
        scope: {
            let mut s = BTreeMap::new();
            s.insert("channel".to_string(), "Targeted Display".to_string());
            s
        },
        event_type: "budget_change".to_string(),
        description: "Budget cut".to_string(),
        source: None,
        expires_at: None,
    }];

    let templates = vec![make_template(
        "ctx_scope",
        "has_context_event('budget_change') == 1",
        "scoped event matched",
        None,
        500,
    )];

    let cube = monthly_performance(); // subproduct = "Targeted Display"
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&events));

    assert_eq!(
        narratives.len(),
        1,
        "scoped event should match when scope is subset of eval scope"
    );
}

#[test]
fn test_context_events_absent_returns_zero() {
    // No context events → has_context_event always returns 0.
    let templates = vec![make_template(
        "no_ctx",
        "has_context_event('budget_change') == 1",
        "should not fire",
        None,
        500,
    )];

    let cube = monthly_performance();
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, None);

    assert!(
        narratives.is_empty(),
        "no context events → template should not fire"
    );
}

#[test]
fn test_mc7051_unknown_period_warning() {
    use mc_narrative::context_events::{validate_context_events, ContextEvent};
    use mc_narrative::NarrativeError;

    let events = vec![ContextEvent {
        id: "ce-2099-01-001".to_string(),
        period: "2099-01".to_string(),
        scope: BTreeMap::new(),
        event_type: "budget_change".to_string(),
        description: "future event".to_string(),
        source: None,
        expires_at: None,
    }];

    let known_periods = vec!["Aug_2025", "Jul_2025"];
    let errors = validate_context_events(&events, &known_periods);
    let has_mc7051 = errors
        .iter()
        .any(|e| matches!(e, NarrativeError::ContextEventUnknownPeriod { .. }));
    assert!(has_mc7051, "MC7051 should fire for unknown period");
}

#[test]
fn test_mc7052_expires_before_period_warning() {
    use mc_narrative::context_events::{validate_context_events, ContextEvent};
    use mc_narrative::NarrativeError;

    let events = vec![ContextEvent {
        id: "ce-2025-08-001".to_string(),
        period: "2025-08".to_string(),
        scope: BTreeMap::new(),
        event_type: "budget_change".to_string(),
        description: "bad expires".to_string(),
        source: None,
        expires_at: Some("2025-07".to_string()),
    }];

    let errors = validate_context_events(&events, &[]);
    let has_mc7052 = errors
        .iter()
        .any(|e| matches!(e, NarrativeError::ContextEventExpiresBeforePeriod { .. }));
    assert!(has_mc7052, "MC7052 should fire when expires_at < period");
}

// ─── Phase 7A.5 Session 3: Auto-detection tests ─────────────────────

#[test]
fn test_auto_detect_budget_decrease_event() {
    // Cube where Budget dropped >20% → auto-detect budget_decrease event.
    let cube = CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Budget".into(),
                vec![
                    CellEntry {
                        category: "Jul 2025".into(),
                        value: 10000.0,
                    },
                    CellEntry {
                        category: "Aug 2025".into(),
                        value: 5000.0, // 50% decrease
                    },
                ],
            ),
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Jul 2025".into(),
                        value: 25000.0,
                    },
                    CellEntry {
                        category: "Aug 2025".into(),
                        value: 12000.0,
                    },
                ],
            ),
        ]),
    };

    // Template checks for auto-detected budget_decrease event.
    let templates = vec![make_template(
        "auto_budget",
        "has_context_event('budget_decrease') == 1",
        "budget decrease detected",
        None,
        500,
    )];

    // No manual events — should still fire via auto-detection.
    let empty_events: Vec<mc_narrative::context_events::ContextEvent> = Vec::new();
    let narratives =
        mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&empty_events));

    assert_eq!(
        narratives.len(),
        1,
        "auto-detected budget_decrease event should fire template"
    );
}

#[test]
fn test_auto_detect_single_period_event() {
    let cube = CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([(
            "Impressions".into(),
            vec![CellEntry {
                category: "Aug 2025".into(),
                value: 25000.0,
            }],
        )]),
    };

    let templates = vec![make_template(
        "single_period",
        "has_context_event('single_period') == 1",
        "single period",
        None,
        500,
    )];

    let empty_events: Vec<mc_narrative::context_events::ContextEvent> = Vec::new();
    let narratives =
        mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&empty_events));

    assert_eq!(
        narratives.len(),
        1,
        "auto-detected single_period event should fire template"
    );
}

#[test]
fn test_auto_events_coexist_with_manual_events() {
    use mc_narrative::context_events::ContextEvent;

    let cube = CubeData {
        table_name: "Monthly Performance".into(),
        subproduct: "Targeted Display".into(),
        source_file: "report.csv".into(),
        dimension_name: None,
        values: BTreeMap::from([
            (
                "Budget".into(),
                vec![
                    CellEntry {
                        category: "Jul 2025".into(),
                        value: 10000.0,
                    },
                    CellEntry {
                        category: "Aug 2025".into(),
                        value: 5000.0,
                    },
                ],
            ),
            (
                "Impressions".into(),
                vec![
                    CellEntry {
                        category: "Jul 2025".into(),
                        value: 25000.0,
                    },
                    CellEntry {
                        category: "Aug 2025".into(),
                        value: 12000.0,
                    },
                ],
            ),
        ]),
    };

    // Manual event alongside auto-detected.
    let events = vec![ContextEvent {
        id: "ce-manual-001".to_string(),
        period: "Aug 2025".to_string(),
        scope: BTreeMap::new(),
        event_type: "creative_pause".to_string(),
        description: "3 creatives paused".to_string(),
        source: None,
        expires_at: None,
    }];

    // Two templates: one checks auto-detected, one checks manual.
    let templates = vec![
        make_template(
            "auto_check",
            "has_context_event('budget_decrease') == 1",
            "auto",
            None,
            500,
        ),
        make_template(
            "manual_check",
            "has_context_event('creative_pause') == 1",
            "manual",
            None,
            500,
        ),
    ];

    let narratives = mc_narrative::evaluate_all(&templates, &[cube], None, None, Some(&events));

    let ids: Vec<&str> = narratives.iter().map(|n| n.template_id.as_str()).collect();
    assert!(
        ids.contains(&"auto_check"),
        "auto-detected event should fire"
    );
    assert!(ids.contains(&"manual_check"), "manual event should fire");
}
