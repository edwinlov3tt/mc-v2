# Phase 5B Completion Report — LLM-Assisted Recipe Authoring

**Status:** Complete, pending user review. **NOT committed, NOT tagged, NOT pushed.**
**Date:** 2026-05-04
**Branch:** `phase-5b/llm-recipe-authoring` (off `main` at `0857865`).
**Inherits:** Phase 5A merge state (Streams A+B+C; Stream D in-flight per its handoff).
**Scope-bound to:** `mosaic-plugin/` (new skills + agent + command + manifest update) and `mosaic-plugin/examples/adapters/` (adapter `--mode propose-recipe` extension). Zero Rust crate modifications.

---

## Headline result

**Best-of-3 acceptance gate: 3 / 3 per adapter — both adapters cleared by maximum margin (gate: ≥ 2/3).**

| Adapter | Run 1 | Run 2 | Run 3 | Pass | Validation path |
|---|---|---|---|---|---|
| Anthropic (`claude-opus-4-7`) | converged @ iter 1 | converged @ iter 1 | converged @ iter 1 | **3 / 3 ✓** | structural |
| OpenAI (`gpt-5.5`) | converged @ iter 1 | converged @ iter 1 | converged @ iter 1 | **3 / 3 ✓** | structural |

Every recipe correctly:
- Targets only Input measures (`Spend`, `CPC`) — NOT any Acme Derived (Clicks/Leads/Customers/Revenue/Gross_Profit). Rule 3 / MC5018 satisfied.
- Picks `driver: sqlite` matching the prompt's source description.
- Pins Scenario / Version via `defaults:`; varies Time / Channel / Market via `columns:`. Rule 2 / MC5016 satisfied.
- Uses `version: 1` + `write_disposition: replace` + `incremental: false` + `on_missing_element: error` (Phase 5A defaults).

See [`phase-5b-proof/transcript-anthropic.md`](./phase-5b-proof/transcript-anthropic.md) and [`phase-5b-proof/transcript-openai.md`](./phase-5b-proof/transcript-openai.md) for full transcripts; raw run artifacts (recipe YAMLs + stdout/stderr) at [`phase-5b-proof/runs/`](./phase-5b-proof/runs/).

---

## Validation path used

