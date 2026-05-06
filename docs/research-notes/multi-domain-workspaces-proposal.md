# Multi-Domain Workspaces — Capability Gaps + Proposed Structure

**Status:** `proposal — not adopted; seeking validation`
**Created:** 2026-05-06
**Last touched:** 2026-05-06
**Spans phases:** `Phase 3+ (workspace primitive), Phase 4 (domain packs), Phase 5+ (cross-cube refs)`
**Author intent:** consult-driven sketch handed to the PM. Not a decision; not an ADR. The PM either greenlights a Tier A slice, defers, or pushes back on the framing.

---

## Conclusion (one sentence)

Mosaic today is a *single-cube engine with a directory-of-files convention* — to make it credibly multi-domain (sports-betting + FP&A + marketing + prospect-scoring on the same install) we need a **Workspace primitive** (manifest + shared catalogs + cross-cube link contract), **domain packs** with a stable plugin shape, and a phased path to **cross-cube formula refs** without breaking single-cube semantics.

## Why this matters

The strategic positioning ([`../strategy/POSITIONING.md`](../strategy/POSITIONING.md)) frames Mosaic as a *general LNM substrate with installable domain schemas*. The kernel is broadly there. The product surface above the kernel is not — every cube is an island, every domain re-implements the same patterns from scratch, and the architect/author agents have only one domain pack (marketing-mix) to draw from.

What goes wrong without this:

- **Schema drift** — a "Sportsbook" element list copy-pasted across 8 cubes. One updates, seven don't. No engine catches it.
- **Domain-blind agent assistance** — the architect agent designs an FP&A cube using marketing-mix vocabulary because that's the only `domain-schemas/` pattern it has.
- **No cross-cube composition** — an NBA-totals cube produces `Predicted_Total`. An NBA-player-props cube can't reference it. Users either inline-duplicate the prediction logic or stitch with external scripts (which the engine can't validate or trace).
- **No workspace as a unit** — there's no answer to "what does this user / team / project own?" because there's no container above the cube.
- **Onboarding friction** — a new domain (FP&A) asks "how do I start?" and the honest answer today is "copy `acme-marketing.yaml` and rewrite everything." That's not a platform; that's a code sample.

## Current state — what exists, what doesn't

Verbatim from spec ([engine-semantics.md §20](../specs/engine-semantics.md), §1659):

> **Workspace:** Top-level container for a set of cubes that share dimensions, principals, and permissions. **Phase 3+ concept; v1 has one workspace.**

And [`engine-semantics.md`](../specs/engine-semantics.md) `I-Dep-6`:

> Dependency edges may cross cubes (Phase 5+). v1 confines edges to a single cube; cross-cube dependencies are rejected.

The plugin has *one* domain pack: `mosaic-plugin/skills/domain-schemas/marketing-mix/`. Sports-betting exists as a sample cartridge in `examples/sports-betting/` but has no architect-level pack. FP&A, prospect-scoring, sales-forecasting, demand-planning have neither.

So today, "workspace" is a folder convention. "Domain" is one plugin skill (or zero). "Cross-cube ref" is "write a Python script."

---

## Capability gaps (broad — apply across all domains)

Numbered for handoff. Severity: **B**lock = nothing meaningful ships without it; **F**riction = ships, but every domain pays the same tax; **P**olish = nice-to-have once Block + Friction are closed.

### Structural

1. **No workspace primitive (B).** No `Workspace` type in the kernel; no `workspace.yaml` manifest; no CLI verb that operates above the cube level. Every multi-cube install is conventionally-organized at best.
2. **No shared dimension catalogs (B).** Every cube redeclares its Time, Channel, Market elements. If two cubes need the same Sportsbook list (or US-States, or Org-Chart, or SKU-catalog) it's copy-paste. No `$ref:` resolver, no compile-time inclusion, no drift detection.
3. **No cross-cube refs in formulas (B for some domains; F for others).** Currently `Predicted_Total` in cube A cannot be read by `Should_Bet` in cube B inside a rule body. Spec calls this out as Phase 5+. Sports-betting wants this immediately; FP&A wants it for actuals-vs-budget across plan domains; demand-planning wants it for SKU-cube → location-cube fanout.
4. **No domain pack format (B).** `mosaic-plugin/skills/domain-schemas/<domain>/` is one skill (marketing-mix). There's no contract for what a domain pack contains, no template directory shape, no bundled catalogs/cube-templates/workflow doc.
5. **No workspace-scoped principals or permissions (F).** Tied to the spec's Phase 3+ workspace concept. Without it, multi-user / multi-team installs have nothing to hang RBAC on.

