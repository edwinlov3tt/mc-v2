# Phase 4C Handoff — Organization and Workspace Primitive

**Status:** Proposed (next to start)
**Date:** 2026-05-09
**Predecessor:** Phase 4D (complete), Phase 7A.6 (complete), Phase 6E (complete)
**ADR:** [ADR-0026](../decisions/0026-org-workspace-resource-scope-capability-grants.md) (Accepted)
**Prior research:** [Multi-domain workspaces proposal](../research-notes/multi-domain-workspaces-proposal.md)
**Estimated effort:** 4–6 sessions (Tier A scope only)
**Crate(s) touched:** new `mc-workspace` crate + `mc-cli` additions (NO kernel changes, NO `mc-core` changes)

---

## What this phase does

Implements ADR-0026's organizational container model as a **manifest + CLI layer above the existing cube engine**. After this phase, Mosaic has:

1. A `workspace.yaml` manifest that declares what cubes, shared resources, and links a workspace contains
2. An `org.yaml` manifest that declares org identity, installed cartridges, and template/benchmark inheritance
3. A `$ref:` resolver that lets cubes reference shared dimension catalogs and fitted models from the workspace
4. `mc workspace {init, validate, lint, test, inspect}` CLI verbs
5. The foundation for Phase 8 daemon (org-aware) and Phase 9 cloud (tenant = org)

**What this phase does NOT do:**
- No kernel (`mc-core`) changes
- No cross-cube formula refs (Tier C — Phase 5+ per spec)
- No RBAC, auth, or multi-user permissions
- No billing, SCIM, SAML
- No capability-grant enforcement at runtime (grants are declared; enforcement comes in Phase 8)
- No cross-workspace aggregation engine

---

## Architecture (how it fits)

```
┌─────────────────────────────────────────────┐
│  mc-cli (verbs: mc workspace ...)           │  ← CLI orchestration
├─────────────────────────────────────────────┤
│  mc-workspace (NEW)                         │  ← Manifest parsing, $ref resolution,
│    - org.yaml parser + validator            │     workspace-level validation/lint,
│    - workspace.yaml parser + validator      │     cross-cube consistency checks
│    - $ref resolver (catalogs, fitted, etc.) │
│    - workspace-level lint rules             │
│    - workspace inspector                    │
├─────────────────────────────────────────────┤
│  mc-model (unchanged)                       │  ← Per-cube pipeline (parse → validate → compile)
├─────────────────────────────────────────────┤
│  mc-core (unchanged)                        │  ← Kernel (sync, deployment-agnostic)
└─────────────────────────────────────────────┘
```

The workspace layer **orchestrates over** the existing per-cube pipeline. It resolves `$ref:` directives by inlining shared resources into a synthesized model YAML, then hands each cube to the unchanged `mc-model::load()` pipeline. The kernel never sees workspace concepts.

---

## Scope: What to build

### 1. New crate: `mc-workspace`

Create `crates/mc-workspace/` with:

**Manifest types:**

```rust
// org.yaml
pub struct ParsedOrg {
    pub org_format_version: u32,      // starts at 1
    pub name: String,
    pub id: String,                    // stable org identifier
    pub description: Option<String>,
    pub installed_cartridges: Vec<CartridgeRef>,
    pub org_templates_path: Option<PathBuf>,
    pub org_benchmarks_path: Option<PathBuf>,
    pub workspaces: Vec<WorkspaceEntry>,
}

pub struct WorkspaceEntry {
    pub path: PathBuf,                 // relative to org root
    pub name: String,
}

// workspace.yaml
pub struct ParsedWorkspace {
    pub workspace_format_version: u32, // starts at 1
    pub name: String,
    pub id: String,                    // stable workspace identifier
    pub description: Option<String>,
    pub domain: Option<String>,        // references a domain pack
    pub org_id: Option<String>,        // which org owns this workspace (None = standalone)
    pub shared_dimensions: Vec<SharedCatalog>,
    pub shared_fitted_models: Vec<SharedArtifact>,
    pub shared_calibration_maps: Vec<SharedArtifact>,
    pub shared_lookup_tables: Vec<SharedArtifact>,
    pub cubes: Vec<CubeEntry>,
    pub links: Vec<CubeLink>,          // declarative only in Tier A (no engine enforcement)
    pub golden_suites: Vec<PathBuf>,
}

pub struct SharedCatalog {
    pub id: String,
    pub source: PathBuf,
}

pub struct SharedArtifact {
    pub id: String,
    pub source: PathBuf,
}

pub struct CubeEntry {
    pub path: PathBuf,
    pub name: Option<String>,          // override display name
}

pub struct CubeLink {
    pub from_cube: String,
    pub from_measure: String,
    pub to_cube: String,
    pub to_measure: String,
    pub kind: LinkKind,                // ReadOnly for now
    pub description: Option<String>,
}
```

