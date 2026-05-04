# Phase 5B Handoff — LLM-Assisted Recipe Authoring

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 5B.
> **You inherit a green Phase 5A** (commit `6c9950d`, 502 / 0 tests).
> **Branch:** `phase-5b/llm-recipe-authoring` (create from `main`).
>
> **This phase ships new plugin skills, an agent, a command, and
> adapter updates** under `mosaic-plugin/` that teach LLMs how to
> author Tessera recipes. It does NOT touch any Rust crate.
>
> **Read [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
> Decision 7 (recipe format + semantic rules) and Decision 9 (5B row)
> BEFORE this handoff.** Also read
> [Amendment 2](../decisions/0010-amendment-2-long-format-recipe-support.md)
> for the long-format recipe extension the skills must document.
>
> **Process note:** this handoff was drafted under the
> **handoff-first parallel flow** (per [`../process-notes.md`](../process-notes.md) §1) after applying the 5-question self-test
> (all 5 yes — see "Self-test result" below). No new ADR required;
> ADR-0010 Decision 9 commits the strategic shape for 5B.
>
> **Hard rule:** Phase 5B touches ONLY `mosaic-plugin/` (new skills +
> agent + command) and `mosaic-plugin/examples/adapters/` (adapter
> updates). It does NOT touch any Rust crate (`mc-core`, `mc-recipe`,
> `mc-drivers`, `mc-tessera`, `mc-model`, `mc-fixtures`, `mc-cli` ALL
> locked).

---

## The one paragraph you must internalize before writing code

**Phase 5B is to Tessera recipes what Phase 4A was to YAML models.** The
plugin teaches the LLM how to author recipes; the LLM proposes recipes
that are structurally valid against the `mc-recipe` schema. The recipe
format IS the Phase 5B authoring surface — Phase 5A Stream B got it
right (the schema at `crates/mc-recipe/src/schema.rs`, the 6 semantic
rules in ADR-0010 Decision 7, the 18 MC5xxx codes); Phase 5B translates
that knowledge into LLM-readable skill content and wires it into the
plugin infrastructure (agent + command + adapter updates). The skills
must teach BOTH wide-format and long-format recipes (per Amendment 2),
all 6 supported drivers, all 6 semantic rules, and common pitfalls. The
end-to-end proof is: a natural-language description of a data source
goes in; a structurally valid recipe YAML comes out.

---

## Self-test result (handoff-first eligibility)

Per [`../process-notes.md`](../process-notes.md) §1's 5-question
self-test, Phase 5B passes all 5:

| # | Question | Phase 5B answer |
|---|---|---|
| 1 | Kernel change? | **No.** All Rust crates locked. |
| 2 | Runtime dep to any crate? | **No** in the Rust workspace. Python adapter deps are scoped to per-adapter `pyproject.toml` (unchanged from Phase 4B). |
| 3 | Contract shape change? | **No.** Recipe schema, diagnostic codes, plugin manifest format all unchanged. |
| 4 | < ~1500 LOC added? | **Yes.** ~800–1200 total: 3 new skill files + 1 agent + 1 command + adapter updates (~150 lines each). |
| 5 | Strategic decisions derivable from prior ADRs? | **Yes.** ADR-0010 Decision 9 commits 5B's shape; ADR-0008 amendments A + D + G define the adapter pattern. |

**Result: handoff-first parallel flow appropriate.**

---

## Where Phase 5A ended

- **Phase 5A merge commit:** `6c9950d` — 502 / 0 tests across all targets. 10/10 deterministic.
- **New crates shipped:** `mc-recipe` (recipe schema + validator), `mc-drivers` (6 source drivers), `mc-tessera` (orchestrator + CLI verbs).
- **Plugin content added in 5A Stream B:** `mosaic-plugin/skills/import/recipe-format/SKILL.md` — the canonical recipe-format skill (already exists; Phase 5B adds ADDITIONAL skills that build on it).
- **CLI verbs available:** `mc tessera {apply, dry-run, history, rollback, audit}`.
- **Example recipes at:** `crates/mc-recipe/examples/recipes/` (8 examples: CSV, SQLite, DuckDB, HTTP/JSON, Postgres, plus 3 intentionally-invalid recipes for diagnostic testing).
- **Recipe schema at:** `crates/mc-recipe/src/schema.rs` — the exact types the LLM authors against.
- **MC5xxx diagnostic codes:** MC5001–MC5018 stable (plus MC5019–MC5022 from Amendment 2 for long-format validation).
- **Phase 4B adapters at:** `mosaic-plugin/examples/adapters/{anthropic,openai}-python/author.py` — the iteration-loop architecture Phase 5B extends.
- **Plugin manifest at:** `mosaic-plugin/.claude-plugin/plugin.json` — commands array + agents array; skills auto-discovered.

---

## Phase 5B prompt (verbatim — this is your contract)

> We are starting Mosaic Phase 5B: LLM-Assisted Recipe Authoring.
>
> **Context.** Phase 5A shipped Tessera — the declarative recipe engine that imports external data into Mosaic cubes. Phase 5B is the authoring layer: plugin skills teach LLMs how to write recipes; the `/mosaic-import` command drives the full authoring flow; the Phase 4B Python adapters gain a `--mode propose-recipe` flag. If the LLM can produce a structurally valid recipe from a natural-language description, the "any data source via natural language" claim is proven.
>
> **Goal.** Ship new plugin skills, an agent, a command, and adapter updates such that:
>
> 1. The LLM has complete knowledge of the recipe schema (wide + long format), all 6 drivers, all 6 semantic rules, all MC5xxx codes, common mapping patterns, and worked examples for each driver type.
> 2. A natural-language description of a data source + target model produces a recipe that is structurally plausible (and machine-validated if `mc tessera dry-run` is available).
> 3. Both Python adapters can run the import-authoring flow end-to-end.
>
> **Phase 5B scope** (binding contract):
>
> 1. **New plugin skills at `mosaic-plugin/skills/import/`:**
>
>    - **`csv-mapping/SKILL.md`** — How to map CSV columns to cube dimensions and measures. Covers: wide-format (one measure per column) vs. long-format (measure name + value in dedicated columns per Amendment 2); header detection; date/time format strings; numeric scaling (`scale:`); skip patterns for irrelevant columns; common CSV pitfalls (encoding, delimiters, quoting — out of recipe scope but worth noting); worked examples for both wide and long CSV imports.
>
>    - **`sql-mapping/SKILL.md`** — How to write recipes for SQLite, DuckDB, and Postgres sources. Covers: `query:` vs `table:` mode (and their mutual exclusion — MC5003); credential handling via `${env.VAR}` interpolation; DuckDB-attached-Postgres (`duckdb_postgres` driver); SQL best practices for recipe queries (WHERE clauses to scope the import, column aliasing to match dim/measure names); worked examples for each of the 3 SQL-family drivers.
>
>    - **`api-mapping/SKILL.md`** — How to write recipes for HTTP/JSON endpoints. Covers: the `http_json` driver; `url:` field; `json_path:` for navigating nested JSON responses (`$.data.items[*]`); credential/auth patterns (`Bearer ${env.TOKEN}` in credentials); pagination limitations (Phase 5A HttpJsonDriver is GET-only, single-page — document this clearly); worked examples.
>
> 2. **New plugin agent: `mosaic-plugin/agents/mosaic-importer.md`**
>
>    The agent system prompt for recipe authoring. Responsibilities:
>    - Accept a natural-language description of the data source AND the target model's dimensions/measures.
>    - Know all 6 semantic rules (from ADR-0010 Decision 7 / recipe-format skill).
>    - Know both wide and long format (from Amendment 2 / csv-mapping skill).
>    - Propose a complete recipe YAML in a single fenced block.
>    - If validation fails (MC5xxx diagnostics), iterate using structured feedback (same pattern as mosaic-debugger for MC1xxx-MC3xxx).
>    - Hand off to mosaic-debugger if the diagnostics indicate a model-level issue (MC1xxx-MC3xxx) rather than a recipe issue (MC5xxx).
>
> 3. **New plugin command: `mosaic-plugin/commands/mosaic-import.md`**
>
>    `/mosaic-import "natural language description"` — invokes the mosaic-importer agent, produces a recipe YAML, runs validation (see validation strategy below), iterates on MC5xxx errors, and outputs the validated recipe for user review.
>
> 4. **Phase 4B Python adapter updates:**
>
>    Both `mosaic-plugin/examples/adapters/anthropic-python/author.py` and `mosaic-plugin/examples/adapters/openai-python/author.py` gain a `--mode propose-recipe` flag. When `--mode propose-recipe` is passed:
>    - The adapter loads the IMPORT-related plugin content (the new skills at `skills/import/`, the mosaic-importer agent, plus the existing recipe-format skill) instead of the model-authoring content.
>    - The adapter builds a system prompt from the import skills + agent + worked recipe examples.
>    - The iteration loop runs the RECIPE validation path (see validation strategy below) instead of the model-authoring path (`mc model validate/lint/test`).
>    - Output: a `.recipe.yaml` file (default: `output.recipe.yaml`).
>    - The existing `--mode author` (or no `--mode` flag) continues to work exactly as before (model-authoring path unchanged).
>
> 5. **Update `mosaic-plugin/.claude-plugin/plugin.json`:**
>    - Add `"./commands/mosaic-import.md"` to the `commands` array.
>    - Add `"./agents/mosaic-importer.md"` to the `agents` array.
>    - Skills are auto-discovered (no manifest change needed for new skill files).
>
> 6. **Validation strategy (critical — read carefully):**
>
>    The Phase 4B adapters validated model YAML via `mc model validate --format json`. For recipes, the equivalent is `mc tessera dry-run <recipe> --format json`.
>
>    **Two possible states at Phase 5B execution time:**
>
>    - **If `mc tessera dry-run` exists and emits JSON diagnostics:** the adapter's iteration loop calls `mc tessera dry-run <recipe.yaml> --format json`, parses the MC5xxx diagnostic envelope (same JSON shape as model diagnostics), feeds structured feedback to the LLM, and iterates. This is the ideal path.
>
>    - **If `mc tessera dry-run` does NOT exist or does NOT support `--format json`:** the adapter uses LLM self-validation. The skills teach the recipe schema and all 6 semantic rules thoroughly enough that the LLM can self-check its output against the rules. The iteration loop becomes: (a) LLM proposes recipe, (b) adapter asks LLM "validate this recipe against the 6 semantic rules and the MC5xxx code table — report any issues," (c) if issues found, LLM corrects and re-proposes. No subprocess validation.
>
>    **The adapter MUST detect which path is available at runtime** (e.g., `which mc && mc tessera dry-run --help` or similar probe). Document both paths clearly. The machine-validated path is preferred; the self-validated path is the fallback.
>
>    **Known limitation (document explicitly):** when using self-validation, the recipe is "structurally plausible per the skill content" but NOT machine-validated against the actual target model. The LLM cannot verify that dimension/measure names exist in the model without loading the model. This gap closes in Phase 5B.1 when `mc tessera dry-run` is guaranteed available.
>
> 7. **End-to-end proof (best-of-3 per Phase 4B pattern):**
>
>    The canonical acceptance prompt is:
>
>    > *"import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"*
>
>    Run **3 times against each adapter** (6 runs total, `--mode propose-recipe`). For each adapter, **at least 2 of 3 runs must produce a recipe that passes validation:**
>    - If `mc tessera dry-run` is available: the recipe passes `mc tessera dry-run <recipe> --format json` with zero MC5xxx errors.
>    - If `mc tessera dry-run` is NOT available: the recipe is structurally valid per manual inspection against the `mc-recipe` schema (correct `version: 1`, valid `driver:`, well-formed `columns:` array with proper dimension/measure mappings, no semantic rule violations).
>
>    Document which validation path was used. Capture all 6 runs in transcripts at `docs/reports/phase-5b-proof/`.
>
> **Hard rules:**
>
> - **All Rust crates LOCKED.** No source change in any crate. `git diff 6c9950d -- crates/` returns 0 lines. `cargo test --workspace` still produces 502 / 0 at HEAD.
> - **Plugin's existing skills/agents/commands from Phase 4A/5A LOCKED.** Do NOT modify `skills/authoring/`, `skills/debugging/`, `skills/formulas/`, `skills/domain-schemas/`, `skills/import/recipe-format/` (the existing one), `agents/mosaic-architect.md`, `agents/mosaic-author.md`, `agents/mosaic-debugger.md`, `agents/mosaic-validator.md`, `commands/mosaic-init.md`, `commands/mosaic-validate.md`, `commands/mosaic-inspect.md`, `commands/mosaic-lint.md`, `commands/mosaic-test.md`, `commands/mosaic-author.md`. Only ADD new files.
> - **Toolchain:** Rust stays at 1.78. Cargo.lock pins intact. Python: minimum 3.10 (unchanged from Phase 4B).
> - **No new Rust deps.** Period.
> - **No new Python deps** beyond what Phase 4B already declared (`anthropic` in one adapter, `openai` in the other). No `pyyaml`, `pydantic`, etc.
> - **No async, no concurrency** in the adapters.
> - **No new diagnostic codes.** MC5xxx codes are stable from Phase 5A. Phase 5B adds zero codes.
> - **Marketing-mix (Acme) is the ONLY domain** exercised in the proof.
> - **`mc tessera propose` CLI verb DEFERRED to Phase 5B.1.** It requires mc-tessera changes (Rust crate modification); Phase 5B is plugin-only.
> - **No provider-specific content in the plugin.** Import skills, agent, and command are provider-agnostic markdown. Provider coupling lives only in the adapter `.py` files.
>
> **Acceptance gate (best-of-3):**
>
> Headline: **For each adapter, >= 2 of 3 runs of the canonical acceptance prompt (with `--mode propose-recipe`) produce a recipe that passes validation.** Document which validation path was used (machine or self).
>
> Supporting:
>
> 1. `mosaic-plugin/skills/import/csv-mapping/SKILL.md` exists and covers wide + long format.
> 2. `mosaic-plugin/skills/import/sql-mapping/SKILL.md` exists and covers SQLite/DuckDB/Postgres.
> 3. `mosaic-plugin/skills/import/api-mapping/SKILL.md` exists and covers HTTP/JSON.
> 4. `mosaic-plugin/agents/mosaic-importer.md` exists with proper frontmatter and system prompt.
> 5. `mosaic-plugin/commands/mosaic-import.md` exists with proper frontmatter.
> 6. `mosaic-plugin/.claude-plugin/plugin.json` updated with the new command + agent.
> 7. Both adapters support `--mode propose-recipe` and produce `.recipe.yaml` output.
> 8. Both adapters' existing `--mode author` (model-authoring) path still works unchanged.
> 9. Best-of-3 gate passes for both adapters.
> 10. Plugin's existing skills/agents/commands unchanged (`git diff 6c9950d -- mosaic-plugin/skills/authoring/ mosaic-plugin/skills/debugging/ mosaic-plugin/skills/formulas/ mosaic-plugin/skills/domain-schemas/ mosaic-plugin/skills/import/recipe-format/ mosaic-plugin/agents/mosaic-architect.md mosaic-plugin/agents/mosaic-author.md mosaic-plugin/agents/mosaic-debugger.md mosaic-plugin/agents/mosaic-validator.md` returns 0 lines).
> 11. Rust workspace unchanged (`git diff 6c9950d -- crates/` returns 0 lines).
> 12. All 502 existing Rust tests still pass.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (502 / 0)
> - `git diff 6c9950d -- crates/` (zero lines)
> - `git diff 6c9950d -- mosaic-plugin/skills/authoring/ mosaic-plugin/skills/debugging/ mosaic-plugin/skills/formulas/ mosaic-plugin/skills/domain-schemas/ mosaic-plugin/skills/import/recipe-format/ mosaic-plugin/agents/mosaic-architect.md mosaic-plugin/agents/mosaic-author.md mosaic-plugin/agents/mosaic-debugger.md mosaic-plugin/agents/mosaic-validator.md` (zero lines)
> - 3 acceptance runs against each adapter with `--mode propose-recipe`
> - Document validation path used + per-run outcomes
>
> **SPEC QUESTION triggers:**
>
> 1. **Recipe schema needs a field the skills don't cover** — e.g., the Amendment 2 long-format fields (`format:`, `long_format:`) have a shape the skills can't express clearly. Surface before guessing.
> 2. **`mc tessera dry-run` is unavailable or doesn't support `--format json`** — fall back to self-validation path (documented above); no SPEC QUESTION needed unless BOTH paths produce < 2/3 convergence.
> 3. **Plugin manifest format changed since Phase 4A** — if `.claude-plugin/plugin.json` has a different shape than documented here (new required fields, changed array semantics), surface.
> 4. **Both adapters consistently fail to converge** within 5 iterations on the canonical acceptance prompt. Same trigger logic as Phase 4B: surface BEFORE bumping `max_iterations`.
> 5. **Adapter exceeds ~300 lines after adding `--mode propose-recipe`** — the dual-mode addition should be ~100–150 lines on top of the existing ~265-line `author.py`. If it balloons past 400 total, consider a separate `propose_recipe.py` entry point instead. Surface before splitting.
> 6. **The existing recipe-format skill (`skills/import/recipe-format/SKILL.md`) has a content gap** that blocks recipe authoring (e.g., wrong example, missing semantic rule). Surface as a SPEC QUESTION + Phase 5A.2 follow-up commit; do NOT modify the locked file.
>
> **Completion report format:**
>
> ```
> DONE: Phase 5B LLM-Assisted Recipe Authoring
>
> Build:    cargo build --release --workspace         (unchanged)
> Format:   cargo fmt --check --all                   (unchanged)
> Lint:     cargo clippy --workspace --all-targets -- -D warnings (unchanged)
> Tests:    cargo test --workspace 502 / 0            (unchanged)
> Locked surfaces:
>   git diff 6c9950d -- crates/ 0 lines
>   git diff 6c9950d -- <locked plugin paths> 0 lines
> Validation path used: [machine via mc tessera dry-run | self-validation]
> Anthropic adapter (best-of-3, --mode propose-recipe):
>   Run 1: ... (converged/failed; validation outcome)
>   Run 2: ...
>   Run 3: ...
>   PASSING RUNS: <count>/3 (gate: >= 2/3)
> OpenAI adapter (best-of-3):
>   (same shape)
>
> Source manifest:
> - mosaic-plugin/skills/import/csv-mapping/SKILL.md       (NEW)
> - mosaic-plugin/skills/import/sql-mapping/SKILL.md       (NEW)
> - mosaic-plugin/skills/import/api-mapping/SKILL.md       (NEW)
> - mosaic-plugin/agents/mosaic-importer.md                (NEW)
> - mosaic-plugin/commands/mosaic-import.md                (NEW)
> - mosaic-plugin/.claude-plugin/plugin.json               (modified — +1 command, +1 agent)
> - mosaic-plugin/examples/adapters/anthropic-python/author.py (modified — +propose-recipe mode)
> - mosaic-plugin/examples/adapters/openai-python/author.py    (modified — +propose-recipe mode)
> - docs/reports/phase-5b-completion-report.md             (NEW)
> - docs/reports/phase-5b-proof/transcript-anthropic.md    (NEW)
> - docs/reports/phase-5b-proof/transcript-openai.md       (NEW)
> - docs/reports/phase-5b-proof/output-anthropic.recipe.yaml (NEW)
> - docs/reports/phase-5b-proof/output-openai.recipe.yaml    (NEW)
> - docs/CURRENT_STATE.md                                  (modified)
> - docs/roadmap/MASTER_PHASE_PLAN.md                      (modified)
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The recipe schema (the authoring surface)

The exact Rust types the LLM authors against live at `crates/mc-recipe/src/schema.rs`. Key types:

- `Recipe` — top-level struct with all fields
- `SourceConfig` — driver + path/query/table/url/json_path
- `DriverKind` — enum: `Csv`, `Sqlite`, `Duckdb`, `Postgres`, `DuckdbPostgres`, `HttpJson` (serialized as snake_case)
- `ColumnMapping` — source + dimension/measure/type/scale/format/skip
- `WriteDisposition` — Phase 5A: only `Replace`
- `OnError` — `Abort`, `SkipRow`, `Quarantine`
- `OnMissingElement` — Phase 5A: only `Error`
- `BatchConfig` — size (default 50K)

The skills must teach this schema precisely — field names, enum values (lowercase snake_case in YAML), optional vs required, defaults.

### B. Long-format recipes (Amendment 2)

Amendment 2 adds `format: long` + `long_format: { measure_column: ..., value_column: ... }` to the source config. Long-format is where each row is one cell (measure name in a column, value in another). The skills must cover BOTH formats because real-world data comes in both shapes. See `docs/decisions/0010-amendment-2-long-format-recipe-support.md` for the full spec + example.

### C. Worked example recipes (few-shot references for the LLM)

Located at `crates/mc-recipe/examples/recipes/`:

- `acme-csv-import.recipe.yaml` — CSV wide-format
- `acme-sqlite-import.recipe.yaml` — SQLite with query
- `acme-duckdb-import.recipe.yaml` — DuckDB with query
- `acme-postgres-import.recipe.yaml` — Postgres with DSN credential
- `acme-http-json-import.recipe.yaml` — HTTP/JSON with json_path
- `acme-invalid-derived.recipe.yaml` — fires MC5018 (Derived measure)
- `acme-invalid-mutual-exclusion.recipe.yaml` — fires MC5016
- `acme-invalid-unknown-dim.recipe.yaml` — fires MC5004

The adapter's system prompt should include at least 2-3 of the valid examples as few-shot references.

### D. The existing recipe-format skill

`mosaic-plugin/skills/import/recipe-format/SKILL.md` already covers:
- The full schema (all fields with types and MC5xxx codes)
- All 6 semantic rules with worked right/wrong examples
- All 18 MC5xxx codes with fire conditions and fixes
- Driver-by-driver examples (CSV, SQLite, DuckDB, Postgres, DuckDB-attached-Postgres, HTTP/JSON)
- Common authoring mistakes

The NEW skills (csv-mapping, sql-mapping, api-mapping) go DEEPER on their respective driver families — patterns, anti-patterns, common data shapes, format strings, credential patterns. They are complements to recipe-format, not replacements.

### E. The Phase 4B adapter architecture (what you're extending)

Both adapters at `mosaic-plugin/examples/adapters/{anthropic,openai}-python/author.py` follow the same pattern:

1. `find_plugin_root()` — walk up from `__file__`
2. `load_plugin_content(root)` — read all skills/agents/commands + Acme example
3. `build_system_prompt(content)` — preamble + content + response-format instruction
4. `call_provider(client, system, messages)` — single API call
5. `extract_yaml(response)` — strip fences
6. Iteration loop: validate → lint → test → feedback → retry

For `--mode propose-recipe`, you modify steps 2 (load import-specific content), 3 (recipe-oriented preamble), 5 (extract recipe YAML), and the iteration loop (recipe validation instead of model validation).

### F. The diagnostic JSON envelope

Same shape for recipe diagnostics as model diagnostics:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC5004",
      "severity": "error",
      "path": "/columns/2/dimension",
      "message": "column \"market_region\" references unknown dimension \"Region\""
    }
  ]
}
```

If `mc tessera dry-run` emits this envelope, the adapter parses it identically to model diagnostics.

### G. The canonical acceptance prompt and why it works

*"import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"*

This forces:
- **SQLite driver** — tests the SQL-mapping skill
- **Spend + CPC** — both are Input measures in Acme (the LLM must NOT also map Clicks, which is Derived)
- **monthly** — implies Time dimension with monthly elements
- **HubSpot campaign metrics** — anchors the source-table naming
- **Acme marketing-mix model** — the LLM knows the model's dims/measures from the domain-schema skill

The recipe should look similar to `crates/mc-recipe/examples/recipes/acme-sqlite-import.recipe.yaml` but scoped to only Spend + CPC (not all 6 Input measures).

---

## Pointers to existing files you will most likely touch

| Why | File | Phase 5B action |
|---|---|---|
| CSV mapping skill | `mosaic-plugin/skills/import/csv-mapping/SKILL.md` | new |
| SQL mapping skill | `mosaic-plugin/skills/import/sql-mapping/SKILL.md` | new |
| API mapping skill | `mosaic-plugin/skills/import/api-mapping/SKILL.md` | new |
| Importer agent | `mosaic-plugin/agents/mosaic-importer.md` | new |
| Import command | `mosaic-plugin/commands/mosaic-import.md` | new |
| Plugin manifest | `mosaic-plugin/.claude-plugin/plugin.json` | modify (+1 command, +1 agent) |
| Anthropic adapter | `mosaic-plugin/examples/adapters/anthropic-python/author.py` | modify (+propose-recipe mode) |
| OpenAI adapter | `mosaic-plugin/examples/adapters/openai-python/author.py` | modify (+propose-recipe mode) |
| Completion report | `docs/reports/phase-5b-completion-report.md` | new |
| Proof transcripts | `docs/reports/phase-5b-proof/` | new dir |
| Status flips | `docs/CURRENT_STATE.md`, `docs/roadmap/MASTER_PHASE_PLAN.md` | flip 5B |

**Do not touch:**

- **`crates/`** — entire Rust workspace locked. Zero source changes.
- **Existing plugin skills** — `skills/authoring/`, `skills/debugging/`, `skills/formulas/`, `skills/domain-schemas/`, `skills/import/recipe-format/` ALL locked.
- **Existing plugin agents** — `mosaic-architect.md`, `mosaic-author.md`, `mosaic-debugger.md`, `mosaic-validator.md` locked.
- **Existing plugin commands** — all 6 existing commands locked.
- **`mosaic-plugin/.mcp.json`** — locked.
- **`mosaic-plugin/examples/models/`** — locked.
- **`mosaic-plugin/hooks/`** — locked.
- **`docs/specs/`** — locked.
- **`docs/decisions/`** — locked (do not modify ADR-0010 or any earlier ADR).
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock`** — pins all stay.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

source $HOME/.cargo/env

# Pre-5B gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                    # 502 / 0

# `mc` install precondition
which mc || cargo install --path crates/mc-cli --locked
mc --version

# Check if mc tessera dry-run is available (determines validation path)
mc tessera dry-run --help 2>/dev/null && echo "MACHINE VALIDATION AVAILABLE" || echo "SELF-VALIDATION FALLBACK"

# Verify locked surfaces
git diff 6c9950d -- crates/
# expected: zero output

# Adapter dev (Anthropic, propose-recipe mode)
cd mosaic-plugin/examples/adapters/anthropic-python
pip install -e .
export ANTHROPIC_API_KEY=<your-key>
python author.py --mode propose-recipe "import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"
# -> produces output.recipe.yaml

# If machine validation available:
mc tessera dry-run output.recipe.yaml --format json

# Adapter dev (OpenAI, propose-recipe mode)
cd ../openai-python
pip install -e .
export OPENAI_API_KEY=<your-key>
python author.py --mode propose-recipe "import monthly spend and CPC data from a SQLite database of HubSpot campaign metrics into the Acme marketing-mix model"

# Reference: existing recipe examples
ls crates/mc-recipe/examples/recipes/
cat crates/mc-recipe/examples/recipes/acme-sqlite-import.recipe.yaml

# Reference: the recipe schema
cat crates/mc-recipe/src/schema.rs

# Reference: Phase 4B's in-session proof (structural template)
ls docs/reports/phase-4b-proof/
```

---

## Final checklist before you call Phase 5B done

- [ ] `mosaic-plugin/skills/import/csv-mapping/SKILL.md` exists and covers wide + long format with worked examples.
- [ ] `mosaic-plugin/skills/import/sql-mapping/SKILL.md` exists and covers SQLite/DuckDB/Postgres with query vs table mode.
- [ ] `mosaic-plugin/skills/import/api-mapping/SKILL.md` exists and covers HTTP/JSON with json_path.
- [ ] `mosaic-plugin/agents/mosaic-importer.md` exists with proper frontmatter and system prompt.
- [ ] `mosaic-plugin/commands/mosaic-import.md` exists with proper frontmatter.
- [ ] `mosaic-plugin/.claude-plugin/plugin.json` updated with the new command + agent.
- [ ] Both adapters support `--mode propose-recipe` and produce `.recipe.yaml` output.
- [ ] Both adapters' existing mode (model-authoring) still works unchanged.
- [ ] **Each adapter ran 3 times against the canonical acceptance prompt with `--mode propose-recipe`; >= 2/3 runs per adapter produce a valid recipe.**
- [ ] Validation path documented (machine via `mc tessera dry-run` OR self-validation).
- [ ] If self-validation used: recipes manually inspected against `mc-recipe` schema for structural correctness.
- [ ] Plugin's existing skills/agents/commands unchanged (git diff gate passes).
- [ ] Rust workspace unchanged (git diff gate passes).
- [ ] All 502 existing Rust tests still pass.
- [ ] `cargo fmt --check`, `cargo clippy`, `cargo build --release` all clean.
- [ ] No new Rust deps in any crate.
- [ ] No new Python deps beyond existing `anthropic`/`openai`.
- [ ] No async, no concurrency, no streaming in adapters.
- [ ] No new diagnostic codes; MC5xxx codes unchanged.
- [ ] Marketing-mix (Acme) is the only domain exercised.
- [ ] `docs/reports/phase-5b-proof/` contains: `transcript-anthropic.md`, `transcript-openai.md`, `output-anthropic.recipe.yaml`, `output-openai.recipe.yaml`.
- [ ] Completion report at `docs/reports/phase-5b-completion-report.md`.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated.
- [ ] **You did NOT commit, tag, or push.** The user does that after review.
- [ ] **You did NOT modify any existing plugin content** (skills/agents/commands that existed before 5B).
- [ ] **You did NOT modify any Rust crate.**
- [ ] **You did NOT implement `mc tessera propose` CLI verb** (deferred to 5B.1).
- [ ] **You did NOT add a `propose_recipe.py` entry point** unless `author.py` exceeded ~400 lines (SPEC QUESTION trigger #5).

---

## Operating principles (carry-forward)

**The skills are the deliverable. The adapters are the proof.** If you spend 80% of your time on adapter logic and 20% on skill content, you're doing it backwards. The skills must teach the recipe schema so thoroughly that any LLM reading them can produce a valid recipe. The adapters just wire that knowledge to a provider API.

**Source-bounded.** Phase 5B touches `mosaic-plugin/` (new files + manifest update + adapter modifications). Nothing else.

**The existing recipe-format skill is the foundation; the new skills are extensions.** `csv-mapping`, `sql-mapping`, and `api-mapping` go deeper on their respective driver families. They reference the recipe-format skill for the full schema; they don't repeat it.

**Wide + long format.** Both must be documented. Amendment 2 added long-format; real-world data uses both. The LLM needs to know when to use each and how the recipe schema differs between them.

**Semantic rules are non-negotiable.** All 6 rules from ADR-0010 Decision 7 must be taught. The most common authoring mistake is targeting a Derived measure (Rule 3 / MC5018). The skills must make this crystal clear.

**Two validation paths; prefer machine.** If `mc tessera dry-run` works at runtime, use it. If not, self-validation is acceptable for Phase 5B — the LLM knows the rules from the skills. Document which path was used.

If at any point you find yourself needing to modify a Rust crate, STOP. That's out of scope. Surface as a SPEC QUESTION.

---

*Phase 5B handoff drafted 2026-05-04 after Phase 5A shipped at `6c9950d` (502/0 tests). Per `docs/process-notes.md` §1's 5-question self-test, Phase 5B is eligible for the handoff-first parallel flow (all 5 questions yes). ADR-0010 Decision 9 commits the strategic shape; this handoff is the binding implementation contract.*
