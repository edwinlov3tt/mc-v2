//! PPTX profile schema — per ADR-0023 Decision 2.
//!
//! Profiles decouple **what tactic** (section) from **what kind of table**
//! (family). The cross-product of sections and families produces the full
//! mapping space. Profiles live at `.mosaic/pptx-profiles/<profile-id>.yaml`.

use serde::{Deserialize, Serialize};

/// A PPTX matching profile — declares sections, table families, aliases,
/// skip rules, and duplicate pairs for a specific reporting template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PptxProfile {
    pub schema_version: String,
    pub profile_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub thresholds: MatchThresholds,
    #[serde(default)]
    pub aliases: AliasConfig,
    #[serde(default)]
    pub sections: Vec<SectionDef>,
    #[serde(default)]
    pub table_families: Vec<TableFamilyDef>,
    #[serde(default)]
    pub skip_tables: Vec<SkipRule>,
    #[serde(default)]
    pub duplicate_section_pairs: Vec<DuplicatePair>,
    #[serde(default)]
    pub overrides: Vec<OverrideDef>,
}

/// Confidence threshold overrides — configurable per profile.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchThresholds {
    #[serde(default = "default_030")]
    pub auto_match_min_score: f64,
    #[serde(default = "default_005")]
    pub auto_match_min_margin: f64,
    #[serde(default = "default_020")]
    pub auto_match_min_relative_margin: f64,
    #[serde(default = "default_020")]
    pub flag_for_review_min_score: f64,
}

impl Default for MatchThresholds {
    fn default() -> Self {
        Self {
            auto_match_min_score: 0.30,
            auto_match_min_margin: 0.05,
            auto_match_min_relative_margin: 0.20,
            flag_for_review_min_score: 0.20,
        }
    }
}

fn default_030() -> f64 {
    0.30
}
fn default_005() -> f64 {
    0.05
}
fn default_020() -> f64 {
    0.20
}

/// Three kinds of aliases — applied at different cascade steps.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AliasConfig {
    #[serde(default)]
    pub tactic: Vec<TacticAlias>,
    #[serde(default)]
    pub header: Vec<HeaderAlias>,
    #[serde(default)]
    pub registry: Vec<RegistryAlias>,
}

/// Tactic-name normalization — used by first-column lookup AND rollup parsing.
/// Has two output forms: `canonical` (single target) or `expands_to` (multiple).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TacticAlias {
    pub input: String,
    #[serde(default)]
    pub canonical: Option<String>,
    #[serde(default)]
    pub expands_to: Vec<String>,
}

/// Header-token normalization — applied before IDF scoring.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeaderAlias {
    pub input: String,
    pub canonical: String,
}

/// Registry duplicate patches — different entries that should be treated
/// as the same logical tactic.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryAlias {
    pub canonical: RegistryRef,
    pub duplicates_of: Vec<RegistryRef>,
}

/// A reference to a specific registry entry by product + subproduct.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryRef {
    pub product_name: String,
    pub subproduct_name: String,
}

/// A section definition — what propagates as context after a divider.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SectionDef {
    pub id: String,
    pub title_matchers: Vec<TitleMatcher>,
    pub propagates: SectionPropagates,
}

/// What a section divider propagates to subsequent slides.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SectionPropagates {
    pub product_name: String,
    pub default_subproduct: String,
}

/// A table family definition — what kind of table this is.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TableFamilyDef {
    pub id: String,
    pub title_matchers: Vec<TitleMatcher>,
    pub table_name: String,
    #[serde(default)]
    pub use_first_column_lookup: bool,
    #[serde(default)]
    pub first_column_header_in: Vec<String>,
}

/// Title matching predicate — multiple variants for flexibility.
/// A matcher is satisfied if ANY of its conditions match.
/// Deserialized from YAML maps like `{ equals: "Meta" }` or `{ contains_any: [...] }`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TitleMatcher {
    #[serde(default)]
    pub equals: Option<String>,
    #[serde(default)]
    pub equals_any: Option<Vec<String>>,
    #[serde(default)]
    pub starts_with: Option<String>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub contains_any: Option<Vec<String>>,
}

impl TitleMatcher {
    /// Check if a normalized title matches this matcher.
    /// ANY set field that matches returns true.
    pub fn matches(&self, normalized_title: &str) -> bool {
        if let Some(ref s) = self.equals {
            if normalized_title == s.to_lowercase() {
                return true;
            }
        }
        if let Some(ref list) = self.equals_any {
            if list.iter().any(|s| normalized_title == s.to_lowercase()) {
                return true;
            }
        }
        if let Some(ref s) = self.starts_with {
            if normalized_title.starts_with(&s.to_lowercase()) {
                return true;
            }
        }
        if let Some(ref s) = self.contains {
            if normalized_title.contains(&s.to_lowercase()) {
                return true;
            }
        }
        if let Some(ref list) = self.contains_any {
            if list
                .iter()
                .any(|s| normalized_title.contains(&s.to_lowercase()))
            {
                return true;
            }
        }
        false
    }
}

