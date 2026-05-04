//! Recipe validation against a loaded `mc_model::ValidatedModel`.
//!
//! [`validate_recipe`] walks the parsed [`Recipe`] and emits every
//! validation error it finds (no fail-fast) — the diagnostic envelope's
//! deterministic sort order makes the result LLM-friendly even when
//! many problems are present in a single recipe.
//!
//! The six semantic rules from ADR-0010 Decision 7 are enforced here:
//!
//! 1. **1:1 mappings** (amendment #7) — MC5011, both shapes.
//! 2. **Defaults vs. columns mutual exclusion** (amendment #8) — MC5016.
//! 3. **Input measures only** (amendment #2) — MC5018.
//! 4. **`write_disposition: replace` semantics** (amendment #4) — schema
//!    enforced (only `Replace` is a valid variant).
//! 5. **`model:` path resolution** (amendment #10) — MC5017.
//! 6. **`on_error:` semantics** (amendment #9) — schema enforced (only
//!    `abort` / `skip_row` / `quarantine` are valid variants).
//!
//! Rules 4 and 6 require no validator code: rejecting unrecognized
//! values is the deserializer's job (MC5001 fires at parse time on a
//! bad enum value). The remaining rules + the supporting MC5xxx checks
//! are below.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use mc_model::ValidatedModel;

use crate::error::{ColumnTargetIssue, RecipeError};
use crate::schema::{ColumnMapping, Recipe, SourceFormat};

/// Optional file-system context for validation. When supplied, enables
/// MC5017 path-escape detection. When `None`, the path-escape check is
/// skipped — appropriate for in-memory recipes (e.g., LLM-emitted, no
/// file backing).
#[derive(Clone, Copy, Debug)]
pub struct PathContext<'a> {
    /// Directory containing the recipe file. The recipe's
    /// [`Recipe::model`] path is resolved relative to this directory.
    pub recipe_dir: &'a Path,
    /// Workspace root. The resolved model path must remain inside this
    /// directory; lexically escaping outside fires MC5017.
    pub workspace_root: &'a Path,
}

/// Validate a parsed recipe against a loaded model.
///
/// Returns the list of every [`RecipeError`] found, in declaration order
/// (sort with [`crate::diagnostic::sort_diagnostics`] before emission).
/// Empty vec means the recipe is valid against the model.
///
/// `path_ctx` is optional: when supplied, MC5017 (model path escape) is
/// checked; when `None`, that check is skipped.
pub fn validate_recipe(
    recipe: &Recipe,
    model: &ValidatedModel,
    path_ctx: Option<PathContext<'_>>,
) -> Vec<RecipeError> {
    let mut errors = Vec::new();

    // Per ADR-0010 amendment #10: model: path escape protection.
    if let Some(ctx) = path_ctx {
        if let Some(err) = check_model_path_escape(&recipe.model, ctx) {
            errors.push(err);
        }
    }

    // Per ADR-0010 §"Recipe semantic rules": source.table / source.query
    // mutual exclusion (MC5003).
    if recipe.source.table.is_some() && recipe.source.query.is_some() {
        errors.push(RecipeError::SourceTableQueryConflict {
            path: "/source".to_string(),
        });
    }

    // Track source columns we've seen — MC5010 (duplicate column).
    // Insertion order: first-seen wins; subsequent occurrences fire
    // MC5010 with first_path back-reference.
    let mut seen_columns: HashMap<&str, String> = HashMap::new();

    // Track which dimensions are mapped via columns — used downstream
    // for MC5016 (mutual exclusion with defaults). Map dim_name → first
    // column path that introduced it.
    let mut mapped_dimensions: HashMap<&str, String> = HashMap::new();

    // Per-column checks (MC5004, MC5005, MC5006, MC5010, MC5011, MC5018).
    for (idx, col) in recipe.columns.iter().enumerate() {
        let path = format!("/columns/{idx}");
        validate_column(col, &path, model, &mut errors);

        // MC5010: duplicate source column. Also records first occurrence
        // so a third duplicate still references the original first.
        if let Some(first_path) = seen_columns.get(col.source.as_str()) {
            errors.push(RecipeError::DuplicateColumn {
                path: format!("{path}/source"),
                source_column: col.source.clone(),
                first_path: format!("{first_path}/source"),
            });
        } else {
            seen_columns.insert(col.source.as_str(), path.clone());
        }

        // Track mapped dimensions for the MC5016 check below.
        if !column_is_skipped(col) {
            if let Some(dim) = col.dimension.as_deref() {
                // First-occurrence wins; if the user wrote two mappings
                // to the same dim that's a recipe-author bug but it's
                // covered by MC5010 (duplicate source) or its own
                // detection elsewhere — record only the first.
                mapped_dimensions
                    .entry(dim)
                    .or_insert_with(|| format!("{path}/dimension"));
            }
        }
    }

    // Per ADR-0010 Amendment 2: MC5021 — format: long with measure: X in
    // columns is a mutual-exclusion violation.
    let is_long_format = matches!(recipe.source.format, Some(SourceFormat::Long));
    if is_long_format {
        for (idx, col) in recipe.columns.iter().enumerate() {
            if column_is_skipped(col) {
                continue;
            }
            if let Some(measure) = &col.measure {
                errors.push(RecipeError::LongFormatMeasureColumnConflict {
                    path: format!("/columns/{idx}/measure"),
                    source_column: col.source.clone(),
                    measure: measure.clone(),
                });
            }
        }
    }

    // Per ADR-0010 amendment #8: defaults vs. columns mutual exclusion
    // (MC5016) + default-key/value resolution (MC5008, MC5009).
    for (dim_name, element_name) in &recipe.defaults {
        let default_path = format!("/defaults/{dim_name}");

        // MC5016: dim appears in both columns and defaults.
        if let Some(column_path) = mapped_dimensions.get(dim_name.as_str()) {
            errors.push(RecipeError::DimensionInColumnsAndDefaults {
                dimension: dim_name.clone(),
                column_path: column_path.clone(),
                default_path: default_path.clone(),
            });
            // Skip element-resolution for this dim — the structural
            // problem is the mutual-exclusion violation, not the value.
            continue;
        }

        // MC5008: default key must name a real dimension.
        let dim_idx = match model.dim_index_by_name.get(dim_name) {
            Some(idx) => *idx,
            None => {
                errors.push(RecipeError::DefaultUnknownDimension {
                    path: default_path,
                    dimension: dim_name.clone(),
                });
                continue;
            }
        };

        // MC5009: default value must name a real element of that dim.
        let element_map = &model.element_index_by_name[dim_idx];
        if !element_map.contains_key(element_name) {
            errors.push(RecipeError::DefaultUnknownElement {
                path: default_path,
                dimension: dim_name.clone(),
                element: element_name.clone(),
            });
        }
    }

    errors
}

