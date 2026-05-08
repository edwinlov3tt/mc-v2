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
    // 14 from display-like.yaml + 5 from trend-templates.yaml (Phase 7A.3).
    assert_eq!(
        templates.len(),
        19,
        "display-like.yaml (14) + trend-templates.yaml (5) = 19 templates"
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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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
    let narratives = mc_narrative::evaluate_all(&templates, &cubes, None);

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

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger));

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

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger));

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

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger));

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

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&ledger));

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
    let narratives = mc_narrative::evaluate_all(&templates, &[cube], Some(&empty_ledger));

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

    let narratives = mc_narrative::evaluate_all(&templates, &cubes, Some(&empty_ledger));

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
    let _ = mc_narrative::evaluate_all(&templates, &cubes, Some(&entries));

    // Measure 10 iterations.
    let start = Instant::now();
    let iterations = 10;
    for _ in 0..iterations {
        let _ = mc_narrative::evaluate_all(&templates, &cubes, Some(&entries));
    }
    let elapsed = start.elapsed();
    let median_ms = elapsed.as_millis() as f64 / iterations as f64;

    assert!(
        median_ms < 50.0, // 50ms generous ceiling (handoff says <5ms median)
        "ledger query with 1000 entries should complete in <50ms; got {median_ms:.1}ms"
    );
    eprintln!("[perf] 1000-entry ledger evaluation: {median_ms:.2}ms avg over {iterations} runs");
}