**Structural self-validation** (the Phase 5B handoff's documented fallback path).

`mc tessera dry-run` is not available on PATH because `mc-tessera` (Stream D's deliverable) is in-flight per the [Stream D handoff](../handoffs/phase-5a-stream-d-handoff.md) — the workspace does not yet declare a `crates/mc-tessera` member, and the installed `mc` binary's `--help` enumerates `demo`, `model {validate,inspect,lint,test}`, and `mcp` only (no `tessera` verbs).

The adapters' `tessera_dry_run_available()` runtime probe detected this (returned `False`) and selected the structural path automatically. When Stream D ships, the same probe will return `True` and the adapters will switch to the machine path with no code change.

Structural validator coverage (per `structural_validate_recipe()` in both adapters):

| Check | MC code | Implemented |
|---|---|---|
| Tab indentation | MC5001 | ✓ |
| Required top-level fields (`version`/`name`/`model`/`source`/`columns`) | MC5007 | ✓ |
| `version: 1` pin | MC5012 | ✓ |
| Driver in 6 supported set (`csv` / `sqlite` / `duckdb` / `postgres` / `duckdb_postgres` / `http_json`) | MC5002 | ✓ |
| `query:` / `table:` mutual exclusion | MC5003 | ✓ |
| No mapping to Acme Derived measure | MC5018 | ✓ (hardcoded Acme Derived set) |
| `format: long` rejection (5A.1-pending) | MC5001 | ✓ |
| Unknown dim / measure name | MC5004 / MC5005 | ✗ (needs live model load — gap closes when `mc tessera dry-run` lands) |
| Mutual exclusion `columns:` ↔ `defaults:` | MC5016 | ✗ (needs indentation-aware YAML parsing) |
| Unknown element value | MC5009 | ✗ (needs live model load) |

Per the Phase 5B handoff §6 documented limitation, the structural path is "structurally plausible per the skill content" but not equivalent to the machine path. The 6-of-6 convergence in this gate is consistent with — but does not prove — the recipes would also pass `mc tessera dry-run` once it ships.

---

## Build / lint / format / test gate

| Gate | Command | Status |
|---|---|---|
| Format | `cargo fmt --check --all` | ✓ exit 0 |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✓ exit 0 (zero warnings) |
| Build | `cargo build --release --workspace` | ✓ zero warnings (incremental cache) |
| Tests | `cargo test --workspace` | ✓ **502 / 0** (unchanged from Phase 5A merge state) |
| Locked: Rust workspace | `git diff 6c9950d -- crates/` | ✓ 0 lines |
| Locked: existing plugin content | `git diff 6c9950d -- mosaic-plugin/skills/{authoring,debugging,formulas,domain-schemas,import/recipe-format}/ mosaic-plugin/agents/{mosaic-architect,mosaic-author,mosaic-debugger,mosaic-validator}.md` | ✓ 0 lines |

---

## Source manifest

Files added or modified by Phase 5B:

```
mosaic-plugin/skills/import/csv-mapping/SKILL.md         (NEW)
mosaic-plugin/skills/import/sql-mapping/SKILL.md         (NEW)
mosaic-plugin/skills/import/api-mapping/SKILL.md         (NEW)
mosaic-plugin/agents/mosaic-importer.md                  (NEW)
mosaic-plugin/commands/mosaic-import.md                  (NEW)
mosaic-plugin/.claude-plugin/plugin.json                 (modified — +1 command, +1 agent)
mosaic-plugin/examples/adapters/anthropic-python/author.py (modified — +propose-recipe mode; 267 → 477 lines)
mosaic-plugin/examples/adapters/openai-python/author.py    (modified — +propose-recipe mode; 263 → 467 lines)

docs/reports/phase-5b-completion-report.md               (NEW — this file)
docs/reports/phase-5b-proof/transcript-anthropic.md      (NEW)
docs/reports/phase-5b-proof/transcript-openai.md         (NEW)
docs/reports/phase-5b-proof/output-anthropic.recipe.yaml (NEW — copy of runs/anthropic-1.recipe.yaml)
docs/reports/phase-5b-proof/output-openai.recipe.yaml    (NEW — copy of runs/openai-1.recipe.yaml)
docs/reports/phase-5b-proof/runs/                        (NEW — 18 files: per-run recipe.yaml + stdout.txt + stderr.txt × 6 runs)
docs/CURRENT_STATE.md                                    (modified — Phase 5A + 5B status entries)
docs/roadmap/MASTER_PHASE_PLAN.md                        (modified — Phase 5 sub-phase status block)
```

Zero changes outside this manifest. No Rust source modified, no `Cargo.toml`/`Cargo.lock` modified, no toolchain change, no new Python deps.

---

## Notes on adapter line counts (SPEC QUESTION trigger #5)

Both adapters grew past the 400-line soft ceiling mentioned in the Phase 5B handoff's SPEC QUESTION trigger #5:

- `anthropic-python/author.py`: 267 → 477 lines (+210)
- `openai-python/author.py`: 263 → 467 lines (+204)

The handoff trigger reads: *"If it balloons past 400 total, **consider** a separate `propose_recipe.py` entry point instead. **Surface before splitting.**"* — i.e., the trigger fires before splitting, not before staying with the dual-mode flag.

I did NOT split because:

1. The Phase 5B prompt (handoff §A.4 / canonical prompt at line 117) explicitly directs: *"Both `mosaic-plugin/examples/adapters/{anthropic,openai}-python/author.py` gain a `--mode propose-recipe` flag."* That's the dual-mode pattern, not a split.
2. The +210 / +204 lines split as: ~25 lines content loader, ~10 lines preamble builder, ~5 lines runtime probe, ~50 lines structural validator, ~50 lines `propose_recipe()` loop, ~25 lines main-dispatch refactor, ~15 lines new constants. None of these are bloat — each is load-bearing.
3. Splitting would duplicate the shared helpers (`find_plugin_root`, `extract_yaml`, `run_mc`, `parse_envelope`, `diagnostics_by_severity`, `format_feedback`, `call_provider`, `re_request`) into a second file, increasing total LOC across the two entry points rather than reducing it.

The lines are conservative and the dual-mode flow is the user-requested shape. Surfacing here for review per the trigger's "surface before splitting" wording — if you'd prefer the split into `propose_recipe.py`, that's a follow-up commit; the test substance is unchanged.

---

## Phase 5B acceptance gate (per handoff)

| # | Item | Status |
|---|---|---|
| 1 | `mosaic-plugin/skills/import/csv-mapping/SKILL.md` exists, covers wide + long format | ✓ |
| 2 | `mosaic-plugin/skills/import/sql-mapping/SKILL.md` exists, covers SQLite/DuckDB/Postgres | ✓ |
| 3 | `mosaic-plugin/skills/import/api-mapping/SKILL.md` exists, covers HTTP/JSON | ✓ |
| 4 | `mosaic-plugin/agents/mosaic-importer.md` exists with frontmatter | ✓ |
| 5 | `mosaic-plugin/commands/mosaic-import.md` exists with frontmatter | ✓ |
| 6 | `mosaic-plugin/.claude-plugin/plugin.json` updated with new command + agent | ✓ |
| 7 | Both adapters support `--mode propose-recipe` and produce `.recipe.yaml` | ✓ |
| 8 | Both adapters' existing `--mode author` path still works | ✓ (default-mode preserved; --help shows mode flag with `author` default) |
| 9 | Best-of-3 gate passes for both adapters | ✓ **3/3 each** |
| 10 | Plugin's existing skills/agents/commands unchanged | ✓ (locked-surfaces git diff = 0 lines) |
| 11 | Rust workspace unchanged | ✓ (`git diff 6c9950d -- crates/` = 0 lines) |
| 12 | All 502 existing Rust tests still pass | ✓ |

All 12 acceptance items cleared.

---

## Operating-principle adherence

Per the handoff's "carry-forward" operating principles:

- **The skills are the deliverable; the adapters are the proof.** Skill content totals ~1,400 lines of dense, schema-precise markdown across 3 new files. Adapter additions are mechanical. ✓
- **Source-bounded.** Phase 5B touched `mosaic-plugin/` (skills + agent + command + manifest) and `mosaic-plugin/examples/adapters/` (adapter modifications). Plus the proof transcripts and status flips under `docs/`. Nothing else. ✓
- **The existing recipe-format skill is the foundation; the new skills are extensions.** All three new skills cross-reference `recipe-format/SKILL.md` for the full schema and the 18 MC5xxx codes; they don't repeat the foundation. ✓
- **Wide + long format.** Both documented in `csv-mapping/SKILL.md`. Long-format is clearly marked as Phase 5A.1-pending (filed in ADR-0010 Amendment 2; not yet in `mc-recipe` schema). ✓
- **Semantic rules are non-negotiable.** All 6 rules are taught in every relevant skill + the importer agent + the import command. Rule 3 (Input-only) is the highest-stakes rule and is consistently emphasized. ✓
- **Two validation paths; prefer machine.** Adapters probe at runtime; structural is the documented fallback when `mc tessera dry-run` doesn't exist. ✓ (structural path used in this gate; machine path will activate automatically once Stream D merges).

---

## What ships next (Phase 5B.1 candidates — not Phase 5B)

These are deliberately deferred per the handoff's hard-locks:

- **`mc tessera propose` CLI verb** — requires `mc-tessera` (Stream D's in-flight crate). Phase 5B.1 once Stream D merges.
- **Machine-validation path activation** — once `mc tessera dry-run --format json` is available on PATH, the adapter's runtime probe selects it automatically; the documented "structurally plausible but not machine-validated" gap closes.
- **Structural validator MC5004 / MC5005 / MC5009 / MC5016 detection** — gaps in the regex-based fallback. Closes when the machine path activates and structural becomes a redundant secondary check rather than the primary.
- **Long-format recipe support** — Phase 5A.1 per ADR-0010 Amendment 2. Skills already document the shape; the live `mc-recipe` schema gains `format:` + `long_format:` fields then.
- **Adapter line-count split** — only if the user prefers `propose_recipe.py` as a separate entry point per the handoff's SPEC QUESTION trigger #5 wording.

---

## Cross-references

- [Phase 5B handoff](../handoffs/phase-5b-handoff.md) — the binding contract this report fulfills.
- [ADR-0010 Phase 5 Tessera architecture](../decisions/0010-phase-5-tessera-architecture.md) Decision 7 (recipe format + 6 semantic rules) and Decision 9 (5B's place in sub-phase decomposition).
- [ADR-0010 Amendment 2: long-format recipe support](../decisions/0010-amendment-2-long-format-recipe-support.md) — the Phase 5A.1 long-format spec the new csv-mapping skill documents.
- [Phase 4B completion report](./phase-4b-completion-report.md) — the precedent best-of-3 gate Phase 5B mirrors.
- [Phase 5B proof bundle](./phase-5b-proof/) — transcripts + recipe outputs + per-run logs.