/// Per-column validation: MC5004 (unknown dim), MC5005 (unknown
/// measure), MC5006 (type incompat), MC5011 (no/ambiguous target),
/// MC5018 (Derived measure).
fn validate_column(
    col: &ColumnMapping,
    path: &str,
    model: &ValidatedModel,
    errors: &mut Vec<RecipeError>,
) {
    let skipped = column_is_skipped(col);
    let has_dim = col.dimension.is_some();
    let has_measure = col.measure.is_some();

    // MC5011: 1:1 mapping rule — when not skipped, exactly one of
    // dimension / measure must be set.
    if !skipped {
        if has_dim && has_measure {
            errors.push(RecipeError::ColumnNoSingleTarget {
                path: path.to_string(),
                source_column: col.source.clone(),
                kind: ColumnTargetIssue::Ambiguous,
            });
            // Continue — still surface MC5004/MC5005 if either name
            // is invalid, so the author sees every problem in one pass.
        } else if !has_dim && !has_measure {
            errors.push(RecipeError::ColumnNoSingleTarget {
                path: path.to_string(),
                source_column: col.source.clone(),
                kind: ColumnTargetIssue::NoTarget,
            });
            // No target → no further per-target validation needed.
            return;
        }
    } else {
        // Skipped column: ignore dimension/measure/type fields entirely.
        return;
    }

    // MC5004: dimension name resolution.
    if let Some(dim) = col.dimension.as_deref() {
        if !model.dim_index_by_name.contains_key(dim) {
            errors.push(RecipeError::UnknownDimension {
                path: format!("{path}/dimension"),
                source_column: col.source.clone(),
                dimension: dim.to_string(),
            });
        }
    }

    // MC5005 + MC5018 + MC5006: measure name resolution + role check +
    // type compat.
    if let Some(measure_name) = col.measure.as_deref() {
        match model.measure_index_by_name.get(measure_name) {
            None => {
                errors.push(RecipeError::UnknownMeasure {
                    path: format!("{path}/measure"),
                    source_column: col.source.clone(),
                    measure: measure_name.to_string(),
                });
            }
            Some(&m_idx) => {
                let measure = &model.parsed.measures[m_idx];

                // MC5018: must be Input role.
                //
                // ValidatedModel exposes the measure's role as a string
                // ("Input" / "Derived"); per the Stream B handoff's
                // SPEC QUESTION #3 we read it directly from the
                // ValidatedModel rather than importing mc-core's
                // MeasureRole enum (mc-recipe must not depend on
                // mc-core).
                if measure.role != "Input" {
                    errors.push(RecipeError::DerivedMeasureWriteRejected {
                        path: format!("{path}/measure"),
                        source_column: col.source.clone(),
                        measure: measure_name.to_string(),
                    });
                }

                // MC5006: type compat (only when column declared a type).
                if let Some(col_type) = col.data_type.as_deref() {
                    if !types_compatible(col_type, &measure.data_type) {
                        errors.push(RecipeError::ColumnTypeIncompatible {
                            path: format!("{path}/type"),
                            source_column: col.source.clone(),
                            column_type: col_type.to_string(),
                            measure: measure_name.to_string(),
                            measure_type: measure.data_type.clone(),
                        });
                    }
                }
            }
        }
    }
}

