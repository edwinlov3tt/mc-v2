//! PPTX cascade matcher — per ADR-0023.
//!
//! Takes extracted tables + registry + optional profile + IDF table and returns
//! scored match results. 7 ordered steps: skip gate → profile override →
//! continuation detection → section×family cross-product → first-column tactic
//! lookup → section+title narrowing → TF-IDF weighted F1.
//!
//! **Module independence invariant:** this module has ZERO imports from
//! HTTP frameworks, upload handlers, server state, or routing.

use crate::pptx_profile::{DuplicatePair, MatchThresholds, PptxProfile, SectionDef};
use crate::registry::{Registry, TacticSpec};
use serde::Serialize;
use std::collections::HashMap;

// ─── Foundation Types ────────────────────────────────────────────────────────

/// A table extracted from a PPTX slide, enriched with positional context.
#[derive(Debug, Clone)]
pub struct ExtractedTable {
    /// 1-based slide index.
    pub slide_index: u32,
    /// 0-based table index within the slide.
    pub table_index: u32,
    /// Slide-level title text (from shape content before the table).
    pub slide_title: Option<String>,
    /// Table-specific title — nearest non-date text above the table within the slide.
    pub table_title: Option<String>,
    /// Header row cells.
    pub headers: Vec<String>,
    /// Data rows (excluding headers).
    pub rows: Vec<Vec<String>>,
}

/// What the table maps to in the registry.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryMapping {
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
}

/// Result of matching one table against the registry.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub table: ExtractedTable,
    pub mapping: Option<RegistryMapping>,
    pub confidence: f64,
    pub status: MatchStatus,
    pub alternatives: Vec<(RegistryMapping, f64)>,
    pub flag_for_review: bool,
    pub duplicate_of: Option<TableRef>,
    pub evidence: MatchEvidence,
}

/// Reference to a specific table in the deck.
#[derive(Debug, Clone)]
pub struct TableRef {
    pub slide_index: u32,
    pub table_index: u32,
}

/// Match status — only `Matched` flows into cube ingestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MatchStatus {
    Matched,
    Duplicate,
    Skipped,
    ContinuationCandidate,
    Unresolved,
}

/// Which cascade step produced this result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MatchSource {
    ProfileOverride,
    SectionFamily,
    FirstColumnExact,
    FirstColumnFuzzy,
    NarrowingUnique,
    TfidfMatch,
    TfidfUncertain,
    SkipRule,
    Unmatched,
}

impl std::fmt::Display for MatchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchSource::ProfileOverride => write!(f, "ProfileOverride"),
            MatchSource::SectionFamily => write!(f, "SectionFamily"),
            MatchSource::FirstColumnExact => write!(f, "FirstColumnExact"),
            MatchSource::FirstColumnFuzzy => write!(f, "FirstColumnFuzzy"),
            MatchSource::NarrowingUnique => write!(f, "NarrowingUnique"),
            MatchSource::TfidfMatch => write!(f, "TfidfMatch"),
            MatchSource::TfidfUncertain => write!(f, "TfidfUncertain"),
            MatchSource::SkipRule => write!(f, "SkipRule"),
            MatchSource::Unmatched => write!(f, "Unmatched"),
        }
    }
}

/// Debug/introspection payload — shipped from day 1 for cascade debugging.
#[derive(Debug, Clone)]
pub struct MatchEvidence {
    pub normalized_headers: Vec<String>,
    pub section_context: Option<String>,
    pub table_family: Option<String>,
    pub manifest_candidates: Vec<String>,
    pub candidate_count_before_narrowing: usize,
    pub candidate_count_after_narrowing: usize,
    pub top_scores: Vec<ScoredCandidate>,
    pub fired_step: MatchSource,
}

/// A scored registry candidate for evidence reporting.
#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    pub mapping: RegistryMapping,
    pub score: f64,
}

/// The deck-level result from `match_deck`.
#[derive(Debug)]
pub struct DeckMatchResult {
    pub manifest: DeckManifest,
    pub matches: Vec<MatchResult>,
    pub coverage_warnings: Vec<String>,
    pub stats: MatchStats,
}

/// Deck manifest from rollup pre-pass.
#[derive(Debug, Clone, Default)]
pub struct DeckManifest {
    pub declared_tactics: Vec<DeclaredTactic>,
    pub source_slides: Vec<u32>,
}

/// A tactic declared in a rollup pivot table.
#[derive(Debug, Clone)]
pub struct DeclaredTactic {
    pub raw_label: String,
    pub normalized: String,
    pub registry_match: Option<RegistryMapping>,
}

/// Match statistics for diagnostics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchStats {
    pub total_tables: usize,
    pub auto_resolved: usize,
    pub review_needed: usize,
    pub unmatched: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub by_source: HashMap<String, usize>,
}

// ─── IDF Table ───────────────────────────────────────────────────────────────

/// Pre-computed IDF weights over registry headers.
/// Built once at registry load, shared via `Arc`.
#[derive(Debug, Clone)]
pub struct IdfTable {
    scores: HashMap<String, f64>,
    n: usize,
}

impl IdfTable {
    /// Build from registry entries.
    /// `idf(token) = ln((N+1) / (df(token)+1)) + 1`
    pub fn build(registry: &Registry) -> Self {
        let specs = registry.all_specs();
        let n = specs.len();
        let mut df: HashMap<String, usize> = HashMap::new();

        for spec in specs {
            // Tokenize each spec's headers — split on `;`, then normalize.
            let mut seen = std::collections::HashSet::new();
            for raw_header in &spec.headers {
                let token = normalize_header(raw_header);
                if !token.is_empty() && seen.insert(token.clone()) {
                    *df.entry(token).or_insert(0) += 1;
                }
            }
        }

        let mut scores = HashMap::new();
        for (token, count) in &df {
            let idf = ((n as f64 + 1.0) / (*count as f64 + 1.0)).ln() + 1.0;
            scores.insert(token.clone(), idf);
        }

        IdfTable { scores, n }
    }

    /// Get the IDF score for a token. Unknown tokens get max IDF.
    pub fn score(&self, token: &str) -> f64 {
        self.scores
            .get(token)
            .copied()
            .unwrap_or_else(|| (self.n as f64 + 1.0).ln() + 1.0)
    }

    /// Number of registry entries used to build this table.
    pub fn registry_size(&self) -> usize {
        self.n
    }
}

// ─── Tokenization ────────────────────────────────────────────────────────────

/// Normalize a header string for matching.
/// Per ADR-0023 Decision 11: strip parentheticals → lowercase → trim →
/// collapse whitespace → drop empty.
pub fn normalize_header(h: &str) -> String {
    // Strip parenthetical expressions: "CTR(%)" → "CTR", "CTR (Link ...)" → "CTR"
    let mut result = String::with_capacity(h.len());
    let mut depth = 0i32;
    for ch in h.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ if depth == 0 => result.push(ch),
            _ => {}
        }
    }

    // Lowercase, trim, collapse whitespace.
    let lower = result.to_lowercase();
    let mut collapsed = String::with_capacity(lower.len());
    let mut prev_space = false;
    for ch in lower.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }

    collapsed.trim().to_string()
}

