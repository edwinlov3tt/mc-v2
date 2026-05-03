//! Phase 3C per-validator negative tests for MC2012–MC2025
//! (excluding MC2022's path-escape variant — that lives in
//! `path_escape.rs` per the handoff scope; this file's MC2022 fixture
//! covers the file-not-found case).
//!
//! Each fixture in `tests/fixture_validation_fixtures/` is a minimal
//! YAML model with exactly one mistake. The test driver loads each,
//! runs `parse → validate → resolve_inputs`, and asserts:
//!
//! 1. The expected MC2xxx code appears at least once.
//! 2. No OTHER MC2012–MC2025 codes appear (so the fixture really
//!    isolates the targeted rule and doesn't double-fire).
//!
//! Per ADR-0006 Decision 6: each rule has a dedicated negative
//! fixture. Per amendment #19's columns contract, MC2018 / MC2021 /
//! MC2024 are the three explicitly-required fixtures and they are
//! present in this set.

use std::path::Path;

use mc_model::ValidationError;

const FIXTURE_DIR: &str = "tests/fixture_validation_fixtures";

/// Run the parse → validate → resolve_inputs pipeline against a
/// fixture file. Returns the diagnostic codes produced by EITHER
/// stage 2 (validate) or the resolve-inputs stage. Parse errors are
/// surfaced as `panic!` because the negative fixtures are
/// hand-authored to be syntactically valid.
fn run_pipeline(fixture_filename: &str) -> Vec<&'static str> {
    let path = format!("{FIXTURE_DIR}/{fixture_filename}");
    let yaml = std::fs::read_to_string(&path).expect("read fixture");
    let parsed = mc_model::parse(&yaml, Some(path.clone()))
        .unwrap_or_else(|e| panic!("fixture {fixture_filename:?} failed to parse: {e}"));

    let mut codes: Vec<&'static str> = Vec::new();
    let validated = match mc_model::validate(parsed) {
        Ok(v) => v,
        Err(errs) => {
            // If validate already failed, that's a fixture bug —
            // we want to test resolve_inputs. Surface the codes so
            // the failing assertion shows them.
            return errs.into_iter().map(|e| e.code()).collect();
        }
    };
    let model_dir = Path::new(&path).parent();
    if let Err(errs) = mc_model::resolve_inputs(&validated, model_dir) {
        codes.extend(errs.into_iter().map(|e| e.code()));
    }
    codes
}

/// Codes covered by Phase 3C (MC2012..=MC2025, used to detect spurious
/// other-rule firings).
const PHASE_3C_CODES: &[&str] = &[
    "MC2012", "MC2013", "MC2014", "MC2015", "MC2016", "MC2017", "MC2018", "MC2019", "MC2020",
    "MC2021", "MC2022", "MC2023", "MC2024", "MC2025",
];

fn assert_only_target_fires(codes: &[&'static str], target: &'static str, fixture: &str) {
    assert!(
        codes.contains(&target),
        "fixture {fixture:?}: expected {target} in produced codes, got: {codes:?}"
    );
    let spurious: Vec<&str> = codes
        .iter()
        .copied()
        .filter(|c| *c != target && PHASE_3C_CODES.contains(c))
        .collect();
    assert!(
        spurious.is_empty(),
        "fixture {fixture:?}: spurious other Phase-3C codes fired: {spurious:?} \
         (full code list: {codes:?})"
    );
}

#[test]
fn mc2012_fires_alone() {
    let fixture = "MC2012_unknown_dimension_key.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2012", fixture);
}

#[test]
fn mc2013_fires_alone() {
    let fixture = "MC2013_unknown_element_value.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2013", fixture);
}

#[test]
fn mc2014_fires_alone() {
    let fixture = "MC2014_unknown_measure.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2014", fixture);
}

#[test]
fn mc2015_fires_alone() {
    let fixture = "MC2015_writes_derived.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2015", fixture);
}

#[test]
fn mc2016_fires_alone() {
    let fixture = "MC2016_duplicate_fixture_name.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2016", fixture);
}

#[test]
fn mc2017_fires_alone() {
    let fixture = "MC2017_golden_unknown_fixture.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2017", fixture);
}

#[test]
fn mc2018_fires_alone() {
    let fixture = "MC2018_value_type_mismatch.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2018", fixture);
}

#[test]
fn mc2019_fires_alone() {
    let fixture = "MC2019_missing_required_dimension.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2019", fixture);
}

#[test]
fn mc2020_fires_alone() {
    let fixture = "MC2020_writes_consolidated_cell.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2020", fixture);
}

#[test]
fn mc2021_fires_alone() {
    let fixture = "MC2021_value_is_nan.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2021", fixture);
}

#[test]
fn mc2022_file_not_found_fires_alone() {
    let fixture = "MC2022_source_unreadable.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2022", fixture);
}

#[test]
fn mc2023_fires_alone() {
    let fixture = "MC2023_csv_row_column_count.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2023", fixture);
}

#[test]
fn mc2024_fires_alone() {
    let fixture = "MC2024_csv_header_mismatch.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2024", fixture);
}