/// Whether a column mapping is explicitly skipped.
fn column_is_skipped(col: &ColumnMapping) -> bool {
    matches!(col.skip, Some(true))
}

/// Case-insensitive type-name compatibility check. The recipe's
/// declared column type (`f64`, `i64`, `string`, `bool`, `category`)
/// must match the model measure's `data_type` (`F64`, `I64`, `Bool`,
/// `Category`) after lowercasing both sides.
///
/// Phase 5A only validates exact (case-insensitive) equality. Future
/// phases may add coercion families (e.g., `int` → `I64`).
fn types_compatible(column_type: &str, measure_type: &str) -> bool {
    column_type.to_ascii_lowercase() == measure_type.to_ascii_lowercase()
}

/// MC5017 — does the recipe's `model:` path, resolved relative to the
/// recipe's directory, escape the workspace root?
///
/// **Lexical** resolution (no filesystem touches):
///
/// 1. Compose `recipe_dir.join(model)`.
/// 2. Normalize lexically (collapse `.`, pop on `..`).
/// 3. Canonicalize-style prefix-check: the normalized path must start
///    with the lexically-normalized workspace root.
///
/// We deliberately avoid `Path::canonicalize` because (a) it touches
/// the filesystem, (b) it requires the file to exist (which — per the
/// handoff — is not strictly required at validation time, only at
/// runtime), and (c) any existing-file requirement would surprise
/// callers running `mc tessera dry-run` against a recipe authored
/// before the model was committed.
fn check_model_path_escape(model: &str, ctx: PathContext<'_>) -> Option<RecipeError> {
    let composed = ctx.recipe_dir.join(model);
    let normalized = lexical_normalize(&composed);
    let workspace_norm = lexical_normalize(ctx.workspace_root);

    if normalized.starts_with(&workspace_norm) {
        None
    } else {
        Some(RecipeError::ModelPathEscapesWorkspace {
            path: "/model".to_string(),
            resolved: normalized.display().to_string(),
            workspace_root: workspace_norm.display().to_string(),
        })
    }
}

/// Lexically normalize a path: collapse `.` components and pop on `..`.
/// Does NOT touch the filesystem. Behavior on `..` at the root is to
/// pop nothing (matches Linux semantics; Windows root-disambiguation
/// not needed for Mosaic's use case).
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop the last pushed normal component, if any. If the
                // accumulated path is the root or has no normal
                // component to pop, leave it (matches `path-clean`-style
                // lexical behavior).
                if !out.pop() {
                    // `pop` on an empty PathBuf returns false; preserve
                    // the `..` so callers detecting escape see it.
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn load_acme_model() -> ValidatedModel {
        let yaml = include_str!("../../mc-model/examples/acme.yaml");
        let parsed = mc_model::parse(yaml, None).expect("Acme YAML parses");
        mc_model::validate(parsed).expect("Acme YAML validates")
    }

    fn parse_yaml(s: &str) -> Recipe {
        parse::parse(s).unwrap_or_else(|e| panic!("recipe parse failed: {e}"))
    }

    #[test]
    fn lexical_normalize_collapses_dots() {
        let p = Path::new("/a/./b/../c");
        assert_eq!(lexical_normalize(p), PathBuf::from("/a/c"));
    }

    #[test]
    fn lexical_normalize_preserves_double_dot_at_root() {
        let p = Path::new("../foo");
        assert_eq!(lexical_normalize(p), PathBuf::from("../foo"));
    }

    #[test]
    fn types_compatible_is_case_insensitive() {
        assert!(types_compatible("f64", "F64"));
        assert!(types_compatible("F64", "F64"));
        assert!(types_compatible("BOOL", "Bool"));
        assert!(!types_compatible("i64", "F64"));
        assert!(!types_compatible("string", "F64"));
    }

    fn valid_csv_recipe() -> Recipe {
        parse_yaml(
            r#"
version: 1
name: valid
model: ./acme.yaml
source:
  driver: csv
  path: ./acme.inputs.csv
columns:
  - { source: month, dimension: Time }
  - { source: ch, dimension: Channel }
  - { source: mkt, dimension: Market }
  - { source: spend, measure: Spend, type: f64 }
  - { source: cpc, measure: CPC, type: f64 }
  - { source: campaign, skip: true }
defaults:
  Scenario: Baseline
  Version: Working
"#,
        )
    }

    #[test]
    fn valid_recipe_against_acme_yields_no_errors() {
        let model = load_acme_model();
        let r = valid_csv_recipe();
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.is_empty(), "expected no errors, got: {errs:?}");
    }

    #[test]
    fn unknown_dimension_fires_mc5004() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: region, dimension: Region }