**`$ref:` resolution:**

When a cube's dimension YAML contains:
```yaml
dimensions:
  - name: "Channel"
    kind: "Standard"
    $ref: "workspace://shared_dimensions/Sportsbook"
```

The resolver:
1. Looks up `Sportsbook` in `workspace.shared_dimensions`
2. Reads the source file (`catalogs/sportsbooks.yaml`)
3. Inlines the dimension elements into the cube YAML (in memory)
4. Passes the resolved YAML to `mc-model::load_str()`

Same pattern for `$ref: "workspace://shared_fitted_models/..."` etc.

**Workspace validation (MC5xxx diagnostic codes):**

| Code | Severity | Meaning |
|---|---|---|
| MC5001 | Error | Workspace manifest parse failure |
| MC5002 | Error | Referenced cube file not found |
| MC5003 | Error | `$ref` target not found in workspace shared resources |
| MC5004 | Error | Link references nonexistent cube or measure |
| MC5005 | Warning | Naming inconsistency across cubes (same concept, different names) |
| MC5006 | Warning | Shared catalog not referenced by any cube |
| MC5007 | Warning | Cube not referenced in workspace manifest |
| MC5008 | Info | Workspace has no golden suites defined |

**Workspace lint (extends MC5xxx):**

| Code | Severity | Meaning |
|---|---|---|
| MC5010 | Warning | Dimension naming drift across cubes (e.g., "Paid_Search" vs "paidSearch") |
| MC5011 | Warning | Measure name collision across cubes without explicit link declaration |
| MC5012 | Info | Cube has no description field |

### 2. CLI verbs in `mc-cli`

```
mc workspace init <name> [--domain <domain>]
    Create a new workspace directory with workspace.yaml scaffold.
    If --domain given, seed from domain pack template.

mc workspace validate [--path <dir>]
    Parse workspace.yaml → resolve all $refs → validate each cube → 
    run workspace-level checks (links, naming, catalog refs).
    Returns JSON diagnostic envelope (same shape as mc model validate).

mc workspace lint [--path <dir>]
    Per-cube lint + workspace-scoped lint rules (MC5010–MC5012).

mc workspace test [--path <dir>]
    Run every cube's goldens + workspace-level golden suites.

mc workspace inspect [--path <dir>] [--format text|json]
    Summary: cubes, dimensions, measures, shared resources, links.
```

All verbs default to current directory if `--path` is omitted. A workspace is identified by the presence of `workspace.yaml` in the directory.

### 3. Standalone cube backward compatibility

**Critical:** Single-cube users without a workspace must continue to work exactly as before. All existing `mc model *` verbs work on standalone YAML files without a workspace. The workspace is additive, not required.

A cube YAML with no `$ref:` directives loads through the unchanged `mc-model::load()` pipeline. A cube YAML with `$ref:` directives can only load in a workspace context (via `mc workspace validate`); attempting to load it standalone produces an informative error pointing at the unresolved `$ref`.

### 4. Dimension element typing (TM1 parity feature)

Per the TM1 comparison doc (Part 7), add optional `element_type` to the dimension schema:

```yaml
dimensions:
  - name: "Time"
    kind: "Standard"
    element_type: "date"     # validates elements parse as dates
    elements: [...]
```

Supported types: `string` (default, no validation), `date` (elements must parse as time periods), `numeric` (elements must parse as numbers). This is a small addition to `mc-model`'s validator.

---

## Directory layout (what a workspace looks like)