impl TitleMatcher {
    /// Convenience constructors for tests.
    pub fn eq(s: &str) -> Self {
        TitleMatcher {
            equals: Some(s.to_string()),
            equals_any: None,
            starts_with: None,
            contains: None,
            contains_any: None,
        }
    }
    pub fn contains_str(s: &str) -> Self {
        TitleMatcher {
            equals: None,
            equals_any: None,
            starts_with: None,
            contains: Some(s.to_string()),
            contains_any: None,
        }
    }
}

/// Skip rule — tables matching these patterns are excluded silently.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkipRule {
    pub when: SkipCondition,
    #[serde(default)]
    pub reason: String,
}

/// Conditions for skip rules.
/// Supports both title-based matching (profile-authored) and positional
/// matching (review UI save-back). Positional fields are optional.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkipCondition {
    #[serde(default)]
    pub table_title_contains_any: Vec<String>,
    #[serde(default)]
    pub slide_title_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_index: Option<u32>,
}

/// Known duplicate section pairs for dedup.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DuplicatePair {
    pub sections: Vec<String>,
    #[serde(default)]
    pub matches_section_titles: Vec<String>,
    #[serde(default)]
    pub canonical: Option<String>,
    #[serde(default)]
    pub note: String,
}

/// Hard override — pinned (slide_index, table_index) → mapping.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverrideDef {
    pub slide_index: u32,
    pub table_index: u32,
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
}

/// Save a profile to `<dir>/.mosaic/pptx-profiles/<profile_id>.yaml`.
/// Uses atomic write (tmp + rename) to avoid partial writes.
pub fn save_profile(dir: &std::path::Path, profile: &PptxProfile) -> Result<(), String> {
    let profiles_dir = dir.join(".mosaic").join("pptx-profiles");
    std::fs::create_dir_all(&profiles_dir)
        .map_err(|e| format!("failed to create profile directory: {e}"))?;

    let path = profiles_dir.join(format!("{}.yaml", profile.profile_id));
    let tmp_path = profiles_dir.join(format!(".{}.yaml.tmp", profile.profile_id));

    let content =
        serde_yaml::to_string(profile).map_err(|e| format!("failed to serialize profile: {e}"))?;

    std::fs::write(&tmp_path, content).map_err(|e| format!("failed to write temp profile: {e}"))?;

    std::fs::rename(&tmp_path, path).map_err(|e| format!("failed to rename temp profile: {e}"))?;

    Ok(())
}

