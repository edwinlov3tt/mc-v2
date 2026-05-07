# Phase 7A.2 Handoff — Interpretation Ledger

> **Audience:** the Claude Code instance that implements Phase 7A.2.
> **You inherit `main` at `cc4e27d` (955 / 0 / 5 tests). You'll work
> on the branch `phase-7a-2/interpretation-ledger`.**
>
> **This phase makes narrative output durable.** Phase 7A.1 shipped
> the narrative engine (`mc-narrative` crate + `mc model narrate` verb).
> Every narrate invocation currently produces output that vanishes
> when the process exits. Phase 7A.2 persists every narrative as a
> structured analysis event in a JSONL ledger file. This creates the
> foundation for Phase 7A.3 (cross-period analysis: "this is the
> third consecutive month...") and Phase 7A.4 (benchmark aggregation).
>
> **The binding design is in [`docs/decisions/0020-phase-7a-narrative-engine-plan.md`](../decisions/0020-phase-7a-narrative-engine-plan.md)
> §"Phase 7A.2 — Interpretation Ledger"** including the full JSON
> schema, CLI verbs, architectural questions with PM answers, and
> success criteria. Read it before starting.

---

## The one paragraph you must internalize

Every `mc model narrate` call now writes a structured JSONL entry
per narrative. The entry includes: the rendered text, the template
ID + version, the severity, ALL evidence values that went into the
template, any benchmarks referenced, and metadata (model path,
model hash, period, scope, timestamp). The ledger is append-only,
per-workspace (`.mosaic/analysis-ledger.jsonl`), and uses the same
atomic write pattern as `mc-tessera`'s watermark files (write to
`.tmp`, rename). New CLI verbs let agents and humans query the
ledger: `mc model query-ledger --severity warning --since 2026-01`.
The ledger entry schema is the SAME structured output that
`mc model narrate --format json` already returns — extended with
persistence metadata. No new data shape; just durability.

---

## What gets built (4 sessions estimated)

### Session 1 (~3-4h): Ledger schema + write path

**Goal:** `mc model narrate --save-ledger` writes entries to
`.mosaic/analysis-ledger.jsonl`.

**Deliverables:**

1. **Ledger entry schema** in `mc-narrative/src/ledger.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub schema_version: String,        // "1.0"
    pub ledger_entry_id: String,       // uuid-v4
    pub generated_at: String,          // ISO-8601 UTC
    pub model: String,                 // model YAML path
    pub model_hash: String,            // sha256 of model file contents
    pub report_period: Option<String>, // "2026-04" if --period specified
    pub scope: BTreeMap<String, String>, // { advertiser, market, channel }
    pub narrative: NarrativeRecord,
    pub evidence: BTreeMap<String, serde_json::Value>,
    pub benchmarks_referenced: Vec<BenchmarkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeRecord {
    pub id: String,                    // template ID
    pub section: Option<String>,
    pub severity: String,              // "info" | "warning" | "critical"
    pub text: String,                  // rendered narrative text
    pub template_id: String,
    pub notability_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRef {
    pub id: String,
    pub value: f64,
    pub comparison: String,            // "above" | "below" | "at"
}
```

2. **Write path** — `pub fn write_ledger_entry(dir: &Path, entry: &LedgerEntry) -> Result<(), LedgerError>`:
   - Creates `.mosaic/` directory if absent
   - Writes to `.mosaic/analysis-ledger.jsonl.tmp`
   - Appends one JSON line per entry
   - Renames `.tmp` to target (atomic on POSIX — same pattern as `mc-tessera/src/incremental.rs:save_state`)
   - File locking via `fs2::FileExt::lock_exclusive()` (or hand-rolled flock) for concurrent safety

3. **Integration with `mc model narrate`** — new `--save-ledger` flag:
   - When set: after evaluating templates, convert each `NarrativeOutput` to a `LedgerEntry` (add metadata: model path, model hash, timestamp, scope)
   - Write all entries in one atomic append
   - Print "[ledger] Wrote N entries to .mosaic/analysis-ledger.jsonl" to terminal

4. **Model hash computation** — `sha256` of the model YAML file contents. Use `sha2` crate (add to mc-narrative deps). The hash lets the query layer know "this entry was generated against THIS version of the model."

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Ledger file location? | `.mosaic/analysis-ledger.jsonl` relative to the model file's parent directory. If the model is at `/foo/bar/model.yaml`, the ledger is at `/foo/bar/.mosaic/analysis-ledger.jsonl`. |
| One file per period or one file total? | **One file total.** Append-only JSONL. Simpler; `grep` and `jq` work out of the box. Per-period files are a future split if the file gets too large. |
| UUID generation without a dep? | **Use `sha2` (for model hash) + timestamp + random bytes.** Or add `uuid` crate if the workspace doesn't already have one. Check `Cargo.lock` first. |
| What if `--save-ledger` is not set? | **Don't write.** Narrate still works; output goes to stdout/json as before. Ledger is opt-in in v1. |
| Default `--save-ledger` in future? | **Not now.** Opt-in for 7A.2. Phase 7A.3 may flip the default when cross-period analysis needs the ledger populated. Document the intent. |
| Scope field — where does it come from? | **From the cube's metadata** (advertiser, market, channel — whatever the cube's dimensions are named). For the demo server's cubes, this comes from the CSV filename + registry match. For `mc model narrate`, it comes from the model's dimension names. |