```
my-workspace/
  workspace.yaml              # manifest — identity + resource declarations
  catalogs/                   # shared dimension catalogs
    channels.yaml
    markets.yaml
    time-periods.yaml
  cubes/                      # one YAML per cube (same format as today)
    marketing-finance.yaml
    brand-awareness.yaml
  fitted/                     # shared fitted models / calibration maps
    mmm-lasso-v3.yaml
  fixtures/
    canonical-inputs/         # CSVs for cube inputs
    test-fixtures/            # named overlays
  goldens/                    # workspace-level cross-cube goldens
    full-pipeline.golden.yaml
  .mosaic/                    # workspace runtime state (future: ledger, cache)
```

For orgs with multiple workspaces:
```
my-org/
  org.yaml                    # org identity + installed cartridges
  workspaces/
    client-a/
      workspace.yaml
      cubes/...
    client-b/
      workspace.yaml
      cubes/...
  templates/                  # org-level templates (Level 3 in inheritance)
  benchmarks/                 # org-level benchmarks
```

---

## What NOT to build

| Out of scope | Why | When |
|---|---|---|
| Cross-cube formula refs in the kernel | Tier C — requires dependency graph extension | Phase 5+ |
| Runtime grant enforcement | Grants are declared; enforcement needs the daemon | Phase 8 |
| Multi-user permissions / RBAC | Auth layer; not a manifest concern | Phase 9 |
| Cartridge marketplace | Publishing/discovery infrastructure | Phase 10+ |
| Cross-workspace aggregation engine | Privacy-sensitive; needs Phase 7A.4 model | Phase 9+ |
| Domain pack authoring (beyond marketing-mix) | Real domain expertise needed | Demand-driven |
| Hot-reload of `$ref` changes | Bake (resolve at validate time) for v1 | Phase 6B (UI) |
| Workspace-level snapshot/rollback | Requires kernel understanding of workspace | Phase 8+ |

---

## Implementation path

### Step 1: Create `crates/mc-workspace/`

- `Cargo.toml` — deps: `mc-model`, `serde`, `serde_yaml`, `thiserror`, `ahash`
- `src/lib.rs` — public API: `load_workspace(path)`, `validate_workspace(parsed)`, `resolve_refs(workspace, cube_yaml)`
- `src/schema.rs` — `ParsedWorkspace`, `ParsedOrg`, all supporting types
- `src/parse.rs` — YAML deserialization for workspace.yaml and org.yaml
- `src/validate.rs` — MC5001–MC5008 workspace-level validators
- `src/lint.rs` — MC5010–MC5012 workspace-level lint rules
- `src/resolve.rs` — `$ref:` inlining logic
- `src/inspect.rs` — workspace summary (text + JSON)
- `src/diagnostic.rs` — workspace diagnostic types (reuse patterns from `mc-model`)

### Step 2: Add `$ref` support to mc-model schema

In `crates/mc-model/src/schema.rs`, add an optional `$ref` field to the `ParsedDimension` and other structs that support workspace sharing:

```rust
pub struct ParsedDimension {
    pub name: String,
    pub kind: DimensionKind,
    pub ref_uri: Option<String>,  // e.g., "workspace://shared_dimensions/Sportsbook"
    pub elements: Option<Vec<ParsedElement>>,  // None when $ref is present
    // ...
}
```

The `mc-model` validator allows `elements: None` when `ref_uri: Some(...)` — but if loaded standalone (no workspace context), it errors with "unresolved $ref; load via mc workspace validate".

### Step 3: Add dimension element_type to mc-model

Small addition to `ParsedDimension`:
```rust
pub element_type: Option<ElementType>,  // string | date | numeric
```

Validator checks: if `element_type: date`, all element names must match a known date pattern (per ADR-0014 time_format). If `element_type: numeric`, all element names must parse as numbers.

### Step 4: Wire CLI verbs in mc-cli

Add `crates/mc-cli/src/workspace.rs` with subcommand dispatch:
- `mc workspace init` — scaffold directory + workspace.yaml template
- `mc workspace validate` — full pipeline
- `mc workspace lint` — per-cube + workspace lint
- `mc workspace test` — orchestrate goldens
- `mc workspace inspect` — summary output