### Authoring + tooling

6. **No `mc workspace` CLI surface (F).** All verbs are per-file: `mc model validate <file>`, `mc model test <file>`. No `mc workspace validate` that runs the whole graph + cross-cube link checks.
7. **No workspace-level lint (F).** MC3001 (naming consistency) is per-cube. A workspace with `Paid_Search` in one cube and `paidSearch` in another never trips.
8. **No shared-fitted-model resolution (F).** A Lasso model used by 3 cubes is embedded in each. Update once, miss two. No `$ref:` for `fitted_models:` / `lookup_tables:` / `calibration_maps:` either.
9. **No workspace-level test harness (F).** `mc model test` runs goldens per file. Cross-cube goldens ("if NBA-totals predicts X, then NBA-player-props should evaluate Y") have no home.
10. **No shared `canonical_inputs` source (F).** Two cubes that read the same Sportsbook actuals each declare their own input source. Schema-aligning them across cubes is manual.

### Discovery + agent assistance

11. **No model registry / discovery (F).** Agents have to be handed a cube path. There's no "list cubes in this workspace" or "find the cube that produces measure X."
12. **Domain-blind agent assistance (F).** The architect/author/validator agents have one domain pack. New domains get generic guidance, which means generic mistakes (defaulting to Sum on ratios is the cliché — every domain has its own version).
13. **No cube-link manifest (F).** Even if cross-cube refs are deferred to Phase 5+, the *intention* ("nba-totals produces Predicted_Total; nba-player-props consumes it") has no declarative form today. That declaration is what unblocks workspace-level diagnostics, lineage graphs, and impact-of-change analysis *before* the kernel cross-cube ref ships.

### Operations + lifecycle

14. **No workspace versioning / migration (P).** `model_format_version: 1` is per-cube. There's no workspace-level "this workspace is at v1.3, migrate to v1.4" story.
15. **No workspace telemetry (P).** Multi-cube health view, per-cube drift signals, "how often does this golden flake" — none of it has a cross-cube container.
16. **No domain-pack installer (P).** The plugin manifest doesn't have a domain-install verb. Adding `sports-betting` means writing files; there's no `mc plugin install domain sports-betting`.

---

## Pitched structure — workspaces as **directory + manifest + shared catalogs**

> **Headline answer to "should a workspace be its own file?":** Workspace is a *directory* with a `workspace.yaml` *manifest at the root*. The manifest is the contract; the directory is the container. The manifest references cubes, catalogs, fitted artifacts, and inter-cube links. Cubes stay in their own files.

### Directory layout (proposed)

```
my-workspace/
  workspace.yaml              # manifest — the contract
  catalogs/                   # shared dimension catalogs (workspace-scoped)
    sportsbooks.yaml
    sport-types.yaml
    seasons.yaml
    us-states.yaml            # marketing-mix style
    cost-centers.yaml         # FP&A style
  cubes/                      # one YAML per cube; same shape as today
    nba-totals.yaml
    nba-player-props.yaml
    fpa-monthly-plan.yaml
    calendar-analytics.yaml
  fitted/                     # shared fitted_models / calibration_maps
    nba-totals-lasso-v16.yaml
    win-prob-calibration.yaml
  fixtures/
    canonical-inputs/         # CSVs referenced by cube canonical_inputs:
    test-fixtures/            # named overlays
  goldens/                    # workspace-level cross-cube goldens
    nba-edge-pipeline.golden.yaml
  output/                     # gitignored — traces, exports, reports
  .mosaic/                    # workspace state (lockfile, cache, telemetry)
```

### `workspace.yaml` shape