**Regression tests (5 minimum):**
1. `test_write_ledger_entry_creates_file`
2. `test_ledger_entry_is_valid_jsonl` (each line parses independently)
3. `test_multiple_entries_append_atomically`
4. `test_ledger_model_hash_changes_when_model_changes`
5. `test_narrate_with_save_ledger_flag_writes_entries`

---

### Session 2 (~3-4h): Read path + `mc model query-ledger`

**Goal:** `mc model query-ledger` reads and filters the ledger.

**Deliverables:**

1. **Read path** — `pub fn read_ledger(path: &Path) -> Result<Vec<LedgerEntry>, LedgerError>`:
   - Reads `.mosaic/analysis-ledger.jsonl` line by line
   - Parses each line as a `LedgerEntry`
   - Skips malformed lines with a warning (don't crash on one bad entry)
   - Returns entries in chronological order (file order = append order)

2. **Query filters** — applied after reading:
   - `--severity <info|warning|critical>` — filter by severity
   - `--template <id>` — filter by template_id
   - `--since <period>` — filter by `report_period >= <period>`
   - `--scope <key=value>` — filter by scope field (e.g., `--scope channel=Targeted_Display`)
   - `--repeated <n>` — return only entries where the same template_id fired in N+ consecutive periods (the "third consecutive month" query)
   - Filters AND-combine (all must match)

3. **New CLI verb** — `mc model query-ledger`:
   ```
   mc model query-ledger <model-dir> [--severity <s>] [--template <id>]
       [--since <period>] [--scope <k=v>] [--repeated <n>]
       [--format json|text]
   ```
   - `<model-dir>` is the directory containing `.mosaic/analysis-ledger.jsonl`
   - JSON output: `{ "schema_version": "1.0", "entries": [...], "count": N }`
   - Text output: per-entry severity badge + text + period

4. **`--repeated` implementation** — the most interesting filter:
   - Group entries by (template_id, scope)
   - For each group: find runs of consecutive `report_period` values
   - Return entries where the run length >= N
   - "Consecutive" means adjacent periods in the natural order (Jan, Feb, Mar — not Jan, Mar which is a gap)

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Period ordering for `--repeated`? | **Lexicographic sort on `report_period` string.** `"2026-01" < "2026-02" < "2026-12"`. Works for YYYY-MM format. If users have quarterly periods ("2026-Q1"), they sort correctly too. |
| What if the ledger has 100K entries? | **Read all into memory for v1.** The JSONL file for a single workspace won't exceed a few MB even after years. If it does, Phase 7A.2.1 adds SQLite. Don't pre-optimize. |
| Should `query-ledger` also query across workspaces? | **No in v1.** Per-workspace only. Cross-workspace queries are Phase 7A.4 (benchmark aggregation with privacy boundary). |
| MCP tool? | **Yes** — `mosaic.ledger.query` paralleling `mosaic.model.query`. Same filter syntax. |

**Regression tests (6 minimum):**
1. `test_read_ledger_parses_all_entries`
2. `test_query_filter_severity`
3. `test_query_filter_since`
4. `test_query_filter_template_id`
5. `test_query_repeated_finds_consecutive_runs`
6. `test_query_repeated_ignores_gaps_in_periods`

---

### Session 3 (~2-3h): `mc model ledger-export` + demo integration

**Goal:** Export the ledger in multiple formats. Wire the demo
server to persist narratives automatically.

**Deliverables:**

1. **New CLI verb** — `mc model ledger-export`:
   ```
   mc model ledger-export <model-dir> [--format jsonl|csv] [--since <period>]
   ```
   - JSONL: dump the raw ledger (with optional date filter)
   - CSV: flatten entries into a table (one row per narrative, columns for severity, template_id, text, period, evidence.* fields)

2. **Demo server integration** — `mc-demo-server` writes ledger entries automatically on every upload:
   - The demo server's workspace directory already exists (from Phase 6D Session 4)
   - After narrative evaluation, convert `NarrativeOutput` → `LedgerEntry` and write
   - Terminal prints "[ledger] Wrote 17 entries to .mosaic/analysis-ledger.jsonl"
   - Subsequent uploads append to the same ledger (building history)

3. **Frontend display** (optional stretch) — if time permits, add a
   "Ledger History" tab to the demo UI that shows past narrate runs
   with timestamps. Pull from `GET /api/ledger?since=2026-01`.

**Regression tests (3 minimum):**
1. `test_export_csv_has_correct_columns`
2. `test_export_jsonl_matches_raw_ledger`
3. `test_demo_server_writes_ledger_on_upload`

---

### Session 4 (~2-3h): Privacy boundary + polish + docs

**Goal:** Ship-ready with privacy documentation and clean edges.

**Deliverables:**

1. **Privacy boundary** (per planning doc Q8: opt-in only):
   - Ledger entries do NOT contain raw external IDs by default
   - `scope` fields use the cube's dimension element names (which are user-configured and may or may not contain PII)
   - Document: "Ledger entries contain whatever dimension names and measure values your model declares. If your model uses real advertiser names as dimension elements, those appear in the ledger. Configure dimension elements accordingly if ledger privacy is a concern."
   - NO automatic PII detection in v1 (that's Phase 7A.4 scope)

2. **Ledger versioning** — the `schema_version: "1.0"` on every entry is the forward-compat mechanism. Future readers check the version and handle old entries gracefully. Document the version contract.

3. **MC7020-MC7025 diagnostic codes** (per planning doc):
   - MC7020: Ledger write failed (disk full, permission denied)
   - MC7021: Ledger schema version mismatch (entry from future schema)
   - MC7022: Ledger query with invalid filter
   - MC7023: Ledger query result too large (>10K entries; suggest using `--since` to narrow)
   - MC7024: Reserved for PII detection (Phase 7A.4)
   - MC7025: Ledger entry references unknown template_id

4. **Documentation** — update `demo/README.md` with ledger usage:
   ```bash
   # Generate narratives + save to ledger
   mc model narrate model.yaml --save-ledger

   # Query the ledger
   mc model query-ledger . --severity warning
   mc model query-ledger . --repeated 3 --since 2026-01

   # Export for analysis
   mc model ledger-export . --format csv > analysis.csv
   ```

**Pre-flight code sweep:**
```bash
for code in MC7020 MC7021 MC7022 MC7023 MC7024 MC7025; do
  grep -rn "$code" crates/ | wc -l
done
```
All should be 0 before implementation.

---

## Hard Rules (binding)

1. **`mc-core`, `mc-model`, `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera` all locked.** Ledger lives in `mc-narrative` (the persistence layer) + `mc-cli` (verbs).
2. **`mc-narrative` gains ledger write + read capabilities** — new `src/ledger.rs` module. The existing `evaluate_all` function stays unchanged; ledger write is a SEPARATE step called by the CLI verb (not embedded in evaluation).
3. **The ledger entry schema is the SAME structured output from `mc model narrate --format json`, extended with persistence metadata.** Don't invent a new schema; extend the existing one.
4. **JSONL format for v1.** One JSON object per line, append-only, atomic writes. SQLite is Phase 7A.2.1 if queries get slow.
5. **Per-workspace isolation.** Ledger file at `.mosaic/analysis-ledger.jsonl` relative to the model directory. No cross-workspace queries in v1.
6. **Immutable entries (planning doc Q7).** Once written, entries are never modified. The query layer handles schema version differences.
7. **`--save-ledger` is opt-in in v1.** Default: narrate produces output but doesn't persist. Future phases may flip the default.
8. **Per-session commits (Rule 11).** 4 commits minimum.

---

## Acceptance Gates (lean)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (955 → expect ~+15 = ~970).
- [ ] `mc model narrate --save-ledger` writes entries to `.mosaic/analysis-ledger.jsonl`.
- [ ] `mc model query-ledger . --severity warning` returns only warning-severity entries.
- [ ] `mc model query-ledger . --repeated 3` returns entries that fired in 3+ consecutive periods (test with synthetic ledger data if sample data only has 2 periods).
- [ ] `mc model ledger-export . --format csv` produces a valid CSV.
- [ ] Ledger entries are valid JSONL (each line parses independently).
- [ ] Atomic writes (no partial entries on crash — test by writing during concurrent reads).
- [ ] MC7020-MC7025 codes swept FREE + shipped.
- [ ] Locked surfaces: zero diff.

---

## SPEC QUESTION candidates

- Session 1: How to generate UUIDs without pulling in the `uuid` crate. Options: `sha2(timestamp + model_hash + entry_index)` is deterministic and reproducible (good for testing); `uuid` crate gives proper v4 random UUIDs (standard but adds a dep). PM preference: **deterministic hash-based IDs** unless the implementer finds a compelling reason for random UUIDs.
- Session 2: `--repeated` with quarterly periods ("2026-Q1", "2026-Q2") — should "consecutive" require adjacent quarters, or just sequential? (Answer: sequential — Q1 followed by Q2 is consecutive even though there's a 3-month gap between them. The period string sort handles this.)
- Session 3: Should the demo server write ledger entries by default, or require a config flag? (Answer: by default — the demo is the primary use case for showing the ledger in action.)

---

## Completion Report Expectations

Per process-notes Rule 10:
- SHIPPED section per session
- Per-verb smoke check outputs (actual `mc model narrate --save-ledger` + `query-ledger` + `ledger-export` commands + outputs)
- MC7020-MC7025 codes documented
- Ledger entry schema documented (paste one real entry from the sample data)
- Privacy boundary documented
- Known debt + per-session commit log + locked surfaces grep

---

*End of handoff. Phase 7A.2 makes every narrative durable. After this
ships, the data exists for Phase 7A.3 (cross-period trend detection)
and Phase 7A.4 (benchmark aggregation). The ledger is the foundation
that turns reporting from a one-shot event into a knowledge accumulator.*