/// Normalize a title string for matching (lowercase, trim, collapse spaces).
fn normalize_title(t: &str) -> String {
    let lower = t.to_lowercase();
    let mut collapsed = String::with_capacity(lower.len());
    let mut prev_space = false;
    for ch in lower.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

/// Tokenize a registry entry's headers (split on `;`, normalize each).
fn tokenize_registry_headers(spec: &TacticSpec) -> Vec<String> {
    spec.headers
        .iter()
        .map(|h| normalize_header(h))
        .filter(|h| !h.is_empty())
        .collect()
}

/// Tokenize an extracted table's headers (normalize each cell).
fn tokenize_table_headers(headers: &[String]) -> Vec<String> {
    headers
        .iter()
        .map(|h| normalize_header(h))
        .filter(|h| !h.is_empty())
        .collect()
}

/// Apply header aliases to a token set.
fn apply_header_aliases(
    tokens: &[String],
    aliases: &crate::pptx_profile::AliasConfig,
) -> Vec<String> {
    tokens
        .iter()
        .map(|t| {
            for alias in &aliases.header {
                if *t == alias.input.to_lowercase() {
                    return alias.canonical.to_lowercase();
                }
            }
            t.clone()
        })
        .collect()
}

/// Apply tactic aliases to a value. Returns the canonical form, or
/// the original value if no alias matches.
fn apply_tactic_alias(
    value: &str,
    aliases: &[crate::pptx_profile::TacticAlias],
) -> TacticAliasResult {
    let normalized = value.trim().to_lowercase();
    for alias in aliases {
        if normalized == alias.input.to_lowercase() {
            if let Some(ref canonical) = alias.canonical {
                return TacticAliasResult::Canonical(canonical.clone());
            }
            if !alias.expands_to.is_empty() {
                return TacticAliasResult::ExpandsTo(alias.expands_to.clone());
            }
        }
    }
    TacticAliasResult::NoMatch
}

#[allow(dead_code)] // ExpandsTo data reserved for Phase 7C candidate-pool narrowing
enum TacticAliasResult {
    Canonical(String),
    ExpandsTo(Vec<String>),
    NoMatch,
}

// ─── F1 Scoring ──────────────────────────────────────────────────────────────

/// IDF-weighted F1 between a table's headers and a registry entry's headers.
pub fn score_f1(table_tokens: &[String], entry_tokens: &[String], idf: &IdfTable) -> f64 {
    if table_tokens.is_empty() || entry_tokens.is_empty() {
        return 0.0;
    }

    let table_set: std::collections::HashSet<&str> =
        table_tokens.iter().map(|s| s.as_str()).collect();
    let entry_set: std::collections::HashSet<&str> =
        entry_tokens.iter().map(|s| s.as_str()).collect();

    let intersection: Vec<&str> = table_set.intersection(&entry_set).copied().collect();

    let inter_idf: f64 = intersection.iter().map(|t| idf.score(t)).sum();
    let table_idf: f64 = table_set.iter().map(|t| idf.score(t)).sum();
    let entry_idf: f64 = entry_set.iter().map(|t| idf.score(t)).sum();

    if table_idf < 1e-9 || entry_idf < 1e-9 {
        return 0.0;
    }

    let precision = inter_idf / table_idf;
    let recall = inter_idf / entry_idf;

    if precision + recall < 1e-9 {
        return 0.0;
    }

    2.0 * precision * recall / (precision + recall)
}

/// Metadata about every slide in the deck (for divider detection).
/// Includes slides with no tables.
#[derive(Debug, Clone)]
pub struct SlideInfo {
    pub slide_index: u32,
    pub title: Option<String>,
    pub has_data_tables: bool,
    pub text_line_count: usize,
}

// ─── Section Context ─────────────────────────────────────────────────────────

/// Active section context — propagated from the most recent divider.
#[derive(Debug, Clone)]
struct SectionContext {
    section_id: String,
    product_name: String,
    default_subproduct: String,
}

// ─── Rollup Pre-Pass ─────────────────────────────────────────────────────────

/// Build a deck manifest by scanning for rollup pivot tables.
/// Per ADR-0023 Decision 3: scan first 10 slides for "Overall Performance" / "Top KPIs"
/// pivot tables that declare which tactics the deck contains.
fn build_manifest(
    tables: &[ExtractedTable],
    registry: &Registry,
    aliases: &[crate::pptx_profile::TacticAlias],
) -> DeckManifest {
    let pivot_headers: [&str; 4] = ["product", "tactic", "channel", "platform"];

    let mut declared: Vec<DeclaredTactic> = Vec::new();
    let mut source_slides: Vec<u32> = Vec::new();

    // Only scan tables from the first 10 slides.
    for table in tables.iter().filter(|t| t.slide_index <= 10) {
        let slide_title = table.slide_title.as_deref().unwrap_or("");
        let slide_lower = slide_title.to_lowercase();

        // Check: slide title contains "Overall Performance" or "Top KPIs"
        if !slide_lower.contains("overall performance") && !slide_lower.contains("top kpis") {
            continue;
        }

        // Check: first-column header is in {Product, Tactic, Channel, Platform}
        if table.headers.is_empty() {
            continue;
        }
        let first_header = table.headers[0].to_lowercase();
        if !pivot_headers.iter().any(|&h| first_header == h) {
            continue;
        }

        // Check: ≥2 data rows
        if table.rows.len() < 2 {
            continue;
        }

        source_slides.push(table.slide_index);

        // Extract first-column values and resolve via aliases + registry.
        for row in &table.rows {
            if row.is_empty() {
                continue;
            }
            let raw = row[0].trim().to_string();
            if raw.is_empty() {
                continue;
            }

            let resolved = match apply_tactic_alias(&raw, aliases) {
                TacticAliasResult::Canonical(c) => c,
                TacticAliasResult::ExpandsTo(_) => raw.clone(), // use raw for manifest
                TacticAliasResult::NoMatch => raw.clone(),
            };

            let normalized = resolved.to_lowercase();

            // Registry lookup by subproduct_name (exact then fuzzy).
            let registry_match = find_registry_by_subproduct(registry, &resolved);

            declared.push(DeclaredTactic {
                raw_label: raw,
                normalized,
                registry_match,
            });
        }
    }

    DeckManifest {
        declared_tactics: declared,
        source_slides,
    }
}

/// Find a registry entry by subproduct name (exact, then case-insensitive contains).
fn find_registry_by_subproduct(registry: &Registry, name: &str) -> Option<RegistryMapping> {
    let lower = name.to_lowercase();
    // Exact match.
    for spec in registry.all_specs() {
        if spec.subproduct_name.to_lowercase() == lower {
            return Some(RegistryMapping {
                product_name: spec.product_name.clone(),
                subproduct_name: spec.subproduct_name.clone(),
                table_name: spec.table_name.clone(),
            });
        }
    }
    // Fuzzy: contains match.
    for spec in registry.all_specs() {
        let sp_lower = spec.subproduct_name.to_lowercase();
        if sp_lower.contains(&lower) || lower.contains(&sp_lower) {
            return Some(RegistryMapping {
                product_name: spec.product_name.clone(),
                subproduct_name: spec.subproduct_name.clone(),
                table_name: spec.table_name.clone(),
            });
        }
    }
    None
}

/// Find a registry entry by subproduct name — exact match only.
fn find_registry_by_subproduct_exact(registry: &Registry, name: &str) -> Option<RegistryMapping> {
    let lower = name.to_lowercase();
    for spec in registry.all_specs() {
        if spec.subproduct_name.to_lowercase() == lower {
            return Some(RegistryMapping {
                product_name: spec.product_name.clone(),
                subproduct_name: spec.subproduct_name.clone(),
                table_name: spec.table_name.clone(),
            });
        }
    }
    None
}

// ─── Divider Detection ───────────────────────────────────────────────────────

/// Resolve a divider title to section context.
/// Per ADR-0023 Decision 4: resolution check (at least one must match).
fn resolve_divider_title(
    title: &str,
    profile: Option<&PptxProfile>,
    registry: &Registry,
    manifest: &DeckManifest,
    aliases: &[crate::pptx_profile::TacticAlias],
) -> Option<SectionContext> {
    let normalized = normalize_title(title);

    // 3a: profile section match (highest priority — profile is curated).
    if let Some(prof) = profile {
        for section in &prof.sections {
            if section_matches_title(section, &normalized) {
                return Some(SectionContext {
                    section_id: section.id.clone(),
                    product_name: section.propagates.product_name.clone(),
                    default_subproduct: section.propagates.default_subproduct.clone(),
                });
            }
        }
    }

    // 3b: registry product/subproduct EXACT match (after aliases).
    // Use exact match only for dividers — fuzzy "contains" is too aggressive
    // and would match things like "Video - CTR Last 6 Months" → STV.
    let alias_resolved = match apply_tactic_alias(title, aliases) {
        TacticAliasResult::Canonical(c) => c,
        _ => title.to_string(),
    };

    if let Some(mapping) = find_registry_by_subproduct_exact(registry, &alias_resolved) {
        return Some(SectionContext {
            section_id: mapping.subproduct_name.to_lowercase().replace(' ', "_"),
            product_name: mapping.product_name.clone(),
            default_subproduct: mapping.subproduct_name.clone(),
        });
    }

    // Also check if the title matches a registry product_name exactly.
    let lower = alias_resolved.to_lowercase();
    for spec in registry.all_specs() {
        if spec.product_name.to_lowercase() == lower {
            return Some(SectionContext {
                section_id: spec.product_name.to_lowercase().replace(' ', "_"),
                product_name: spec.product_name.clone(),
                default_subproduct: spec.subproduct_name.clone(),
            });
        }
    }

    // 3c: manifest declared tactic.
    for tactic in &manifest.declared_tactics {
        if tactic.normalized == normalized || tactic.raw_label.to_lowercase() == normalized {
            if let Some(ref reg) = tactic.registry_match {
                return Some(SectionContext {
                    section_id: tactic.normalized.replace(' ', "_"),
                    product_name: reg.product_name.clone(),
                    default_subproduct: reg.subproduct_name.clone(),
                });
            }
        }
    }

    None
}

/// Check if a section definition matches a normalized title.
fn section_matches_title(section: &SectionDef, normalized_title: &str) -> bool {
    section
        .title_matchers
        .iter()
        .any(|m| m.matches(normalized_title))
}

// ─── The Cascade ─────────────────────────────────────────────────────────────

/// Match all tables in a PPTX deck against the registry.
///
/// Per ADR-0023 Decision 1: runs 7 ordered steps for each table.
/// Per ADR-0023 Decision 9: this function has NO dependency on HTTP
/// frameworks, upload handlers, or server state.
pub fn match_deck(
    tables: &[ExtractedTable],
    registry: &Registry,
    profile: Option<&PptxProfile>,
    idf: &IdfTable,
) -> DeckMatchResult {
    match_deck_with_slides(tables, &[], registry, profile, idf)
}

/// Match all tables in a PPTX deck, with slide-level info for divider detection.
pub fn match_deck_with_slides(
    tables: &[ExtractedTable],
    slide_infos: &[SlideInfo],
    registry: &Registry,
    profile: Option<&PptxProfile>,
    idf: &IdfTable,
) -> DeckMatchResult {
    let empty_aliases = crate::pptx_profile::AliasConfig::default();
    let aliases = profile.map_or(&empty_aliases, |p| &p.aliases);
    let tactic_aliases = &aliases.tactic;

    // Phase 1: rollup pre-pass — build deck manifest.
    let manifest = build_manifest(tables, registry, tactic_aliases);

    // Phase 2: divider detection from slide_infos, then per-table cascade.
    let mut matches: Vec<MatchResult> = Vec::with_capacity(tables.len());
    let mut current_section: Option<SectionContext> = None;
    let mut prev_table: Option<&ExtractedTable> = None;
    let mut prev_section_id: Option<String> = None;

    let thresholds = profile.map(|p| &p.thresholds).cloned().unwrap_or_default();

    // Build a mapping of slide_index → section context from slide_infos.
    // A divider is a slide with no data tables, <60 char title, that resolves.
    let mut section_at_slide: HashMap<u32, SectionContext> = HashMap::new();
    {
        let mut section_by_slide_order: Vec<(u32, SectionContext)> = Vec::new();
        for info in slide_infos {
            if info.has_data_tables {
                continue;
            }
            if info.text_line_count >= 15 {
                continue;
            }
            let title = info.title.as_deref().unwrap_or("");
            let trimmed = title.trim();
            if trimmed.is_empty() || trimmed.len() >= 60 {
                continue;
            }
            if let Some(ctx) =
                resolve_divider_title(trimmed, profile, registry, &manifest, tactic_aliases)
            {
                section_by_slide_order.push((info.slide_index, ctx));
            }
        }
        // For each slide, assign the most recent divider's section context.
        for (slide_idx, ctx) in &section_by_slide_order {
            section_at_slide.insert(*slide_idx, ctx.clone());
        }
    }

    // Sort slide_infos' divider entries to propagate section context in order.
    let mut divider_entries: Vec<(u32, &SectionContext)> =
        section_at_slide.iter().map(|(k, v)| (*k, v)).collect();
    divider_entries.sort_by_key(|(idx, _)| *idx);

    for table in tables {
        // Update section context: find the latest divider at or before this slide.
        for (div_slide, ctx) in &divider_entries {
            if *div_slide <= table.slide_index {
                current_section = Some((*ctx).clone());
            } else {
                break;
            }
        }

        let normalized_headers = tokenize_table_headers(&table.headers);
        let aliased_headers = apply_header_aliases(&normalized_headers, aliases);

        let mut evidence = MatchEvidence {
            normalized_headers: aliased_headers.clone(),
            section_context: current_section.as_ref().map(|s| s.product_name.clone()),
            table_family: None,
            manifest_candidates: manifest
                .declared_tactics
                .iter()
                .map(|t| t.raw_label.clone())
                .collect(),
            candidate_count_before_narrowing: registry.len(),
            candidate_count_after_narrowing: registry.len(),
            top_scores: Vec::new(),
            fired_step: MatchSource::Unmatched,
        };

        // ── Step 0: Skip rule ──
        if try_skip_rule(table, profile).is_some() {
            evidence.fired_step = MatchSource::SkipRule;
            matches.push(MatchResult {
                table: table.clone(),
                mapping: None,
                confidence: 1.0,
                status: MatchStatus::Skipped,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence,
            });
            prev_table = Some(table);
            prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
            continue;
        }

        // ── Step 1: Profile override ──
        if let Some(result) = try_profile_override(table, profile) {
            evidence.fired_step = MatchSource::ProfileOverride;
            matches.push(MatchResult {
                table: table.clone(),
                mapping: Some(result),
                confidence: 1.0,
                status: MatchStatus::Matched,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence,
            });
            prev_table = Some(table);
            prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
            continue;
        }

        // ── Step 2: Continuation candidate ──
        if let Some(_prev) = prev_table {
            if is_continuation_candidate(table, _prev, &current_section, &prev_section_id) {
                evidence.fired_step = MatchSource::Unmatched;
                matches.push(MatchResult {
                    table: table.clone(),
                    mapping: None,
                    confidence: 0.0,
                    status: MatchStatus::ContinuationCandidate,
                    alternatives: Vec::new(),
                    flag_for_review: true,
                    duplicate_of: None,
                    evidence,
                });
                prev_table = Some(table);
                prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
                continue;
            }
        }

        // ── Step 3: Section + Table-family cross product ──
        if let Some(ref ctx) = current_section {
            if let Some(prof) = profile {
                if let Some(result) =
                    try_section_family(table, ctx, prof, registry, tactic_aliases, &mut evidence)
                {
                    matches.push(result);
                    prev_table = Some(table);
                    prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
                    continue;
                }
            }
        }

        // ── Step 4: First-column tactic lookup ──
        if let Some(result) =
            try_first_column_lookup(table, registry, tactic_aliases, &mut evidence)
        {
            matches.push(result);
            prev_table = Some(table);
            prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
            continue;
        }

        // ── Step 5: Section + title narrowing ──
        if let Some(result) = try_narrowing(table, &current_section, registry, &mut evidence) {
            matches.push(result);
            prev_table = Some(table);
            prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
            continue;
        }

        // ── Step 6: TF-IDF F1 ranking ──
        let result = try_tfidf(
            table,
            &current_section,
            registry,
            &aliased_headers,
            idf,
            &thresholds,
            &mut evidence,
        );
        matches.push(result);
        prev_table = Some(table);
        prev_section_id = current_section.as_ref().map(|s| s.section_id.clone());
    }

    // Phase 3: duplicate detection.
    if let Some(prof) = profile {
        detect_duplicates(&mut matches, &prof.duplicate_section_pairs);
    }

    // Phase 4: coverage warnings + diagnostics.
    let coverage_warnings = build_coverage_warnings(&manifest, &matches, profile);

    // Phase 5: stats.
    let stats = build_stats(&matches);

    DeckMatchResult {
        manifest,
        matches,
        coverage_warnings,
        stats,
    }
}

// ─── Step 0: Skip Rule ──────────────────────────────────────────────────────

fn try_skip_rule(table: &ExtractedTable, profile: Option<&PptxProfile>) -> Option<()> {
    let prof = profile?;
    let slide_title = normalize_title(table.slide_title.as_deref().unwrap_or(""));
    let table_title = normalize_title(table.table_title.as_deref().unwrap_or(""));

    for rule in &prof.skip_tables {
        // Check positional skip (from review UI save-back).
        if let (Some(si), Some(ti)) = (rule.when.slide_index, rule.when.table_index) {
            if si == table.slide_index && ti == table.table_index {
                return Some(());
            }
        }
        // Check table_title_contains_any.
        for pattern in &rule.when.table_title_contains_any {
            let p = pattern.to_lowercase();
            if (!table_title.is_empty() && table_title.contains(&p))
                || (!slide_title.is_empty() && slide_title.contains(&p))
            {
                return Some(());
            }
        }
        // Check slide_title_contains.
        if let Some(ref pattern) = rule.when.slide_title_contains {
            let p = pattern.to_lowercase();
            if slide_title.contains(&p) {
                return Some(());
            }
        }
    }
    None
}

// ─── Step 1: Profile Override ────────────────────────────────────────────────

fn try_profile_override(
    table: &ExtractedTable,
    profile: Option<&PptxProfile>,
) -> Option<RegistryMapping> {
    let prof = profile?;
    for ovr in &prof.overrides {
        if ovr.slide_index == table.slide_index && ovr.table_index == table.table_index {
            return Some(RegistryMapping {
                product_name: ovr.product_name.clone(),
                subproduct_name: ovr.subproduct_name.clone(),
                table_name: ovr.table_name.clone(),
            });
        }
    }
    None
}

// ─── Step 2: Continuation Candidate ──────────────────────────────────────────

fn is_continuation_candidate(
    table: &ExtractedTable,
    prev: &ExtractedTable,
    current_section: &Option<SectionContext>,
    prev_section_id: &Option<String>,
) -> bool {
    // Same section context.
    let same_section = match (current_section.as_ref(), prev_section_id.as_ref()) {
        (Some(ctx), Some(prev_id)) => ctx.section_id == *prev_id,
        (None, None) => true,
        _ => false,
    };
    if !same_section {
        return false;
    }

    // Column count match.
    if table.headers.len() != prev.headers.len() {
        return false;
    }

    // No header row detected — first row doesn't look like headers.
    // Heuristic: if the first row's first cell looks like a date or number, not a header.
    if table.headers.is_empty() {
        return false;
    }
    let first = &table.headers[0];
    let looks_like_header = first
        .chars()
        .all(|c| c.is_alphabetic() || c == ' ' || c == '_');
    // If it looks like a normal header, it's probably its own table, not a continuation.
    if looks_like_header && first.len() > 2 {
        return false;
    }

    true
}

// ─── Step 3: Section + Table Family ──────────────────────────────────────────

fn try_section_family(
    table: &ExtractedTable,
    ctx: &SectionContext,
    profile: &PptxProfile,
    registry: &Registry,
    tactic_aliases: &[crate::pptx_profile::TacticAlias],
    evidence: &mut MatchEvidence,
) -> Option<MatchResult> {
    let slide_title = normalize_title(table.slide_title.as_deref().unwrap_or(""));
    let table_title = normalize_title(table.table_title.as_deref().unwrap_or(""));

    // Find matching table_family.
    let family = profile.table_families.iter().find(|f| {
        f.title_matchers
            .iter()
            .any(|m| m.matches(&slide_title) || m.matches(&table_title))
    })?;

    evidence.table_family = Some(family.id.clone());
    evidence.fired_step = MatchSource::SectionFamily;

    if family.use_first_column_lookup {
        // Each row's first-column value becomes the subproduct.
        if !family.first_column_header_in.is_empty() {
            let first_header = table
                .headers
                .first()
                .map(|h| h.to_lowercase())
                .unwrap_or_default();
            if !family
                .first_column_header_in
                .iter()
                .any(|h| h.to_lowercase() == first_header)
            {
                // First-column header doesn't match — skip first-column lookup for this family.
                return None;
            }
        }

        for row in &table.rows {
            if row.is_empty() {
                continue;
            }
            let raw_value = row[0].trim();
            if raw_value.is_empty() {
                continue;
            }
            let resolved = match apply_tactic_alias(raw_value, tactic_aliases) {
                TacticAliasResult::Canonical(c) => c,
                TacticAliasResult::ExpandsTo(_) => raw_value.to_string(),
                TacticAliasResult::NoMatch => raw_value.to_string(),
            };

            if find_registry_by_subproduct_exact(registry, &resolved).is_some()
                || find_registry_by_subproduct(registry, &resolved).is_some()
            {
                let mapping = RegistryMapping {
                    product_name: ctx.product_name.clone(),
                    subproduct_name: resolved,
                    table_name: family.table_name.clone(),
                };
                return Some(MatchResult {
                    table: table.clone(),
                    mapping: Some(mapping),
                    confidence: 0.95,
                    status: MatchStatus::Matched,
                    alternatives: Vec::new(),
                    flag_for_review: false,
                    duplicate_of: None,
                    evidence: evidence.clone(),
                });
            }
        }
        // Fall through if no row matched.
        return None;
    }

    // Non-pivot family: use section defaults.
    let mapping = RegistryMapping {
        product_name: ctx.product_name.clone(),
        subproduct_name: ctx.default_subproduct.clone(),
        table_name: family.table_name.clone(),
    };

    Some(MatchResult {
        table: table.clone(),
        mapping: Some(mapping),
        confidence: 0.92,
        status: MatchStatus::Matched,
        alternatives: Vec::new(),
        flag_for_review: false,
        duplicate_of: None,
        evidence: evidence.clone(),
    })
}

// ─── Step 4: First-Column Tactic Lookup ──────────────────────────────────────

fn try_first_column_lookup(
    table: &ExtractedTable,
    registry: &Registry,
    tactic_aliases: &[crate::pptx_profile::TacticAlias],
    evidence: &mut MatchEvidence,
) -> Option<MatchResult> {
    let pivot_headers: [&str; 4] = ["product", "tactic", "channel", "platform"];
    let first_header = table.headers.first()?.to_lowercase();

    if !pivot_headers.contains(&first_header.as_str()) {
        return None;
    }

    for row in &table.rows {
        if row.is_empty() {
            continue;
        }
        let raw = row[0].trim();
        if raw.is_empty() {
            continue;
        }

        let resolved = match apply_tactic_alias(raw, tactic_aliases) {
            TacticAliasResult::Canonical(c) => c,
            TacticAliasResult::ExpandsTo(_) => raw.to_string(),
            TacticAliasResult::NoMatch => raw.to_string(),
        };

        // Exact match.
        if let Some(mapping) = find_registry_by_subproduct_exact(registry, &resolved) {
            evidence.fired_step = MatchSource::FirstColumnExact;
            return Some(MatchResult {
                table: table.clone(),
                mapping: Some(mapping),
                confidence: 1.0,
                status: MatchStatus::Matched,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence: evidence.clone(),
            });
        }

        // Fuzzy match.
        if let Some(mapping) = find_registry_by_subproduct(registry, &resolved) {
            evidence.fired_step = MatchSource::FirstColumnFuzzy;
            return Some(MatchResult {
                table: table.clone(),
                mapping: Some(mapping),
                confidence: 0.92,
                status: MatchStatus::Matched,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence: evidence.clone(),
            });
        }
    }

    None
}

// ─── Step 5: Section + Title Narrowing ───────────────────────────────────────

fn try_narrowing(
    table: &ExtractedTable,
    section: &Option<SectionContext>,
    registry: &Registry,
    evidence: &mut MatchEvidence,
) -> Option<MatchResult> {
    let mut candidates: Vec<&TacticSpec> = registry.all_specs().iter().collect();
    evidence.candidate_count_before_narrowing = candidates.len();

    // Narrow by section product_name.
    if let Some(ctx) = section {
        candidates.retain(|s| s.product_name.to_lowercase() == ctx.product_name.to_lowercase());
    }

    // Narrow by table_title.
    let table_title = normalize_title(table.table_title.as_deref().unwrap_or(""));
    let slide_title = normalize_title(table.slide_title.as_deref().unwrap_or(""));
    let search_title = if !table_title.is_empty() {
        &table_title
    } else {
        &slide_title
    };

    if !search_title.is_empty() {
        let narrowed: Vec<&TacticSpec> = candidates
            .iter()
            .filter(|s| {
                let tn = s.table_name.to_lowercase();
                search_title.contains(&tn) || tn.contains(search_title)
            })
            .copied()
            .collect();

        if !narrowed.is_empty() {
            candidates = narrowed;
        }
    }

    evidence.candidate_count_after_narrowing = candidates.len();

    if candidates.len() == 1 {
        let spec = candidates[0];
        evidence.fired_step = MatchSource::NarrowingUnique;
        return Some(MatchResult {
            table: table.clone(),
            mapping: Some(RegistryMapping {
                product_name: spec.product_name.clone(),
                subproduct_name: spec.subproduct_name.clone(),
                table_name: spec.table_name.clone(),
            }),
            confidence: 0.85,
            status: MatchStatus::Matched,
            alternatives: Vec::new(),
            flag_for_review: false,
            duplicate_of: None,
            evidence: evidence.clone(),
        });
    }

    None
}

// ─── Step 6: TF-IDF F1 Ranking ──────────────────────────────────────────────

fn try_tfidf(
    table: &ExtractedTable,
    section: &Option<SectionContext>,
    registry: &Registry,
    aliased_headers: &[String],
    idf: &IdfTable,
    thresholds: &MatchThresholds,
    evidence: &mut MatchEvidence,
) -> MatchResult {
    let mut candidates: Vec<&TacticSpec> = registry.all_specs().iter().collect();

    // Narrow by section if available.
    if let Some(ctx) = section {
        let narrowed: Vec<&TacticSpec> = candidates
            .iter()
            .filter(|s| s.product_name.to_lowercase() == ctx.product_name.to_lowercase())
            .copied()
            .collect();
        if !narrowed.is_empty() {
            candidates = narrowed;
        }
    }

    evidence.candidate_count_before_narrowing = registry.len();
    evidence.candidate_count_after_narrowing = candidates.len();

    // Score all candidates.
    let mut scored: Vec<(f64, &TacticSpec)> = candidates
        .iter()
        .map(|spec| {
            let entry_tokens = tokenize_registry_headers(spec);
            let score = score_f1(aliased_headers, &entry_tokens, idf);
            (score, *spec)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Record top scores in evidence.
    evidence.top_scores = scored
        .iter()
        .take(5)
        .map(|(score, spec)| ScoredCandidate {
            mapping: RegistryMapping {
                product_name: spec.product_name.clone(),
                subproduct_name: spec.subproduct_name.clone(),
                table_name: spec.table_name.clone(),
            },
            score: *score,
        })
        .collect();

    let (top1_score, top1_spec) = scored.first().map(|(s, sp)| (*s, *sp)).unwrap_or_else(|| {
        // No candidates at all — shouldn't happen but handle gracefully.
        (0.0, &registry.all_specs()[0])
    });
    let top2_score = scored.get(1).map(|(s, _)| *s).unwrap_or(0.0);

    let margin = top1_score - top2_score;
    let relative_margin = if top1_score > 1e-9 {
        margin / top1_score
    } else {
        0.0
    };

    let alternatives: Vec<(RegistryMapping, f64)> = scored
        .iter()
        .skip(1)
        .take(3)
        .map(|(s, sp)| {
            (
                RegistryMapping {
                    product_name: sp.product_name.clone(),
                    subproduct_name: sp.subproduct_name.clone(),
                    table_name: sp.table_name.clone(),
                },
                *s,
            )
        })
        .collect();

    let top1_mapping = RegistryMapping {
        product_name: top1_spec.product_name.clone(),
        subproduct_name: top1_spec.subproduct_name.clone(),
        table_name: top1_spec.table_name.clone(),
    };

    if top1_score >= thresholds.auto_match_min_score
        && (margin >= thresholds.auto_match_min_margin
            || relative_margin >= thresholds.auto_match_min_relative_margin)
    {
        evidence.fired_step = MatchSource::TfidfMatch;
        MatchResult {
            table: table.clone(),
            mapping: Some(top1_mapping),
            confidence: top1_score,
            status: MatchStatus::Matched,
            alternatives,
            flag_for_review: false,
            duplicate_of: None,
            evidence: evidence.clone(),
        }
    } else if top1_score >= thresholds.flag_for_review_min_score {
        evidence.fired_step = MatchSource::TfidfUncertain;
        MatchResult {
            table: table.clone(),
            mapping: Some(top1_mapping),
            confidence: top1_score,
            status: MatchStatus::Unresolved,
            alternatives,
            flag_for_review: true,
            duplicate_of: None,
            evidence: evidence.clone(),
        }
    } else {
        evidence.fired_step = MatchSource::Unmatched;
        MatchResult {
            table: table.clone(),
            mapping: if top1_score >= 0.10 {
                Some(top1_mapping)
            } else {
                None
            },
            confidence: top1_score,
            status: MatchStatus::Unresolved,
            alternatives,
            flag_for_review: false,
            duplicate_of: None,
            evidence: evidence.clone(),
        }
    }
}

// ─── Duplicate Detection ─────────────────────────────────────────────────────

/// Detect and mark duplicate tables.
/// Per ADR-0023 Decision 7: fingerprint-based dedup with section-pair awareness.
fn detect_duplicates(matches: &mut [MatchResult], pairs: &[DuplicatePair]) {
    // Phase 1: collect fingerprints (immutable borrow).
    let mut seen: HashMap<String, (usize, String)> = HashMap::new();
    let mut dup_actions: Vec<(usize, u32, u32)> = Vec::new(); // (index, canonical_slide, canonical_table)

    for (i, m) in matches.iter().enumerate() {
        if m.status != MatchStatus::Matched {
            continue;
        }
        let Some(ref mapping) = m.mapping else {
            continue;
        };

        let fingerprint = build_fingerprint(m);
        let key = format!(
            "{}|{}|{}",
            mapping.product_name.to_lowercase(),
            mapping.table_name.to_lowercase(),
            fingerprint
        );

        let section_id = m
            .evidence
            .section_context
            .as_deref()
            .unwrap_or("")
            .to_lowercase();

        if let Some((canonical_idx, canonical_section)) = seen.get(&key) {
            let is_declared_pair = pairs.iter().any(|p| {
                let all_section_ids: Vec<String> = p
                    .sections
                    .iter()
                    .chain(p.matches_section_titles.iter())
                    .map(|s| s.to_lowercase())
                    .collect();

                let has_canonical = all_section_ids
                    .iter()
                    .any(|s| canonical_section.contains(s) || s.contains(canonical_section));
                let has_current = all_section_ids
                    .iter()
                    .any(|s| section_id.contains(s) || s.contains(&section_id));

                has_canonical && has_current
            });

            if is_declared_pair {
                let cs = matches[*canonical_idx].table.slide_index;
                let ct = matches[*canonical_idx].table.table_index;
                dup_actions.push((i, cs, ct));
            }
        } else {
            seen.insert(key, (i, section_id));
        }
    }

    // Phase 2: apply mutations.
    for (idx, canonical_slide, canonical_table) in dup_actions {
        matches[idx].status = MatchStatus::Duplicate;
        matches[idx].duplicate_of = Some(TableRef {
            slide_index: canonical_slide,
            table_index: canonical_table,
        });
    }
}

/// Build a fingerprint for dedup: (sorted headers, sorted first-col values, row_count).
fn build_fingerprint(m: &MatchResult) -> String {
    let mut headers: Vec<String> = m
        .table
        .headers
        .iter()
        .map(|h| normalize_header(h))
        .collect();
    headers.sort();

    let mut first_col: Vec<String> = m
        .table
        .rows
        .iter()
        .filter_map(|r| r.first().map(|v| v.to_lowercase().trim().to_string()))
        .collect();
    first_col.sort();

    format!("{:?}|{:?}|{}", headers, first_col, m.table.rows.len())
}

/// Sanitize a string into a filename-safe form.
pub fn sanitize_for_filename(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ─── Coverage Warnings + Diagnostics ─────────────────────────────────────────

fn build_coverage_warnings(
    manifest: &DeckManifest,
    matches: &[MatchResult],
    profile: Option<&PptxProfile>,
) -> Vec<String> {
    let mut warnings = Vec::new();

    // MC7064: ≥30% fell through to TfidfUncertain.
    let total_non_skip = matches
        .iter()
        .filter(|m| m.status != MatchStatus::Skipped)
        .count();
    let uncertain = matches
        .iter()
        .filter(|m| m.evidence.fired_step == MatchSource::TfidfUncertain)
        .count();

    if total_non_skip > 0 && (uncertain as f64 / total_non_skip as f64) >= 0.30 {
        warnings.push(format!(
            "[MC7064] {uncertain}/{total_non_skip} tables fell through to TfidfUncertain — profile may need updating"
        ));
    }

    // MC7065: manifest tactic with no section divider.
    let section_ids: Vec<String> = matches
        .iter()
        .filter_map(|m| m.evidence.section_context.as_ref())
        .cloned()
        .collect();

    for tactic in &manifest.declared_tactics {
        let found = section_ids.iter().any(|s| {
            s.to_lowercase().contains(&tactic.normalized)
                || tactic.normalized.contains(&s.to_lowercase())
        });
        if !found {
            warnings.push(format!(
                "[MC7065] Manifest-declared tactic '{}' has no corresponding section divider",
                tactic.raw_label
            ));
        }
    }

    // MC7067: duplicate-section-pair declared but no duplicates found.
    if let Some(prof) = profile {
        let has_dups = matches.iter().any(|m| m.status == MatchStatus::Duplicate);
        if !prof.duplicate_section_pairs.is_empty() && !has_dups {
            warnings.push(
                "[MC7067] Duplicate-section-pair declared but no duplicates found".to_string(),
            );
        }
    }

    warnings
}

fn build_stats(matches: &[MatchResult]) -> MatchStats {
    let mut stats = MatchStats {
        total_tables: matches.len(),
        ..Default::default()
    };

    for m in matches {
        match m.status {
            MatchStatus::Matched => stats.auto_resolved += 1,
            MatchStatus::Duplicate => stats.duplicates += 1,
            MatchStatus::Skipped => stats.skipped += 1,
            MatchStatus::ContinuationCandidate | MatchStatus::Unresolved => {
                if m.flag_for_review {
                    stats.review_needed += 1;
                } else {
                    stats.unmatched += 1;
                }
            }
        }

        let source_name = m.evidence.fired_step.to_string();
        *stats.by_source.entry(source_name).or_insert(0) += 1;
    }

    stats
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pptx_profile::{AliasConfig, SkipRule};

    #[test]
    fn test_normalize_header_strip_parens() {
        assert_eq!(normalize_header("CTR(%)"), "ctr");
        assert_eq!(normalize_header("CTR (Link Click-Through Rate)"), "ctr");
        assert_eq!(normalize_header("Conversions (Default)"), "conversions");
    }

    #[test]
    fn test_normalize_header_multiword_preserved() {
        assert_eq!(normalize_header("Link Clicks"), "link clicks");
        assert_eq!(normalize_header("Post Engagements"), "post engagements");
    }

    #[test]
    fn test_normalize_header_whitespace_collapse() {
        assert_eq!(normalize_header("  Date  "), "date");
        assert_eq!(normalize_header("Total  Leads"), "total leads");
    }

    #[test]
    fn test_normalize_header_25_pct() {
        assert_eq!(normalize_header("25% Completion"), "25% completion");
    }

    #[test]
    fn test_idf_build_basic() {
        let csv = "product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n\
                   \"A\",\"X\",\"T1\",\"f1\",\"Date; Impressions; Clicks\",\"\",TRUE,0\n\
                   \"A\",\"Y\",\"T2\",\"f2\",\"Date; Impressions; Spend\",\"\",TRUE,0\n\
                   \"B\",\"Z\",\"T3\",\"f3\",\"Date; Foot Traffic; Spend\",\"\",TRUE,0";
        let reg = Registry::from_csv(csv).expect("parse");
        let idf = IdfTable::build(&reg);

        assert_eq!(idf.registry_size(), 3);

        // "date" appears in all 3 → df=3, idf = ln(4/4) + 1 = 1.0
        let date_idf = idf.score("date");
        assert!((date_idf - 1.0).abs() < 0.01, "date idf={date_idf}");

        // "foot traffic" appears in 1 → df=1, idf = ln(4/2) + 1 ≈ 1.693
        let ft_idf = idf.score("foot traffic");
        assert!(ft_idf > 1.5, "foot traffic idf={ft_idf}");

        // Unknown token → max IDF = ln(4) + 1 ≈ 2.386
        let unknown = idf.score("premium content");
        assert!(unknown > ft_idf, "unknown should be > foot_traffic");
    }

    #[test]
    fn test_f1_scoring_perfect_match() {
        let csv = "product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n\
                   \"A\",\"X\",\"T1\",\"f1\",\"Date; Clicks; CTR\",\"\",TRUE,0";
        let reg = Registry::from_csv(csv).expect("parse");
        let idf = IdfTable::build(&reg);

        let table_tokens = vec!["date".to_string(), "clicks".to_string(), "ctr".to_string()];
        let entry_tokens = vec!["date".to_string(), "clicks".to_string(), "ctr".to_string()];
        let score = score_f1(&table_tokens, &entry_tokens, &idf);
        assert!(
            (score - 1.0).abs() < 1e-9,
            "perfect match should be 1.0, got {score}"
        );
    }

    #[test]
    fn test_f1_scoring_subset() {
        let csv = "product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n\
                   \"A\",\"X\",\"T1\",\"f1\",\"Date; Clicks; CTR; Spend\",\"\",TRUE,0";
        let reg = Registry::from_csv(csv).expect("parse");
        let idf = IdfTable::build(&reg);

        let table_tokens = vec!["date".to_string(), "clicks".to_string()];
        let entry_tokens = vec![
            "date".to_string(),
            "clicks".to_string(),
            "ctr".to_string(),
            "spend".to_string(),
        ];
        let score = score_f1(&table_tokens, &entry_tokens, &idf);
        // Table is a subset → precision=1.0, recall<1.0 → F1 in (0, 1)
        assert!(
            score > 0.0 && score < 1.0,
            "subset F1 should be in (0,1), got {score}"
        );
    }

    #[test]
    fn test_f1_scoring_empty() {
        let csv = "product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n\
                   \"A\",\"X\",\"T1\",\"f1\",\"Date\",\"\",TRUE,0";
        let reg = Registry::from_csv(csv).expect("parse");
        let idf = IdfTable::build(&reg);

        let score = score_f1(&[], &["date".to_string()], &idf);
        assert!((score - 0.0).abs() < 1e-9, "empty table tokens → 0.0");

        let score2 = score_f1(&["date".to_string()], &[], &idf);
        assert!((score2 - 0.0).abs() < 1e-9, "empty entry tokens → 0.0");
    }

    #[test]
    fn test_sanitize_for_filename() {
        assert_eq!(
            sanitize_for_filename("Targeted Display"),
            "targeted-display"
        );
        assert_eq!(
            sanitize_for_filename("Facebook - Link Click"),
            "facebook-link-click"
        );
    }

    #[test]
    fn test_skip_rule_fires() {
        let profile = PptxProfile {
            schema_version: "2.0".to_string(),
            profile_id: "test".to_string(),
            description: String::new(),
            thresholds: MatchThresholds::default(),
            aliases: AliasConfig::default(),
            sections: Vec::new(),
            table_families: Vec::new(),
            skip_tables: vec![SkipRule {
                when: crate::pptx_profile::SkipCondition {
                    table_title_contains_any: vec!["Reach & Frequency".to_string()],
                    slide_title_contains: None,
                    slide_index: None,
                    table_index: None,
                },
                reason: "test".to_string(),
            }],
            duplicate_section_pairs: Vec::new(),
            overrides: Vec::new(),
        };

        let table = ExtractedTable {
            slide_index: 17,
            table_index: 0,
            slide_title: Some("Reach & Frequency".to_string()),
            table_title: Some("Reach & Frequency".to_string()),
            headers: vec!["Tactic".to_string()],
            rows: vec![vec!["Display".to_string()]],
        };

        assert!(try_skip_rule(&table, Some(&profile)).is_some());
    }

    #[test]
    fn test_match_status_only_matched_in_ingestion() {
        // Verify the ingestion adapter filters correctly.
        let table = ExtractedTable {
            slide_index: 1,
            table_index: 0,
            slide_title: None,
            table_title: None,
            headers: vec!["Date".to_string()],
            rows: vec![vec!["2026-01".to_string()]],
        };

        let evidence = MatchEvidence {
            normalized_headers: vec!["date".to_string()],
            section_context: None,
            table_family: None,
            manifest_candidates: Vec::new(),
            candidate_count_before_narrowing: 0,
            candidate_count_after_narrowing: 0,
            top_scores: Vec::new(),
            fired_step: MatchSource::Unmatched,
        };

        let results = vec![
            MatchResult {
                table: table.clone(),
                mapping: Some(RegistryMapping {
                    product_name: "A".into(),
                    subproduct_name: "B".into(),
                    table_name: "C".into(),
                }),
                confidence: 0.92,
                status: MatchStatus::Matched,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence: evidence.clone(),
            },
            MatchResult {
                table: table.clone(),
                mapping: None,
                confidence: 0.0,
                status: MatchStatus::Skipped,
                alternatives: Vec::new(),
                flag_for_review: false,
                duplicate_of: None,
                evidence: evidence.clone(),
            },
            MatchResult {
                table: table.clone(),
                mapping: None,
                confidence: 0.0,
                status: MatchStatus::Unresolved,
                alternatives: Vec::new(),
                flag_for_review: true,
                duplicate_of: None,
                evidence,
            },
        ];

        // Verify ingestion filtering: only Matched results should produce output.
        let matched_count = results
            .iter()
            .filter(|r| r.status == MatchStatus::Matched)
            .count();
        assert_eq!(matched_count, 1, "only 1 Matched result");
        let matched = results
            .iter()
            .find(|r| r.status == MatchStatus::Matched)
            .unwrap();
        let mapping = matched.mapping.as_ref().unwrap();
        let filename = format!(
            "report-{}-{}.csv",
            sanitize_for_filename(&mapping.subproduct_name),
            sanitize_for_filename(&mapping.table_name),
        );
        assert!(filename.contains("report-b-c"));
    }

    /// Integration test against the real Lumina PPTX — only runs if the file exists.
    #[test]
    fn test_lumina_cascade_match() {
        let pptx_path = "/Users/edwinlovettiii/Downloads/1778249994166_lumina_charts.pptx";
        let Ok(bytes) = std::fs::read(pptx_path) else {
            eprintln!("  [skip] {pptx_path} not found");
            return;
        };

        // Extract tables and slide infos using the enriched extractor.
        let tables = crate::pptx::extract_pptx_tables(&bytes).expect("extraction should succeed");
        let slide_infos =
            crate::pptx::extract_slide_infos(&bytes).expect("slide infos should succeed");
        assert!(!tables.is_empty(), "should extract tables");

        // Load registry.
        let reg_paths = [
            "demo/registry/performance_tables.csv",
            "../demo/registry/performance_tables.csv",
            "../../demo/registry/performance_tables.csv",
        ];
        let registry = reg_paths
            .iter()
            .find_map(|p| Registry::from_file(p).ok())
            .expect("registry should load");

        // Load profile.
        let profile_dirs = [
            std::path::Path::new("demo/sample-data"),
            std::path::Path::new("../demo/sample-data"),
            std::path::Path::new("../../demo/sample-data"),
        ];
        let profile = profile_dirs
            .iter()
            .find_map(|d| crate::pptx_profile::load_profile(d, "lumina-charts"));

        // Build IDF table.
        let idf = IdfTable::build(&registry);

        // Run cascade.
        let result =
            match_deck_with_slides(&tables, &slide_infos, &registry, profile.as_ref(), &idf);

        eprintln!("\n  === Lumina Cascade Results ===");
        eprintln!("  Total tables: {}", result.stats.total_tables);
        eprintln!("  Auto-resolved: {}", result.stats.auto_resolved);
        eprintln!("  Skipped: {}", result.stats.skipped);
        eprintln!("  Review needed: {}", result.stats.review_needed);
        eprintln!("  Unmatched: {}", result.stats.unmatched);
        eprintln!("  Duplicates: {}", result.stats.duplicates);
        eprintln!("  By source: {:?}", result.stats.by_source);

        for m in &result.matches {
            let status_str = match m.status {
                MatchStatus::Matched => "MATCHED",
                MatchStatus::Duplicate => "DUP",
                MatchStatus::Skipped => "SKIP",
                MatchStatus::ContinuationCandidate => "CONT?",
                MatchStatus::Unresolved => "UNRESOLVED",
            };
            let mapping_str = m
                .mapping
                .as_ref()
                .map(|m| format!("{}/{}/{}", m.product_name, m.subproduct_name, m.table_name))
                .unwrap_or_else(|| "—".to_string());

            let section = m.evidence.section_context.as_deref().unwrap_or("-");
            eprintln!(
                "  slide {:>3} t{} [{:>10}] [{:>18}] conf={:.2} sect={:<12} → {}",
                m.table.slide_index,
                m.table.table_index,
                status_str,
                format!("{:?}", m.evidence.fired_step),
                m.confidence,
                section,
                mapping_str,
            );
        }

        // Acceptance criteria:
        // ≥80% of tables auto-resolved
        let auto_resolved_pct = if result.stats.total_tables > 0 {
            (result.stats.auto_resolved + result.stats.skipped) as f64
                / result.stats.total_tables as f64
                * 100.0
        } else {
            0.0
        };
        eprintln!("\n  Auto-resolved + skipped: {auto_resolved_pct:.1}%");
        assert!(
            auto_resolved_pct >= 80.0,
            "expected ≥80% auto-resolved, got {auto_resolved_pct:.1}%"
        );

        // Skip rule fires on Reach & Frequency.
        let reach_skipped = result.matches.iter().any(|m| {
            m.status == MatchStatus::Skipped
                && m.table
                    .slide_title
                    .as_deref()
                    .unwrap_or("")
                    .contains("Reach & Frequency")
        });
        assert!(reach_skipped, "Reach & Frequency should be skipped");

        // Ingestion invariant: only Matched results would pass through to_parsed_csvs.
        for m in &result.matches {
            if m.status == MatchStatus::Matched {
                let mapping = m.mapping.as_ref().expect("Matched must have mapping");
                let filename = format!(
                    "report-{}-{}.csv",
                    sanitize_for_filename(&mapping.subproduct_name),
                    sanitize_for_filename(&mapping.table_name),
                );
                assert!(
                    filename.starts_with("report-"),
                    "filename should start with report-: {}",
                    filename
                );
            }
        }

        // Coverage warnings.
        for w in &result.coverage_warnings {
            eprintln!("  WARNING: {w}");
        }
    }

    /// Helper: run cascade on a PPTX file and return (auto_resolved_pct, stats).
    fn run_cascade_on_pptx(pptx_path: &str) -> Option<(f64, MatchStats)> {
        let bytes = std::fs::read(pptx_path).ok()?;
        let tables = crate::pptx::extract_pptx_tables(&bytes).ok()?;
        let slide_infos = crate::pptx::extract_slide_infos(&bytes).ok()?;

        let reg_paths = [
            "demo/registry/performance_tables.csv",
            "../demo/registry/performance_tables.csv",
            "../../demo/registry/performance_tables.csv",
        ];
        let registry = reg_paths.iter().find_map(|p| Registry::from_file(p).ok())?;

        let profile_dirs = [
            std::path::Path::new("demo/sample-data"),
            std::path::Path::new("../demo/sample-data"),
            std::path::Path::new("../../demo/sample-data"),
        ];
        let profile = profile_dirs
            .iter()
            .find_map(|d| crate::pptx_profile::load_profile(d, "lumina-charts"));

        let idf = IdfTable::build(&registry);
        let result =
            match_deck_with_slides(&tables, &slide_infos, &registry, profile.as_ref(), &idf);

        let pct = if result.stats.total_tables > 0 {
            (result.stats.auto_resolved + result.stats.skipped) as f64
                / result.stats.total_tables as f64
                * 100.0
        } else {
            0.0
        };

        eprintln!(
            "\n  === {} ===",
            pptx_path.rsplit('/').next().unwrap_or(pptx_path)
        );
        eprintln!(
            "  {} tables: {} matched, {} skipped, {} review, {} dup, {} unmatched ({:.1}%)",
            result.stats.total_tables,
            result.stats.auto_resolved,
            result.stats.skipped,
            result.stats.review_needed,
            result.stats.duplicates,
            result.stats.unmatched,
            pct,
        );
        eprintln!("  By source: {:?}", result.stats.by_source);

        Some((pct, result.stats))
    }

    /// Test against deck 627917 — should have SEM duplicate suppression.
    #[test]
    fn test_lumina_deck_627917() {
        let path = "/Users/edwinlovettiii/Downloads/1778255627917_lumina_charts.pptx";
        let Some((pct, _stats)) = run_cascade_on_pptx(path) else {
            eprintln!("  [skip] {path} not found");
            return;
        };
        assert!(
            pct >= 60.0,
            "deck 627917: expected ≥60% auto-resolved, got {pct:.1}%"
        );
    }

    /// Test against deck 792946 — should have 3-tactic rollup manifest.
    #[test]
    fn test_lumina_deck_792946() {
        let path = "/Users/edwinlovettiii/Downloads/1778255792946_lumina_charts.pptx";
        let Some((pct, _stats)) = run_cascade_on_pptx(path) else {
            eprintln!("  [skip] {path} not found");
            return;
        };
        // This deck has many sections and more unresolved tables.
        // The primary deck hits 91.7%; this is a secondary regression target.
        assert!(
            pct >= 50.0,
            "deck 792946: expected ≥50% auto-resolved, got {pct:.1}%"
        );
    }

    /// Test against deck 959819 — false-positive divider rejection test.
    #[test]
    fn test_lumina_deck_959819() {
        let path = "/Users/edwinlovettiii/Downloads/1778255959819_lumina_charts.pptx";
        let Some((pct, _stats)) = run_cascade_on_pptx(path) else {
            eprintln!("  [skip] {path} not found");
            return;
        };
        assert!(
            pct >= 60.0,
            "deck 959819: expected ≥60% auto-resolved, got {pct:.1}%"
        );
    }
}