```yaml
workspace_format_version: 1
name: "edwin-sports-research"
description: "Sports-betting research + calendar analytics workspace."
domain: "sports-betting"      # references an installed domain pack
created: "2026-05-06"

# Shared dimension catalogs — referenced by cubes via $ref.
shared_dimensions:
  - id: "Sportsbook"
    source: "catalogs/sportsbooks.yaml"
  - id: "Sport_Type"
    source: "catalogs/sport-types.yaml"

# Shared fitted artifacts — same idea.
shared_fitted_models:
  - id: "nba_totals_lasso_v16"
    source: "fitted/nba-totals-lasso-v16.yaml"
shared_calibration_maps:
  - id: "win_prob_calibration"
    source: "fitted/win-prob-calibration.yaml"

# Cubes participating in this workspace.
cubes:
  - path: "cubes/nba-totals.yaml"
  - path: "cubes/nba-player-props.yaml"
  - path: "cubes/fpa-monthly-plan.yaml"
  - path: "cubes/calendar-analytics.yaml"

# Inter-cube links — declarative, validated at workspace-validate time.
# Phase A: documentation-only (engine doesn't enforce; tooling does).
# Phase C: cross-cube refs in formula bodies become real.
links:
  - from: { cube: "nba-totals", measure: "Predicted_Total" }
    to:   { cube: "nba-player-props", measure: "Game_Total_Reference" }
    kind: "read-only"
    description: "Player props uses the totals cube's prediction as a reference line."

# Workspace-level golden suites.
golden_suites:
  - "goldens/nba-edge-pipeline.golden.yaml"
```

In a cube YAML, a `$ref` to a shared catalog replaces the inline `elements:` list:

```yaml
dimensions:
  - name: "Channel"           # slot 4 — workspace calls this Sportsbook
    kind: "Standard"
    $ref: "workspace://shared_dimensions/Sportsbook"
```

The `workspace://` URI scheme is the key — it lets the cube validate as a standalone file when no workspace is loaded (the `$ref` is a typed placeholder; tooling resolves it at workspace-validate time).

### Domain pack shape (plugin upgrade)

A domain pack is an installable plugin skill bundle:

```
mosaic-plugin/skills/domain-schemas/<domain>/
  SKILL.md                    # patterns, pitfalls, dim-slot conventions (already exists for marketing-mix)
  catalogs/                   # reference catalogs the domain ships
    seasons.yaml
    sportsbooks.yaml
  cube-templates/             # scaffold-ready YAML
    nba-totals.template.yaml
    nba-spreads.template.yaml
    player-props.template.yaml
  fitted-templates/           # pre-fit example artifacts (not production data)
  workspace-template.yaml     # what `mc workspace init --domain X` writes
  workflow.md                 # end-to-end author flow for this domain
  goldens-recipe.md           # what kinds of goldens this domain typically wants
```

Domain packs needed for parity with the strategic plan: `marketing-mix` (exists), `sports-betting`, `fpa`, `prospect-scoring`, `sales-forecasting`, `demand-planning`. Each is ~1–2 weeks of authoring work for someone with domain expertise + the existing marketing-mix pack as a reference.

### CLI surface (proposed)

```
mc workspace init <name> --domain <domain>
    Scaffold a new workspace from a domain pack's workspace-template.yaml.

mc workspace validate
    Per-cube validate + workspace-level checks (catalog refs resolve,
    link manifest references real measures, naming consistent across cubes).

mc workspace lint
    Per-cube lint + workspace-scoped lint (e.g., catalog drift, naming).

mc workspace test
    Run every cube's goldens + workspace-level cross-cube goldens.

mc workspace inspect
    Graph view: cubes, shared catalogs, links, fitted artifacts.

mc workspace add-cube <path> [--from-template <template>]
mc workspace add-catalog <id> --source <file>
mc workspace install domain <domain>
```

All of these are *orchestration over the existing per-cube CLI* in Tier A. They become engine-level once cross-cube refs ship in Tier C.

---

## How workspaces should function — answers to specific questions

**Own file or within a project?** *Both.* The workspace is **a directory** (so it lives "within a project" naturally — a project can be a workspace, or contain N workspaces, or sit alongside one). The workspace **identity is `workspace.yaml` at the root**. So:

- A repo with one workspace: `<repo>/workspace.yaml` at root.
- A repo with multiple workspaces: `<repo>/workspaces/sports/workspace.yaml`, `<repo>/workspaces/fpa/workspace.yaml`.
- A standalone workspace someone shares: a directory with `workspace.yaml` inside, zip-shippable.

**Are cubes per workspace or shareable across workspaces?** Cubes belong to exactly one workspace at a time (the one whose `workspace.yaml` references the cube path). A cube can be *copied* between workspaces; symlinks and "cube libraries" are out of scope. This keeps the lifecycle simple — one owner, one validate run, one golden suite. Reusable patterns live in domain packs (cube templates), not in cross-workspace cube sharing.

