# Phase 6A Handoff — Agent-Ready CLI

> **Audience:** the Claude Code instance that implements Phase 6A.
> **You inherit main at the latest commit, 694/0 tests.**
>
> **This phase makes Mosaic usable by ANY agent — KellyBets, Claude
> Code, a cron job, a CI pipeline, a Jupyter notebook — without a UI.**
> Every operation a user or agent needs to perform against a Mosaic
> model (query cells, test hypotheses, trace computations, export
> data) becomes a CLI verb with structured JSON output. The UI (Phase
> 6B) later renders the same data visually; Phase 6A is the
> capability layer.
>
> **Hard rule:** Phase 6A touches `crates/mc-cli/` ONLY. It does NOT
> modify `mc-core`, `mc-model`, `mc-recipe`, `mc-drivers`, `mc-tessera`,
> `mc-fixtures`, or `mosaic-plugin/`. It reads from compiled cubes
> using existing public APIs. All new verbs are orchestration over
> existing infrastructure — no kernel changes.
>
> **Every new verb MUST support `--format json|csv|text`.** JSON is
> the canonical machine-readable output. CSV is for spreadsheet/pandas
> import. Text is for humans in the terminal. Agents consume JSON.
>
> **Strategic centerpiece:** every place where an agent currently has
> to drop down to Python is a place where Mosaic's CLI surface is
> incomplete. The NBA cartridge instance burned 200+ lines of Python
> tokens writing a manual CSV flattener. The Tide Cleaners instance
> burned 250 lines for a forecast script. Those tokens were spent
> once and benefited nobody else. CLI verbs are spent once (on design)
> and benefit every future agent forever. This is the same pattern as
> Phase 4's plugin-as-source-of-truth: capture knowledge in tooling,
> not in agent transcripts.

---

## Agent-Readiness Invariants (binding across ALL verbs)

These four rules apply to every verb in Phase 6A. They matter more
than any specific verb — 8 verbs that don't follow them are worse
than 4 that do.

1. **Stable JSON output with schema version.** All `--format json`
   output follows Phase 3B's diagnostic-envelope discipline. Every
   JSON response has a `schema_version` field. Schema does not break
   between releases. Agents can parse the output reliably.

2. **Stable exit codes.** `0` = success. `1` = model/recipe error
   (diagnostics in output). `2` = CLI usage error (bad flags/args).
   `3` = I/O error (file not found, network failure). Agents check
   exit codes; humans read messages. Both work.