#[test]
fn mc2025_fires_alone() {
    let fixture = "MC2025_duplicate_input_coordinate.yaml";
    let codes = run_pipeline(fixture);
    assert_only_target_fires(&codes, "MC2025", fixture);
}

/// Sanity sweep: ensure every Phase 3C code MC2012–MC2025 has a
/// dedicated fixture in the directory. Catches "we added a new code
/// but forgot to cover it".
#[test]
fn every_phase_3c_code_has_a_fixture() {
    let entries: Vec<String> = std::fs::read_dir(FIXTURE_DIR)
        .expect("read fixture dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    for code in PHASE_3C_CODES {
        let has_fixture = entries.iter().any(|f| f.starts_with(code));
        assert!(
            has_fixture,
            "no fixture file in {FIXTURE_DIR} starts with {code}; expected one"
        );
    }
}

/// Sanity that ValidationError code() covers all 25 active codes
/// (Phase 3A's MC2001-2010 + MC2011 + Phase 3C's MC2012-2025) and that
/// the codes are unique.
#[test]
fn all_validation_error_codes_are_unique_and_in_range() {
    let codes = [
        ValidationError::DuplicateName {
            kind: "x".into(),
            name: "x".into(),
        }
        .code(),
        ValidationError::MissingDimension {
            name: "x".into(),
            referenced_by: "x".into(),
        }
        .code(),
        ValidationError::InvalidHierarchyEdge {
            dim: "x".into(),
            element: "x".into(),
        }
        .code(),
        ValidationError::HierarchyCycle {
            dim: "x".into(),
            path: "x".into(),
        }
        .code(),
        ValidationError::RuleReferencesUnknownMeasure {
            rule_name: "x".into(),
            measure_name: "x".into(),
        }
        .code(),
        ValidationError::DerivedMeasureWithoutRule {
            measure_name: "x".into(),
        }
        .code(),
        ValidationError::InputMeasureHasRule {
            measure_name: "x".into(),
            rule_name: "x".into(),
        }
        .code(),
        ValidationError::RuleCycle { path: "x".into() }.code(),
        ValidationError::UnsupportedAggregation {
            measure_name: "x".into(),
            method: "x".into(),
        }
        .code(),
        ValidationError::Schema {
            message: "x".into(),
        }
        .code(),
        ValidationError::WeightedAverageMissingWeight {
            measure_name: "x".into(),
        }
        .code(),
        ValidationError::FixtureUnknownDimensionKey {
            input_set: "x".into(),
            column: "x".into(),
        }
        .code(),
        ValidationError::FixtureUnknownElementValue {
            input_set: "x".into(),
            row_index: 0,
            dim: "x".into(),
            value: "x".into(),
        }
        .code(),
        ValidationError::FixtureUnknownMeasure {
            input_set: "x".into(),
            row_index: 0,
            measure: "x".into(),
        }
        .code(),
        ValidationError::FixtureWritesDerivedMeasure {
            input_set: "x".into(),
            row_index: 0,
            measure: "x".into(),
        }
        .code(),
        ValidationError::DuplicateFixtureName { name: "x".into() }.code(),
        ValidationError::GoldenReferencesUnknownFixture {
            golden_name: "x".into(),
            fixture_name: "x".into(),
        }
        .code(),
        ValidationError::FixtureValueTypeMismatch {
            input_set: "x".into(),
            row_index: 0,
            measure: "x".into(),
            data_type: "x".into(),
            value: "x".into(),
        }
        .code(),
        ValidationError::FixtureMissingDimension {
            input_set: "x".into(),
            columns: vec![],
            missing: vec![],
        }
        .code(),
        ValidationError::FixtureWritesConsolidatedCell {
            input_set: "x".into(),
            row_index: 0,
            dim: "x".into(),
            element: "x".into(),
        }
        .code(),
        ValidationError::FixtureValueIsNaN {
            input_set: "x".into(),
            row_index: 0,
        }
        .code(),
        ValidationError::FixtureSourceUnreadable {
            input_set: "x".into(),
            path: "x".into(),
            reason: "x".into(),
        }
        .code(),
        ValidationError::FixtureCsvRowColumnCountMismatch {
            input_set: "x".into(),
            line: 0,
            expected: 0,
            actual: 0,
        }
        .code(),
        ValidationError::FixtureCsvHeaderMismatch {
            input_set: "x".into(),
            expected: vec![],
            actual: vec![],
        }
        .code(),
        ValidationError::FixtureDuplicateCoordinate {
            input_set: "x".into(),
            first_row: 0,
            second_row: 0,
        }
        .code(),
    ];
    let mut sorted: Vec<&str> = codes.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        codes.len(),
        "duplicate codes detected: {codes:?}"
    );
    // No code may equal MC3008 (permanently retired) or fall outside
    // the validation namespace MC2xxx.
    for c in &codes {
        assert!(c.starts_with("MC2"), "validation code outside MC2xxx: {c}");
        assert_ne!(*c, "MC3008", "MC3008 is permanently retired (Phase 3B)");
    }
}