**One workspace per user, team, project?** *Per project.* A user can have many. A team's "production planning workspace" is a different artifact from one analyst's "research scratch workspace." The kernel doesn't need to know about users/teams — that's the auth layer above. Workspaces are the unit of *modeling work*; auth is orthogonal.

**Where do shared catalogs live?** Inside the workspace. Workspace-scoped, not domain-scoped. Domain packs *ship reference catalogs* you can copy in or `$ref` from, but the active version is in your workspace and you own edits. (This avoids "I edited the domain pack and now everyone else broke.")

**What about catalogs shared across workspaces?** Out of scope for v1. If two workspaces both need a US-States catalog, they each have their own copy seeded from the domain pack. Cross-workspace catalog sharing is a registry problem; defer.

**What if someone wants a single cube without a workspace?** Stays supported. A standalone YAML with no workspace is the same shape as today. Workspaces *layer on top*; they don't replace single-file authoring.

---

## Implementation phasing

### Tier A — small, ships value immediately, no kernel changes

Estimated: 2–3 weeks of focused work + per-domain pack authoring (~1 week each).

- `workspace.yaml` manifest format (parser + validator in `mc-model`).
- `$ref:` resolver for shared dim catalogs / fitted models / lookup tables / calibration maps. At workspace-validate time, refs are inlined into a synthesized cube YAML; the kernel still sees one cube per file, fully normalized.
- `mc workspace {init, validate, lint, test, inspect}` verbs, all of which orchestrate over per-cube CLI.
- Workspace-scoped lint (naming consistency across cubes, catalog drift detection).
- Domain pack format spec + at least 2 packs upgraded to it (marketing-mix + sports-betting). Each pack ships catalogs + cube templates + workflow doc.
- Workspace MCP tools (`mosaic.workspace.{validate, test, inspect}`) for the plugin.

**Why this tier first:** zero kernel changes; addresses gaps 1, 2, 4, 6, 7, 8, 10, 11, 12 above; delivers the "platform" feel without touching the engine's correctness story.

### Tier B — medium, unblocks first cross-domain proofs

Estimated: 4–6 weeks. Likely a Phase 4-adjacent deliverable.