defaults: { Scenario: Baseline, Version: Working }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5004"));
    }

    #[test]
    fn unknown_measure_fires_mc5005() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: blah, measure: NoSuchMeasure }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5005"));
    }

    #[test]
    fn type_incompatibility_fires_mc5006() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend, type: bool }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5006"));
    }

    #[test]
    fn default_unknown_dim_fires_mc5008() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
defaults:
  NotADim: Foo
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5008"));
    }

    #[test]
    fn default_unknown_element_fires_mc5009() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
defaults:
  Scenario: NotAScenarioElement
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5009"));
    }

    #[test]
    fn duplicate_column_fires_mc5010() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
  - { source: spend, measure: CPC }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5010"));
    }

    #[test]
    fn no_target_fires_mc5011_no_target() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: orphan }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5011"));
    }

    #[test]
    fn ambiguous_target_fires_mc5011_ambiguous() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: x, dimension: Time, measure: Spend }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        let mc5011: Vec<_> = errs.iter().filter(|e| e.code() == "MC5011").collect();
        assert_eq!(mc5011.len(), 1, "expected exactly one MC5011");
        if let RecipeError::ColumnNoSingleTarget { kind, .. } = mc5011[0] {
            assert_eq!(*kind, ColumnTargetIssue::Ambiguous);
        }
    }

    #[test]
    fn skipped_column_does_not_fire_mc5011() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: ignored, skip: true }
  - { source: spend, measure: Spend }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(!errs.iter().any(|e| e.code() == "MC5011"));
    }

    #[test]
    fn table_and_query_conflict_fires_mc5003() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source:
  driver: sqlite
  path: ./db.sqlite
  table: rows
  query: "SELECT * FROM rows"
columns:
  - { source: spend, measure: Spend }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5003"));
    }

    #[test]
    fn dim_in_columns_and_defaults_fires_mc5016() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: scen, dimension: Scenario }
defaults:
  Scenario: Baseline
  Version: Working
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5016"));
    }

    #[test]
    fn derived_measure_fires_mc5018() {
        let model = load_acme_model();
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: clicks, measure: Clicks }
"#,
        );
        let errs = validate_recipe(&r, &model, None);
        assert!(errs.iter().any(|e| e.code() == "MC5018"));
    }

    #[test]
    fn model_path_escape_fires_mc5017() {
        let recipe_dir = Path::new("/ws/recipes");
        let workspace_root = Path::new("/ws");
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ../../etc/passwd
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
"#,
        );
        let model = load_acme_model();
        let errs = validate_recipe(
            &r,
            &model,
            Some(PathContext {
                recipe_dir,
                workspace_root,
            }),
        );
        assert!(errs.iter().any(|e| e.code() == "MC5017"));
    }

    #[test]
    fn model_path_inside_workspace_does_not_fire_mc5017() {
        let recipe_dir = Path::new("/ws/recipes");
        let workspace_root = Path::new("/ws");
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ../models/acme.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
"#,
        );
        let model = load_acme_model();
        let errs = validate_recipe(
            &r,
            &model,
            Some(PathContext {
                recipe_dir,
                workspace_root,
            }),
        );
        assert!(!errs.iter().any(|e| e.code() == "MC5017"));
    }

    #[test]
    fn no_path_context_skips_mc5017() {
        // Even with a clearly-escaping path, no path context = no MC5017.
        let r = parse_yaml(
            r#"
version: 1
name: x
model: ../../../etc/passwd
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
"#,
        );
        let model = load_acme_model();
        let errs = validate_recipe(&r, &model, None);
        assert!(!errs.iter().any(|e| e.code() == "MC5017"));
    }
}
