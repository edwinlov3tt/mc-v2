//! Integration tests covering every recipe under `examples/recipes/`.
//!
//! Two-axis matrix:
//!
//! - **Valid recipes** (5): parse cleanly, validate against Acme with
//!   ZERO MC5xxx errors, and roundtrip-stable through `to_yaml` →
//!   `parse`.
//! - **Invalid recipes** (3): parse cleanly, validate produces the
//!   expected MC5xxx code(s).
//!
//! These exercise every public path of the crate: schema (de)serde,
//! parse, validator, and the diagnostic-envelope sort/render.

use std::path::Path;

use mc_recipe::{
    diagnostics_to_json, parse, sort_diagnostics, to_yaml, validate_recipe, Diagnostic, Recipe,
    RecipeError,
};

const VALID_RECIPES: &[&str] = &[
    "acme-csv-import.recipe.yaml",
    "acme-sqlite-import.recipe.yaml",
    "acme-duckdb-import.recipe.yaml",
    "acme-postgres-import.recipe.yaml",
    "acme-http-json-import.recipe.yaml",
];

const INVALID_RECIPES: &[(&str, &str)] = &[
    ("acme-invalid-derived.recipe.yaml", "MC5018"),
    ("acme-invalid-mutual-exclusion.recipe.yaml", "MC5016"),
    ("acme-invalid-unknown-dim.recipe.yaml", "MC5004"),
];

fn examples_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn load_acme() -> mc_model::ValidatedModel {
    let acme_path = examples_dir()
        .parent()
        .unwrap()
        .join("mc-model/examples/acme.yaml");
    let yaml = std::fs::read_to_string(&acme_path).unwrap();
    let parsed = mc_model::parse(&yaml, Some(acme_path.display().to_string())).unwrap();
    mc_model::validate(parsed).unwrap()
}

fn read_recipe(name: &str) -> String {
    let path = examples_dir().join("examples/recipes").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn every_valid_recipe_parses_cleanly() {
    for name in VALID_RECIPES {
        let yaml = read_recipe(name);
        if let Err(e) = parse(&yaml) {
            panic!("recipe {name} failed to parse: {e}");
        }
    }
}

#[test]
fn every_valid_recipe_validates_against_acme_with_zero_errors() {
    let model = load_acme();
    for name in VALID_RECIPES {
        let yaml = read_recipe(name);
        let recipe = parse(&yaml).unwrap();
        let errors = validate_recipe(&recipe, &model, None);
        assert!(
            errors.is_empty(),
            "recipe {name} produced unexpected errors: {errors:#?}"
        );
    }
}

#[test]
fn every_invalid_recipe_parses_cleanly() {
    // Invalid recipes are SEMANTICALLY invalid, not SYNTACTICALLY invalid —
    // they parse cleanly into typed Recipe values; the validator catches
    // the problem.
    for (name, _expected_code) in INVALID_RECIPES {
        let yaml = read_recipe(name);
        if let Err(e) = parse(&yaml) {
            panic!("recipe {name} failed to parse: {e}");
        }
    }
}

#[test]
fn every_invalid_recipe_fires_expected_diagnostic() {
    let model = load_acme();
    for (name, expected_code) in INVALID_RECIPES {
        let yaml = read_recipe(name);
        let recipe = parse(&yaml).unwrap();
        let errors = validate_recipe(&recipe, &model, None);
        let codes: Vec<&str> = errors.iter().map(|e| e.code()).collect();
        assert!(
            codes.contains(expected_code),
            "recipe {name} expected to fire {expected_code} but got {codes:?}"
        );
    }
}

#[test]
fn roundtrip_stability_for_every_valid_recipe() {
    // The contract: parse(to_yaml(parse(s))) == parse(s).
    //
    // Equality is structural (PartialEq on Recipe + components). HashMap
    // ordering does not affect the comparison.
    for name in VALID_RECIPES {
        let yaml = read_recipe(name);
        let r1: Recipe = parse(&yaml).unwrap();
        let serialized = to_yaml(&r1).unwrap();
        let r2: Recipe = parse(&serialized).unwrap_or_else(|e| {
            panic!("recipe {name}: serialized YAML failed to re-parse: {e}\n--- yaml ---\n{serialized}")
        });
        assert_eq!(
            r1, r2,
            "recipe {name}: roundtrip produced different parsed Recipe values"
        );
    }
}

#[test]
fn roundtrip_stability_two_levels_deep_structural() {
    // A stronger structural-equality property: a second parse-serialize
    // cycle must produce the same Recipe value, even if the serialized
    // YAML text differs (HashMap iteration order in `defaults` /
    // `credentials` is intentionally nondeterministic — see schema.rs).
    for name in VALID_RECIPES {
        let yaml = read_recipe(name);
        let r1 = parse(&yaml).unwrap();
        let s1 = to_yaml(&r1).unwrap();
        let r2 = parse(&s1).unwrap();
        let s2 = to_yaml(&r2).unwrap();
        let r3 = parse(&s2).unwrap();
        assert_eq!(
            r1, r3,
            "recipe {name}: structural drift across two roundtrips"
        );
    }
}

#[test]
fn diagnostics_envelope_renders_for_invalid_recipe() {
    let model = load_acme();
    let yaml = read_recipe("acme-invalid-derived.recipe.yaml");
    let recipe = parse(&yaml).unwrap();
    let errors = validate_recipe(&recipe, &model, None);

    let mut diags: Vec<Diagnostic> = errors.iter().map(|e| e.to_diagnostic()).collect();
    sort_diagnostics(&mut diags);
    let json = diagnostics_to_json(&diags);

    assert!(json.contains("\"schema_version\": \"1.0\""));
    assert!(json.contains("\"code\": \"MC5018\""));
    assert!(json.contains("\"severity\": \"error\""));
    assert!(json.contains("Clicks"));
}

#[test]
fn empty_diagnostics_envelope_still_emits_schema_version() {
    let json = diagnostics_to_json(&[]);
    assert!(json.contains("\"schema_version\": \"1.0\""));
    assert!(json.contains("\"diagnostics\": []"));
}

#[test]
fn invalid_recipe_with_path_context_does_not_emit_mc5017_for_examples() {
    // The example recipes use `model: ../../../mc-model/examples/acme.yaml`
    // — that path resolves inside the workspace root from this crate's
    // directory. So even with path context enabled, MC5017 should not
    // fire for the example recipes.
    let model = load_acme();
    let recipe_dir = examples_dir().join("examples/recipes");
    let workspace_root = examples_dir().parent().unwrap().parent().unwrap();
    for name in VALID_RECIPES {
        let yaml = read_recipe(name);
        let recipe = parse(&yaml).unwrap();
        let errors = validate_recipe(
            &recipe,
            &model,
            Some(mc_recipe::PathContext {
                recipe_dir: &recipe_dir,
                workspace_root,
            }),
        );
        let mc5017: Vec<_> = errors
            .iter()
            .filter(|e| e.code() == "MC5017")
            .collect::<Vec<&RecipeError>>();
        assert!(
            mc5017.is_empty(),
            "recipe {name}: unexpected MC5017 — {mc5017:?}"
        );
    }
}