- Workspace-level test fixtures + cross-cube goldens (still orchestrated; no kernel cross-cube refs).
- "Cube link manifest" — declarative `links:` block in `workspace.yaml`. Validates that referenced measures exist, but the dataflow is still executed externally (script glue). Useful for lineage diagrams + impact-of-change analysis even without engine support.
- Workspace-level diagnostics envelope (the same JSON envelope shape as MC1xxx–MC4xxx, but at workspace scope: MC5xxx codes for workspace-level issues — drift, link mismatch, missing catalog, etc.).
- Shared `canonical_inputs:` resolution (one CSV referenced by N cubes whose Time / Channel / Market dims share catalog).
- Per-domain pack agent guidance (architect knows it's an FP&A workspace and proposes FP&A patterns, not marketing patterns).

### Tier C — large, requires kernel changes

Estimated: substantial; Phase 5+ per the spec.

- Cross-cube refs in formula bodies. Syntax sketch: `{cube: "nba-totals", measure: "Predicted_Total"}` or `nba-totals::Predicted_Total`. Kernel changes: dependency graph extends across `CubeId`s; dirty propagation crosses cubes; revision becomes per-workspace (or coherent across linked cubes).
- Workspace-scoped principals + permissions.
- Multi-cube transaction semantics (atomic write across N cubes — needed for "approve this scenario across all linked cubes simultaneously").
- Workspace snapshot / rollback (snapshot N cubes at one revision; rollback restores all of them).
- Workspace-level optimistic concurrency.

### Tier D — polish

- Workspace versioning / migration tooling.
- Workspace telemetry view.
- Domain pack installer (`mc plugin install domain sports-betting`).
- Workspace export formats (single-file bundle, archive, registry push).

---

## Open questions for the PM

These are the decisions that should not be silently made by the first implementation:

1. **Workspace identity.** Filesystem path? UUID? Both (`name` is human, internal `workspace_id` is stable)? Multi-instance sync needs an ID; single-user dev probably doesn't. Pick before the manifest format ships.
2. **Domain pack ownership model.** Anthropic-shipped? Plugin author? Customer? Open marketplace? This shapes whether domain packs go in the plugin repo, in `~/.mosaic/domains/`, or in a registry. **Recommendation:** ship in plugin repo (Tier A), introduce registry only if a real customer asks.
3. **`$ref:` resolution model.** Compile-time bake (workspace-validate inlines all refs into a synthetic per-cube YAML) vs hot-reload (cube re-validates whenever a referenced catalog changes). Bake is simpler; hot-reload is friendlier for active authoring. **Recommendation:** bake for v1; hot-reload is a Phase 6 (UI) concern.
4. **Cross-cube ref declaration site.** In the workspace manifest's `links:` block (DAG known up front, easier static analysis) or inline in the cube YAML (cube author owns the ref, like an import statement)? **Recommendation:** both — manifest holds the contract (declared up-front), cube uses syntax that references it. Hard-fail on undeclared cross-cube use.
5. **Single-file standalone cubes — do they get equal-citizen status forever, or sunset to "for scratch only"?** A user who only wants one cube shouldn't be forced into a workspace. But if workspaces become the default unit of work, single-file workflows risk slow rot.
6. **Cube portability across workspaces.** A cube that uses `$ref: "workspace://..."` is bound to its workspace. Should we also support `$ref: "domain://..."` (resolves to the installed domain pack) for cubes designed to be portable?
7. **First domain pack roadmap.** Marketing-mix exists. The strategic doc lists 5 more (sports-betting, FP&A, prospect-scoring, sales-forecasting, demand-planning). Which two are next, and which one is the "second proof schema" the strategic positioning calls for?
8. **Workspace as auth boundary?** Today, no auth exists in `mc-core`. When auth lands (Phase 6 per the strategic doc), is the workspace the *natural* permission scope, or is the domain? **Recommendation:** workspace. A user has access to a workspace; the domain is a property of the workspace, not the auth scope.

---

## Honest red flags

- **Tier A is doable but spec-heavy.** Six new manifest concepts (workspace.yaml, $ref schema, link manifest, domain pack format, workspace lint codes, workspace CLI surface) all need careful design before code. Risk: rushed spec produces a v1 that needs a v2 within 6 months.
- **Domain packs are the slowest part.** Each one is real domain expertise. Marketing-mix took the original author + Acme as a reference. Sports-betting has a sample cartridge but no pack. FP&A, prospect-scoring, sales-forecasting, demand-planning have neither — and a non-expert authoring those will produce generic-looking guidance that defeats the purpose.
- **Cross-cube refs (Tier C) are the moment dependency tracking gets genuinely hard.** Today's dirty-set is per-cube; cross-cube changes the kernel's mental model. Don't let that bleed into Tier A scope.
- **Workspace ≠ auth ≠ project ≠ domain.** Easy to conflate, easy to over-couple. Keep them distinct vocabulary words and pick one to be the auth boundary later, not now.
- **Single-cube users are the silent majority right now.** Whatever ships, the no-workspace flow has to stay first-class. A workspace must be *additive*, not *required*.

---

## Where it shows up in the engine (today, before any of this ships)

- **Spec:** [`../specs/engine-semantics.md`](../specs/engine-semantics.md) §20 (workspace as Phase 3+), §1230 (`I-Dep-6` cross-cube refs Phase 5+), §1314 (`I-Dirty-5` per-cube dirty set).
- **Strategy:** [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md) (general engine + specific schemas framing).
- **Plugin:** [`../../mosaic-plugin/skills/domain-schemas/marketing-mix/`](../../mosaic-plugin/skills/domain-schemas/) (the only existing domain pack).
- **Sample cartridge:** [`../../examples/sports-betting/`](../../examples/sports-betting/) (a cube without a domain pack).
- **Master plan:** [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) (Phase 4+ sequencing).

## Related notes

- [`./dual-fixture-claw-stress-test.md`](./dual-fixture-claw-stress-test.md) — proposal to use claw-core NBA totals as a second fixture; same "schema-as-product" framing.
- [`./formula-language-expansion.md`](./formula-language-expansion.md) — domain-by-domain formula coverage; confirms the multi-domain wedge.
- [`./model-as-judge-architecture.md`](./model-as-judge-architecture.md) — sports-betting-derived patterns that would benefit from a sports-betting domain pack.

## History

- 2026-05-06 — created during a multi-domain consult. Author: solutions-architecture instance after reviewing `mosaic-plugin/`, `docs/strategy/POSITIONING.md`, `docs/specs/engine-semantics.md` §20, and the existing marketing-mix + NBA-totals examples. Status: proposal, not adopted.