3. **Idempotent where possible.** Re-running `mc tessera apply` with
   the same import_id skips or warns (doesn't double-apply). `query`,
   `trace`, `export`, `transform` are naturally idempotent (read-only).
   `whatif` auto-rollbacks (no persistent side effects). `write` is
   the exception (intentionally stateful).

4. **`--dry-run` on all state-changing verbs.** Agents verify before
   committing. `mc tessera apply --dry-run` already exists. Extend to:
   `mc model write --dry-run` (shows what would change without changing
   it), `mc model sweep --dry-run` (shows the parameter grid without
   running evaluations).

---

## The one paragraph you must internalize

An agent's primary operation is **"ask the cube a question and get
structured data back."** Today, Mosaic can build cubes, validate them,
lint them, test them with goldens, and ingest data — but it CANNOT
answer arbitrary queries against the computed state. An agent can't
ask "which games have positive EV tonight?" or "what happens if the
line moves to 223?" or "why does the model predict 228?" Phase 6A
closes this gap. After 6A ships, the CLI IS the complete interface
— the UI adds chrome, not capability.

---

## The 6 new CLI verbs

### 1. `mc model query` — Read cells by coordinate filter (PRIORITY 0)

**The most important verb.** Everything else builds on this.

```bash
# Read one specific cell:
mc model query model.yaml \
  --coord "Scenario=Actual,Version=Working,Time=2025_04_15,Game=LAL_at_BOS,Measure=Predicted_Total"

# Filter: all cells matching a condition
mc model query model.yaml \
  --where "EV_Per_Dollar > 0.03 and Time == '2025_04_15'" \
  --show "Game,Sportsbook,Predicted_Total,Calibrated_P,EV_Per_Dollar,Kelly_Fraction"

# Slice: fix some dims, enumerate others
mc model query model.yaml \
  --where "Game == 'LAL_at_BOS'" \
  --show "Sportsbook,Market_Line,P_Over,EV_Per_Dollar,Should_Bet"

# Aggregate: compute summary statistics
mc model query model.yaml \
  --aggregate "mean(Abs_Error), mean(Direction_Correct), sum(Profit_Units), count(Should_Bet == 1)"

# All with --format json|csv|text (default: text for terminal)
mc model query model.yaml --where "..." --show "..." --format json
```

**Implementation:**

1. Load the model (parse → validate → compile → apply canonical_inputs)
2. Parse the `--where` expression as a filter predicate (same formula grammar — reuse the existing parser!)
3. Parse the `--show` list as measure names to read
4. Enumerate all leaf coordinates in the cube
5. For each coordinate: evaluate the `--where` predicate; if truthy, read the `--show` measures
6. Format output per `--format` flag

**The `--where` filter reuses the formula parser.** The expression `EV_Per_Dollar > 0.03 and Time == '2025_04_15'` is syntactically valid in the Phase 3E formula grammar (comparisons + logical operators). The evaluator already handles this. You're just running a formula eval per-coordinate and filtering.

**The `--show` list:** measure names to include in the output. If omitted, show all non-Null measures at matching coordinates.

**The `--aggregate` flag:** instead of per-coordinate rows, compute aggregate functions across all matching coordinates. Functions: `mean(measure)`, `sum(measure)`, `count(predicate)`, `min(measure)`, `max(measure)`. These are NOT new formula functions — they're CLI-level post-processing over the query result set.

**`--coord` flag:** shorthand for a single exact-coordinate read. Returns one cell value. Faster than `--where` for single lookups.

**Dimension values in `--where`:** when the filter references a dimension name (like `Time == '2025_04_15'` or `Game == 'LAL_at_BOS'`), it compares against the current coordinate's element name in that dimension. This is the same pattern as `lookup("table", DimName)` — dimension names resolve to the current element.

**JSON output shape:**

```json
{
  "query": "EV_Per_Dollar > 0.03 and Time == '2025_04_15'",
  "results": [
    {
      "coord": {"Scenario":"Actual","Version":"Working","Time":"2025_04_15","Sportsbook":"Pinnacle","Game":"LAL_at_BOS"},
      "values": {"Predicted_Total":228.13,"Calibrated_P":0.65,"EV_Per_Dollar":0.08,"Kelly_Fraction":0.02}
    },
    ...
  ],
  "count": 12,
  "aggregates": null
}
```

**Aggregate JSON output shape:**

```json
{
  "query": null,
  "results": null,
  "count": 45,
  "aggregates": {
    "mean(Abs_Error)": 13.4,
    "mean(Direction_Correct)": 0.62,
    "sum(Profit_Units)": 3.7,
    "count(Should_Bet == 1)": 8
  }
}
```

---

### 2. `mc model whatif` — Override one input, report deltas (PRIORITY 0)

```bash
mc model whatif model.yaml \
  --set "Scenario=Actual,Version=Working,Time=2025_04_15,Sportsbook=Pinnacle,Game=LAL_at_BOS,Measure=Market_Line" \
  --value 223.0 \
  --show "P_Over,EV_Per_Dollar,Kelly_Fraction,Should_Bet" \
  --format json
```

**Implementation:**

1. Load the model + apply canonical_inputs (same as `mc model test`)
2. Read the `--show` measures at the affected coordinates → "before" values
3. Override the specified cell with `--value` (via `cube.write()`)
4. Re-read the `--show` measures → "after" values
5. Compute deltas → output

**JSON output:**

```json
{
  "cell_overridden": {
    "coord": "Scenario=Actual,...,Measure=Market_Line",
    "before": 221.5,
    "after": 223.0
  },
  "affected_measures": [
    {"measure":"P_Over","before":0.65,"after":0.58,"delta":-0.07},
    {"measure":"EV_Per_Dollar","before":0.08,"after":0.03,"delta":-0.05},
    {"measure":"Kelly_Fraction","before":0.02,"after":0.008,"delta":-0.012},
    {"measure":"Should_Bet","before":1.0,"after":1.0,"delta":0.0}
  ]
}
```

**Atomicity invariant (binding):** whatif does NOT persist the change.
The lifecycle is: load model → snapshot → override → compute → report
deltas → rollback (implicit on process exit). The source CSV is never
modified. The cube state after the process exits is identical to before
the process started. This is a read-only operation that happens to
involve a temporary write internally. Agents can safely call whatif
in a loop without accumulating side effects.

---

### 3. `mc model query --output <file>` replaces a separate export verb

Per Desktop review: `export` is just "query with file output." Don't
ship a separate verb — add `--output <path>` to `mc model query`:

```bash
# Query to stdout (terminal / pipe):
mc model query model.yaml --where "Should_Bet == 1" --format json

# Query to file (the "export" use case):
mc model query model.yaml \
  --where "Should_Bet == 1" \
  --show "Game,Sportsbook,Calibrated_P,EV_Per_Dollar" \
  --output bets.json \
  --format json

# Query everything to CSV (bulk export):
mc model query model.yaml --output results.csv --format csv
```

**No separate `mc model export` verb.** One verb, one mental model.
Two verbs that nearly overlap is a UX problem for agents parsing help
text. `--output` is optional; omitting it prints to stdout.

---

### 4. `mc model trace` — Show computation chain (PRIORITY 1)

```bash
mc model trace model.yaml \
  --coord "Scenario=Actual,Version=Working,Time=2025_04_15,Sportsbook=Pinnacle,Game=LAL_at_BOS,Measure=EV_Per_Dollar" \
  --format json
```

**Implementation:**

1. Load the model + apply inputs
2. Find the rule that targets the `--coord`'s Measure
3. Walk the rule's AST: for each Ref node, recursively trace
4. For leaf Inputs, read the value
5. Build a tree of (measure → value → children)

**JSON output:**

```json
{
  "measure": "EV_Per_Dollar",
  "value": 0.08,
  "rule": "rule_ev",
  "formula": "Calibrated_P * (Decimal_Odds - 1) - (1 - Calibrated_P)",
  "inputs": {
    "Calibrated_P": {
      "value": 0.65,
      "rule": "rule_calibrated_p",
      "formula": "calibrate(P_Over, 'v16_calibration')",
      "inputs": {
        "P_Over": {
          "value": 0.68,
          "rule": "rule_p_over",
          "formula": "1 - norm_cdf(Market_Line, Predicted_Total, 17.251)",
          "inputs": {
            "Market_Line": {"value": 221.5, "source": "input"},
            "Predicted_Total": {
              "value": 228.13,
              "rule": "rule_predicted_total",
              "formula": "predict('nba_v16_lasso', avg_pace, ...)",
              "inputs": {
                "avg_pace": {"value": 100.8, "source": "input"},
                "combined_off_rating": {"value": 226.4, "source": "input"}
              }
            }
          }
        }
      }
    },
    "Decimal_Odds": {"value": 1.91, "source": "input"}
  }
}
```

**This is the "explainability" feature.** An agent can answer "why does the model recommend this bet?" by tracing the computation tree all the way to input values.

**Output MUST be hierarchical (tree-structured), not flat.** A flat list of "Revenue = 1000, Customers = 50" is less useful than a tree showing dependency relationships. The JSON nests naturally (see above). For `--format text`, use indentation:

```
EV_Per_Dollar = 0.08
├── Calibrated_P = 0.65
│   └── P_Over = 0.68
│       ├── Market_Line = 221.5 (input)
│       └── Predicted_Total = 228.13
│           ├── avg_pace = 100.8 (input)
│           ├── combined_off_rating = 226.4 (input)
│           └── ... (9 features)
└── Decimal_Odds = 1.91 (input)
```

Hierarchical trace is what makes this verb genuinely useful for debugging — it shows causality, not just values.

---

### 5. `mc model sweep` — Parameter sensitivity analysis (PRIORITY 1)

Per the Model-as-Judge research note (`docs/research-notes/model-as-judge-architecture.md`). Already fully specified there. Key points:

```bash
mc model sweep model.yaml \
  --model nba_v16_lasso \
  --coefficient avg_pace \
  --range "0:5:0.5" \
  --metric "mean(Abs_Error)" \
  --goal minimize \
  --format json
```

**Implementation:** loop `whatif` over parameter values, record metric at each point. Named selectors (not array indices). In-memory struct override (no YAML patching). JSON canonical output. Baseline comparison by default.

---

### 6. `mc model write` — Set one cell without editing CSV (PRIORITY 2)

```bash
mc model write model.yaml \
  --coord "Scenario=Actual,Version=Working,Time=2025_04_15,Sportsbook=Pinnacle,Game=LAL_at_BOS,Measure=Market_Line" \
  --value 223.0
```

**Implementation:**

1. Load model + apply inputs
2. Write the cell via `cube.write()`
3. Persist to the `.tessera/` sidecar as a one-cell import
4. Print confirmation + affected derived measures

This is the "live update" verb — an agent receives a line movement alert and patches one cell without re-ingesting the full CSV.

**Persistence model (binding):** append-only log at `<model_dir>/.tessera/writes.jsonl`. One entry per write:

```json
{"timestamp":"2025-04-15T22:30:00Z","coord":"Scenario=Actual,...,Measure=Market_Line","value":223.0,"source":"mc model write","agent":"KellyBets"}
```

On next `mc model test` or `mc model query`, the write log is replayed
on top of canonical_inputs (writes override input values at matching
coordinates). This pairs with the existing `.tessera/audit.jsonl`
pattern from Phase 5A. `mc model write --dry-run` shows what would be
logged without logging it.

---

### 7. `mc model diff` — Compare two cube states (PRIORITY 2)

```bash
# Compare current state to a previous import:
mc model diff model.yaml --since last --format json

# Compare two scenarios:
mc model diff model.yaml --left "Scenario=Actual" --right "Scenario=Forecast" --format json

# Compare before/after a what-if (pipe from whatif output):
mc model diff model.yaml --before snapshot-id-1 --after snapshot-id-2
```

**Implementation:**

1. Load the model (compile + apply inputs)
2. Determine the "left" and "right" states to compare:
   - `--since last`: compare current state to the state before the most recent `mc tessera apply` or `mc model write`
   - `--left`/`--right`: compare two coordinate-filtered slices (e.g., two scenarios)
   - `--before`/`--after`: compare two named snapshots (from Tessera's `.tessera/snapshots/`)
3. Enumerate all leaf coordinates; for each, read both states
4. Report cells where values differ: coord, left_value, right_value, delta
5. Sort by `abs(delta)` descending — top changes first

**JSON output:**

```json
{
  "comparison": "current vs last_import",
  "changed_cells": 47,
  "top_changes": [
    {"coord":"...Game=LAL_at_BOS,Measure=Market_Line","left":221.5,"right":222.0,"delta":0.5},
    {"coord":"...Game=LAL_at_BOS,Measure=EV_Per_Dollar","left":0.08,"right":0.06,"delta":-0.02}
  ],
  "summary": {
    "cells_increased": 23,
    "cells_decreased": 24,
    "max_abs_delta": 3.5,
    "measures_affected": ["Market_Line","P_Over","EV_Per_Dollar","Kelly_Fraction"]
  }
}
```

**Agent use case:** "What changed since the last data load?" After a
`mc tessera apply`, the agent runs `mc model diff --since last` to
see which cells moved and by how much. This is how KellyBets detects
line movements — diff shows "Pinnacle LAL_at_BOS line moved from
221.5 to 222.0; EV dropped from 0.08 to 0.06."

---

### 8. `mc tessera transform` — Convert raw data to model-compatible format (PRIORITY 1)

The "I have a raw CSV/endpoint export and need to make it cube-ready" verb.

```bash
# From a local file:
mc tessera transform \
  --source ~/Downloads/hubspot_export.csv \
  --recipe recipes/hubspot-import.recipe.yaml \
  --output data/hubspot-ready.csv

# From a URL (simple HTTP GET — no auth, no pagination):
mc tessera transform \
  --source "https://api.balldontlie.io/v1/games?dates[]=2025-04-15" \
  --recipe recipes/nba-stats-import.recipe.yaml \
  --output data/game-stats.csv \
  --format csv

# With API key in query string (simple auth — no OAuth, no headers):
mc tessera transform \
  --source "https://api.the-odds-api.com/v4/sports/basketball_nba/odds/?apiKey=${ODDS_API_KEY}&markets=totals" \
  --recipe recipes/odds-api-import.recipe.yaml \
  --output data/tonights-odds.csv

# Preview without writing (dry-run: show first 10 rows):
mc tessera transform \
  --source raw.csv \
  --recipe recipe.yaml \
  --preview 10
```

**Implementation:**

1. **Source resolution:**
   - If `--source` starts with `http://` or `https://` → simple HTTP GET via `ureq` (already in workspace from mc-drivers) into a temp file, then process
   - **URL fetch scope for Phase 6A: simple GET only.** No OAuth, no bearer tokens in headers, no POST bodies, no pagination, no retry/backoff, no rate limiting. API keys can be passed in query strings (environment variable expansion: `${ODDS_API_KEY}`). Complex API ingestion (auth headers, pagination, retries) goes through `mc tessera apply` with the full `http_json` driver + recipe. The transform verb is the simple case; the recipe is the complex case.
   - If `--source` ends with `.json` or response Content-Type is JSON → parse JSON, flatten using `json_path` from the recipe's source config
   - If `--source` is a local file → read directly
   - Excel support (`.xlsx`) → deferred until xlsx driver ships; for now, convert to CSV externally

2. **Recipe-driven transform:**
   - Load the recipe YAML (same as `mc tessera apply` does)
   - Apply column mappings: `source: "campaign_name" → dimension: "Channel"`
   - Apply type coercion: `scale: 0.01` (cents→dollars), `time_format: "MM/DD/YYYY"`, `time_timezone: "America/New_York"`
   - Apply defaults: `defaults: { scenario: "Actual", version: "Working" }`
   - Handle `format: long` vs `format: wide` per ADR-0010 Amendment 2

3. **Output:**
   - Writes a clean long-format CSV matching the model's `canonical_inputs` shape: `Dim1,Dim2,...,DimN,Measure,value`
   - Or `--format json` for a JSON array of records
   - The output file is ready to be used as `canonical_inputs.source:` in the model YAML or ingested via `mc tessera apply`

**Why this matters (the token-burning problem):**

Without `transform`, an agent (or human) that has API access to live data must:
1. Write a custom Python script to fetch + flatten + format
2. Run the script
3. Hope the format matches what the model expects
4. Debug mismatches manually

With `transform`, the recipe IS the transformation spec. The agent writes the recipe once (or uses `mc tessera propose` to generate it), then `transform` handles fetch + flatten + format forever. No Python. No burned tokens writing bespoke flatteners.

**The sports-betting case that motivated this:**

```bash
# Tonight's NBA odds — one command, direct from API to model-ready CSV:
mc tessera transform \
  --source "https://api.the-odds-api.com/v4/sports/basketball_nba/odds/?apiKey=$ODDS_API_KEY&markets=totals&oddsFormat=decimal" \
  --recipe recipes/odds-api-nba-totals.recipe.yaml \
  --output data/tonights-odds.csv

# The recipe maps the JSON structure to model dimensions:
# { "bookmakers": [{ "key": "pinnacle", "markets": [{ "outcomes": [{ "point": 221.5 }] }] }] }
# → Scenario=Actual, Version=Working, Time=today, Sportsbook=pinnacle, Game=LAL_at_BOS, Measure=Market_Line, value=221.5
```

This replaces the 200+ lines of Python that the NBA cartridge instance wrote manually to generate sample data. One recipe + one command = fresh data from any API.

**Recipe additions for URL sources:**

```yaml
source:
  driver: http_json
  url: "https://api.the-odds-api.com/v4/sports/basketball_nba/odds/"
  headers:
    Authorization: "Bearer ${env.ODDS_API_KEY}"
  json_path: "$.data[*]"      # JSONPath to the array of records
  # Response flattening:
  flatten:
    - path: "$.bookmakers[*]"
      as: "book_row"
      extract:
        sportsbook: "$.key"
        line: "$.markets[0].outcomes[0].point"
        odds: "$.markets[0].outcomes[0].price"
```

The `flatten:` block handles nested JSON → flat rows. This is the piece that eliminates custom Python for API responses. The recipe declares HOW to flatten; `transform` executes it.

**Note:** the `flatten:` recipe extension is NEW — it doesn't exist in the current `mc-recipe` schema. If implementing this would require mc-recipe changes (which are locked in Phase 6A), then:
- Option A: implement the JSON flattening in mc-cli directly (the transform verb handles it without touching mc-recipe's schema)
- Option B: defer the `flatten:` schema field to a mc-recipe amendment and implement basic URL→CSV in 6A (fetch the JSON, user provides a pre-flattened recipe)

**Recommend Option A** — the flattening logic lives in the transform verb's source-fetching code, not in the recipe schema. The recipe's `json_path` field (already in the schema from Phase 5A's HttpJsonDriver) handles the top-level array selection; the per-field extraction is the transform verb's job.

---

## MCP exposure

**Every verb above must also be exposed as an MCP tool** via the existing `mc mcp` server. The server already handles `tools/list` and `tools/call` for validate/lint/test/inspect/demo. Add:

```json
{"name": "mosaic.model.query", "inputSchema": {"path":"string","where":"string","show":"string[]","aggregate":"string[]","format":"string"}}
{"name": "mosaic.model.whatif", "inputSchema": {"path":"string","set_coord":"string","value":"number","show":"string[]"}}
{"name": "mosaic.model.export", "inputSchema": {"path":"string","where":"string","show":"string[]","format":"string","output":"string"}}
{"name": "mosaic.model.trace", "inputSchema": {"path":"string","coord":"string"}}
{"name": "mosaic.model.sweep", "inputSchema": {"path":"string","model":"string","coefficient":"string","range":"string","metric":"string","goal":"string"}}
{"name": "mosaic.model.write", "inputSchema": {"path":"string","coord":"string","value":"number"}}
{"name": "mosaic.tessera.transform", "inputSchema": {"source":"string","recipe":"string","output":"string","format":"string","preview":"number"}}
```

This means Claude Code (or any MCP client) can call these as native tool invocations — no subprocess needed.

---

## Acceptance gates

1. `mc model query nba-totals.yaml --where "Should_Bet == 1" --format json` returns valid JSON with correct bet recommendations
2. `mc model query nba-totals.yaml --aggregate "mean(Abs_Error)"` returns the model's average error
3. `mc model whatif nba-totals.yaml --set "...Market_Line" --value 223 --show "EV_Per_Dollar" --format json` shows before/after/delta
4. `mc model export nba-totals.yaml --where "EV_Per_Dollar > 0" --format csv --output /tmp/bets.csv` writes a valid CSV
5. `mc model trace nba-totals.yaml --coord "...Measure=EV_Per_Dollar" --format json` returns the full computation tree
6. `mc model sweep nba-totals.yaml --model nba_v16_lasso --coefficient avg_pace --range "0:5:1" --metric "mean(Abs_Error)" --goal minimize` returns a sweep curve with baseline comparison
7. `mc tessera transform --source "https://httpbin.org/json" --recipe test-recipe.yaml --output /tmp/test.csv` fetches URL + transforms to CSV
8. `mc tessera transform --source local.csv --recipe recipe.yaml --preview 5` shows first 5 transformed rows without writing
9. All 7 verbs support `--format json` and produce valid JSON parseable by `jq`
10. All 7 verbs are exposed via `mc mcp` (tools/list shows them; tools/call works)
11. All existing 694 tests still pass
12. `mc model validate/lint/test/inspect/demo` and `mc tessera apply/dry-run/history/rollback/audit` behavior unchanged (no regressions)

---

## Implementation order (dependency-correct)

1. **`mc model query`** (P0) — foundational; whatif/diff/sweep all reuse query infrastructure. Includes `--output <file>` (replaces separate export verb).
2. **`mc tessera transform`** (P1) — highest token-saving leverage; independent of query; fetch + flatten + output
3. **`mc model whatif`** (P1) — depends on query for delta output; snapshot/rollback lifecycle
4. **`mc model trace`** (P1) — depends on query infrastructure; AST walking for hierarchical output
5. **`mc model diff`** (P1) — depends on query; compare two states
6. **`mc model sweep`** (P1) — loop over whatif with metric collection
7. **`mc model write`** (P2) — after persistence-model decision; append-only log
8. **MCP exposure** — add all 7 verbs as tools in the existing `mc mcp` server (can be done incrementally as each verb ships)

---

## Hard rules

- **mc-core, mc-model, mc-recipe, mc-drivers, mc-tessera, mc-fixtures: ALL LOCKED.** Zero-line diff. Phase 6A is purely `mc-cli` additions.
- **Reuse the existing formula parser for `--where` expressions.** Do NOT write a separate expression parser. The formula grammar already handles `EV > 0.03 and Game == 'LAL_at_BOS'`.
- **JSON output must be valid and parseable by `jq`.** Test every verb with `| jq .` to verify.
- **Every verb loads the model fresh each invocation** (same as current `mc model test`). No daemon mode. No persistent state across invocations (that's Phase 6B territory).
- **`--format json` is the default for programmatic use.** But `--format text` should be the default when stdout is a TTY (detect via `isatty()`). This way agents always get JSON; humans always get readable text.
- **No new dependencies in mc-cli** beyond what's already there.
- **Performance target:** `mc model query` on the Acme model (2,520 input cells) should complete in < 100ms. On the NBA model (45 game-book combos) should be < 50ms.

---

## SPEC QUESTION triggers

1. **The `--where` expression needs dimension-name resolution** (comparing against element names like `Game == 'LAL_at_BOS'`). How to distinguish "Game" (a dimension name) from "Game" (a potential measure name)? Likely: check dimensions first, measures second. If ambiguous, surface as a diagnostic.

2. **`mc model write` persists via Tessera sidecar** — but what if the model doesn't have a `.tessera/` directory? Create it on first write? Require `mc tessera init` first?

3. **Sweep calls compile N times (one per sweep point).** For a sweep with 20 points on a model that takes 50ms to compile, that's 1 second. Acceptable? Or should sweep compile once and override in-memory (the research note's recommendation)?

4. **Trace depth limit.** A trace on a deep rule chain (7 levels per the NBA cartridge) produces a large JSON tree. Should there be a `--depth N` flag to limit? Default unlimited?

5. **Query result size limit.** A model with 100K coordinates and `--where` that matches all of them would produce 100K rows. Should there be a `--limit N` flag? Default 1000?

---

## Files to touch

```
crates/mc-cli/src/
├── main.rs              # Add new verb dispatch (ModelVerb::Query, Whatif, Trace, Sweep, Diff, Write)
├── query.rs             # NEW — the query engine (filter + show + aggregate + --output file)
├── whatif.rs            # NEW — snapshot + override + compute + report deltas + rollback
├── trace.rs             # NEW — hierarchical AST walking + tree building
├── sweep.rs             # NEW — loop over whatif with metric collection
├── diff.rs              # NEW — compare two cube states, report deltas
├── write.rs             # NEW — single-cell write + append-only log persist
├── transform.rs         # NEW — fetch URL/file + recipe-driven flatten + output CSV/JSON
├── mcp.rs              # MODIFY — add 7 new tools to tools/list + tools/call dispatch
└── tessera.rs          # MODIFY — add "transform" verb dispatch to tessera subcommand
```

---

## Completion report format

```
DONE: Phase 6A — Agent-Ready CLI

Build/Format/Lint/Tests: ✓ / ✓ / ✓ / [N]/0
New CLI verbs: query, whatif, export, trace, sweep, write, transform (7)
MCP tools added: mosaic.model.{query,whatif,export,trace,sweep,write} + mosaic.tessera.transform (7)
Formats supported: json, csv, text on all verbs
Performance: query on Acme < 100ms, query on NBA < 50ms
URL fetch: mc tessera transform --source "https://..." fetches + flattens + outputs ✓

Acceptance gates: 12/12
- query with --where filter ✓
- query with --aggregate ✓
- whatif before/after/delta ✓
- export to file ✓
- trace computation tree ✓
- sweep with baseline comparison ✓
- transform from URL to CSV ✓
- transform preview mode ✓
- all verbs --format json produces valid JSON ✓
- all verbs exposed via mc mcp ✓
- 694 existing tests pass ✓
- existing verbs unchanged ✓
```

Do NOT commit. Report DONE when all 10 gates pass.

---

## The KellyBets test (the real acceptance scenario)

After Phase 6A ships, this must work end-to-end:

```bash
# An agent (KellyBets, Claude Code, cron job) runs this nightly:

# 0. Fetch tonight's live odds from API → model-ready CSV (NO PYTHON!)
mc tessera transform \
  --source "https://api.the-odds-api.com/v4/sports/basketball_nba/odds/?apiKey=$ODDS_API_KEY&markets=totals&oddsFormat=decimal" \
  --recipe recipes/odds-api-import.recipe.yaml \
  --output data/tonights-odds.csv

# 1. Ingest the fresh odds into the cube
mc tessera apply tonights-odds.recipe.yaml

# 2. Query bet recommendations
BETS=$(mc model query nba-totals.yaml \
  --where "Should_Bet == 1 and Time == '2025_04_15'" \
  --show "Game,Sportsbook,Calibrated_P,EV_Per_Dollar,Kelly_Fraction" \
  --format json)

# 3. Check a specific line movement (what-if)
mc model whatif nba-totals.yaml \
  --set "...Sportsbook=Pinnacle,Game=LAL_at_BOS,Measure=Market_Line" \
  --value 223.0 \
  --show "EV_Per_Dollar,Should_Bet" \
  --format json

# 4. Export tonight's full analysis to a file for archiving
mc model export nba-totals.yaml \
  --where "Time == '2025_04_15'" \
  --output /var/data/analysis_2025_04_15.json \
  --format json

# 5. Trace why the model likes one particular game
mc model trace nba-totals.yaml \
  --coord "...Game=LAL_at_BOS,Measure=EV_Per_Dollar" \
  --format json

# That's it. No Python. No custom inference server. No manual CSV formatting.
# API → transform → ingest → query → JSON. Five commands, zero scripts.
```

This is the test. If an agent can run these 6 commands and get correct,
parseable JSON back for each — Phase 6A is done. The `transform` step
is what eliminates the "write a Python flattener" bottleneck that burned
tokens on the NBA cartridge build and on the Tide Cleaners proof.