### Step 5: Integration tests

- Workspace with 2+ cubes, shared catalogs, links → validate passes
- `$ref` resolution works (shared dimension inlined correctly)
- Missing `$ref` target → MC5003 error
- Broken link → MC5004 error
- Standalone cube without workspace → works as before (no regression)
- Standalone cube WITH `$ref` but no workspace → informative error
- Naming drift lint fires → MC5010
- Dimension element_type validation works

### Step 6: Update Acme demo as a workspace

Convert the existing Acme example into workspace form:
```
examples/acme-workspace/
  workspace.yaml
  catalogs/
    channels.yaml
    markets.yaml
    time-periods.yaml
  cubes/
    marketing-finance.yaml    # the existing acme.yaml, refactored with $refs
  fixtures/
    canonical-inputs/
      marketing-finance.inputs.csv
```

This proves the workspace model works with real data and gives other implementations a reference.

---

## Open design decisions (for implementer to resolve)

1. **Workspace ID format.** UUID vs human-readable slug vs both? Recommendation: `id: "acme-marketing"` (slug) + let Phase 8 daemon assign UUIDs if needed. Keep it simple for file-based deployments.

2. **Org without workspace.** Can an `org.yaml` exist without workspaces listed? Yes — an org with zero workspaces is valid (freshly created, not yet populated).

3. **Nested workspace discovery.** Does `mc workspace validate` auto-discover workspaces in subdirectories? Recommendation: No. Explicit paths only. Auto-discovery is fragile and slow on large repos.

4. **Catalog YAML format.** What does `catalogs/channels.yaml` look like?
   ```yaml
   # Shared dimension catalog
   catalog_format_version: 1
   dimension: "Channel"
   elements:
     - { name: "Paid_Search", parent: "Digital" }
     - { name: "Display", parent: "Digital" }
     - { name: "Social", parent: "Digital" }
     - { name: "TV", parent: "Traditional" }
     - { name: "Print", parent: "Traditional" }
   hierarchy:
     - { name: "Digital", children: ["Paid_Search", "Display", "Social"] }
     - { name: "Traditional", children: ["TV", "Print"] }
     - { name: "All_Channels", children: ["Digital", "Traditional"] }
   ```

5. **Error vs warning for unused shared resources.** MC5006 (shared catalog not referenced by any cube) — Warning feels right; it might be there for a cube that hasn't been created yet.

---

## Acceptance criteria

1. `mc workspace validate` on the Acme workspace example passes cleanly
2. `$ref:` resolution correctly inlines shared dimension catalogs into cubes
3. Standalone cubes without `$ref` continue to work identically (no regression)
4. Standalone cubes with unresolved `$ref` produce informative error
5. Workspace-level diagnostics (MC5001–MC5012) fire correctly
6. `mc workspace inspect` produces a coherent summary (text + JSON)
7. `mc workspace init` scaffolds a usable workspace directory
8. Dimension `element_type` validation works for `date` and `numeric`
9. `cargo test --workspace` passes
10. `cargo clippy --all-targets --workspace -- -D warnings` passes
11. No changes to `mc-core`

---

## Dependencies

**New workspace crate deps (all already in workspace Cargo.toml):**
- `mc-model` (path dep)
- `serde` + `serde_yaml` (already used by mc-model; same versions)
- `thiserror` (workspace dep)
- `ahash` (workspace dep)

**No new external dependencies introduced to the workspace.**

---

## Cross-links

- **ADR-0026:** The binding architecture this phase implements
- **ADR-0025 Decision 7:** Kernel remains workspace-unaware
- **Prior research:** `docs/research-notes/multi-domain-workspaces-proposal.md` (Tier A scope maps to this phase)
- **TM1 comparison Part 7:** Dimension element typing as a Phase 4C candidate
- **Phase 8 (daemon):** Inherits from this phase — must be org-aware
- **Phase 9 (cloud):** Multi-tenant where tenant = org

---

**End of handoff. This phase creates the organizational foundation that all future deployment shapes build on. The kernel stays untouched; the workspace is a layer above. Ship Tier A, defer Tiers B–D to demand.**