/// Load a profile from `<dir>/.mosaic/pptx-profiles/<profile_id>.yaml`.
/// Returns `None` if the file doesn't exist.
pub fn load_profile(dir: &std::path::Path, profile_id: &str) -> Option<PptxProfile> {
    let path = dir
        .join(".mosaic")
        .join("pptx-profiles")
        .join(format!("{profile_id}.yaml"));

    let content = std::fs::read_to_string(path).ok()?;
    match serde_yaml::from_str::<PptxProfile>(&content) {
        Ok(profile) => Some(profile),
        Err(e) => {
            eprintln!("  [MC7060] warning: failed to parse profile {profile_id}: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_matcher_equals() {
        let m = TitleMatcher::eq("Meta");
        assert!(m.matches("meta"));
        assert!(!m.matches("metadata"));
    }

    #[test]
    fn test_title_matcher_contains() {
        let m = TitleMatcher::contains_str("Monthly Performance");
        assert!(m.matches("display - monthly performance"));
        assert!(!m.matches("campaign performance"));
    }

    #[test]
    fn test_title_matcher_contains_any() {
        let m = TitleMatcher {
            contains_any: Some(vec![
                "Creative Performance".to_string(),
                "Creative By Name".to_string(),
            ]),
            ..TitleMatcher::eq("")
        };
        assert!(m.matches("display - creative performance"));
        assert!(m.matches("creative by name - details"));
        assert!(!m.matches("monthly performance"));
    }

    #[test]
    fn test_title_matcher_starts_with() {
        let m = TitleMatcher {
            starts_with: Some("Addressable Display".to_string()),
            ..TitleMatcher::eq("")
        };
        assert!(m.matches("addressable display overview"));
        assert!(!m.matches("meta - addressable display"));
    }

    #[test]
    fn test_title_matcher_equals_any() {
        let m = TitleMatcher {
            equals_any: Some(vec!["SEM".to_string(), "Search & Intent Media".to_string()]),
            ..TitleMatcher::eq("")
        };
        assert!(m.matches("sem"));
        assert!(m.matches("search & intent media"));
        assert!(!m.matches("sem stuff"));
    }

    #[test]
    fn test_default_thresholds() {
        let t = MatchThresholds::default();
        assert!((t.auto_match_min_score - 0.30).abs() < 1e-9);
        assert!((t.auto_match_min_margin - 0.05).abs() < 1e-9);
        assert!((t.auto_match_min_relative_margin - 0.20).abs() < 1e-9);
        assert!((t.flag_for_review_min_score - 0.20).abs() < 1e-9);
    }

    #[test]
    fn test_yaml_title_matcher_deser() {
        let yaml = r#"
title_matchers:
  - equals: "Meta"
  - starts_with: "Facebook Overview"
"#;
        #[derive(Debug, Deserialize)]
        struct Wrapper {
            title_matchers: Vec<TitleMatcher>,
        }
        let w: Wrapper = serde_yaml::from_str(yaml).expect("should parse");
        assert_eq!(w.title_matchers.len(), 2);
        assert!(w.title_matchers[0].matches("meta"));
    }

    #[test]
    fn test_yaml_full_profile() {
        let path = std::path::Path::new("demo/sample-data");
        let dirs = [path, std::path::Path::new("../../demo/sample-data")];
        let profile = dirs.iter().find_map(|d| load_profile(d, "lumina-charts"));
        if let Some(p) = profile {
            assert_eq!(p.profile_id, "lumina-charts");
            assert!(!p.sections.is_empty(), "should have sections");
            assert!(!p.table_families.is_empty(), "should have table families");
            eprintln!(
                "  Profile loaded: {} sections, {} families",
                p.sections.len(),
                p.table_families.len()
            );
        } else {
            eprintln!("  [skip] lumina-charts.yaml not found");
        }
    }

    #[test]
    fn test_load_profile_nonexistent() {
        let result = load_profile(std::path::Path::new("/nonexistent"), "foo");
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_reload_profile() {
        let tmp = std::env::temp_dir().join("mosaic-test-save-profile");
        // Clean up from any previous run.
        let _ = std::fs::remove_dir_all(&tmp);

        let profile = PptxProfile {
            schema_version: "2.0".to_string(),
            profile_id: "test-save".to_string(),
            description: "test".to_string(),
            thresholds: MatchThresholds::default(),
            aliases: AliasConfig::default(),
            sections: vec![],
            table_families: vec![],
            skip_tables: vec![],
            duplicate_section_pairs: vec![],
            overrides: vec![OverrideDef {
                slide_index: 5,
                table_index: 0,
                product_name: "Meta".to_string(),
                subproduct_name: "Facebook - Link Click".to_string(),
                table_name: "Monthly Performance".to_string(),
            }],
        };

        save_profile(&tmp, &profile).expect("save should succeed");

        let loaded = load_profile(&tmp, "test-save").expect("reload should find profile");
        assert_eq!(loaded.profile_id, "test-save");
        assert_eq!(loaded.overrides.len(), 1);
        assert_eq!(loaded.overrides[0].slide_index, 5);
        assert_eq!(loaded.overrides[0].product_name, "Meta");

        // Clean up.
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_save_profile_with_positional_skip() {
        let tmp = std::env::temp_dir().join("mosaic-test-save-skip");
        let _ = std::fs::remove_dir_all(&tmp);

        let profile = PptxProfile {
            schema_version: "2.0".to_string(),
            profile_id: "test-skip".to_string(),
            description: "test".to_string(),
            thresholds: MatchThresholds::default(),
            aliases: AliasConfig::default(),
            sections: vec![],
            table_families: vec![],
            skip_tables: vec![SkipRule {
                when: SkipCondition {
                    table_title_contains_any: vec![],
                    slide_title_contains: None,
                    slide_index: Some(12),
                    table_index: Some(0),
                },
                reason: "User skipped".to_string(),
            }],
            duplicate_section_pairs: vec![],
            overrides: vec![],
        };

        save_profile(&tmp, &profile).expect("save should succeed");

        let loaded = load_profile(&tmp, "test-skip").expect("reload should find profile");
        assert_eq!(loaded.skip_tables.len(), 1);
        assert_eq!(loaded.skip_tables[0].when.slide_index, Some(12));
        assert_eq!(loaded.skip_tables[0].when.table_index, Some(0));
        assert_eq!(loaded.skip_tables[0].reason, "User skipped");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
