# Phase 4A Completion Report — Mosaic Claude Code Plugin

**Project:** Mosaic — AI-powered Large Numbers Model platform (renamed from MarketingCubes V2 on 2026-05-03)
**ADR:** [`../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md`](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) — Accepted 2026-05-03 with 9 acceptance amendments
**Handoff:** [`../handoffs/phase-4a-handoff.md`](../handoffs/phase-4a-handoff.md)
**Operating manual:** [`../../CLAUDE.md`](../../CLAUDE.md)
**Initial commit (base):** `5ea0f02` — *docs: rename MarketingCubes V2 → Mosaic; reframe positioning as LNM platform; add Phase 3C/3D for-dummies notes* (the rename commit on top of `phase-3d-friendly-formula-syntax`)
**Toolchain:** Rust 1.78 (pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml)) — **unchanged**

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Build gate | ✓ zero warnings |
| `cargo fmt --check --all` | Format gate | ✓ exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint gate | ✓ zero warnings (0 errors, -D enforced) |
| `cargo test --workspace` | Test gate | ✓ **416/0** (was 396/0 at Phase 3D; +20 from Phase 4A) |
| `for i in $(seq 1 10); do cargo test --workspace -q; done` | Determinism (10×) | ✓ 10/10 identical at 416 each run |
| `cargo run --release --bin mc -- demo` | Rust demo | ✓ matches brief §4.6 (unchanged) |
| `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` | YAML demo (source) | ✓ matches Rust demo byte-for-byte |
| `cargo run --release --bin mc -- demo --model mosaic-plugin/examples/models/acme-marketing.yaml` | YAML demo (plugin example) | ✓ matches Rust demo byte-for-byte (plugin example round-trips through kernel) |
| `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` | Acme validate | ✓ exit 0 |
| `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` | Acme lint | ✓ exit 0; **zero warnings** |
| `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` | Acme goldens | ✓ exit 0; **9/9 pass** |
| `echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \| mc mcp` | MCP manual smoke | ✓ valid JSON-RPC; lists 5 tools (`mosaic.demo`, `mosaic.model.{validate,inspect,lint,test}`) |
| `git diff phase-3d-friendly-formula-syntax -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` | Locked-surfaces diff | 55 lines (all from inherited `5ea0f02` rename commit; **0 lines vs `5ea0f02` baseline** — see §3 deviation 1) |
| `git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` | Phase 4A locked-surfaces diff | ✓ **0 lines** |
| End-to-end fresh-instance proof | Headline acceptance gate | ✓ in-session best-effort: see [`phase-4a-proof/transcript.md`](phase-4a-proof/transcript.md). Real fresh-instance verification needs the user to install the plugin in a separate Claude Code session (called out in §6). |

---

## 2. Final test count

**Total: 416 tests passed / 0 failed.**

Per target (deltas vs Phase 3D in **bold**):

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit + integration | 219 | unchanged from Phase 3D |
| `mc-fixtures` unit | 16 | unchanged |
| `mc-model` unit + integration | 161 | unchanged |
| `mc-cli` `tests/mcp_smoke.rs` (Phase 4A) | **+8** | initialize, tools/list, tools/call ×3 (validate/lint/test), unknown-tool error, parse error, notifications |
| `mc-cli` `tests/example_byte_identity.rs` (Phase 4A) | **+2** | plugin YAML byte-identical to source; plugin CSV byte-identical to source |
| `mc-cli` `tests/plugin_lint.rs` (Phase 4A) | **+10** | manifest keys, .mcp.json shape, skills frontmatter, agents frontmatter, commands frontmatter, no provider-specific tags, markdown-only under skills/agents/commands, marketing-mix is only domain, MC3008 is documented as retired, examples/adapters is Phase 4B placeholder |
| **Total** | **416** | **+20** from Phase 3D's 396 |

### Determinism gate

10 consecutive `cargo test --workspace -q` runs all reported the same 416/0 totals. No flakes.

---

## 3. Deviations from the brief / handoff

Surface every deviation honestly per CLAUDE.md §10.3. Six deviations, each with a rationale in §4.

1. **The handoff's reference at `/Users/edwinlovettiii/runtimescope/plugin/` did not exist on this machine.** Resolved via SPEC QUESTION: substituted the cached vercel/0.40.1 + superpowers/5.0.7 plugins under `~/.claude/plugins/cache/claude-plugins-official/` as the canonical Claude Code plugin reference. User confirmed (runtimescope is at github.com/edwinlov3tt/runtimescope but not cloned here).
2. **Plugin manifest path / field shape diverges from ADR-0008 Decision 3 sketches.** The canonical Claude Code format places `plugin.json` in `.claude-plugin/`, uses `commands[]` + `agents[]` arrays (not directory keys), drops `displayName` / `mcpServers` / `hooks` keys, and skills are auto-discovered from `skills/<name>/SKILL.md`. ADR-0008's strategic decisions all hold; only the manifest packaging shape changed.
3. **Plugin CSV file is named `acme.inputs.csv`, not `acme-marketing.inputs.csv` per the handoff source manifest.** The byte-identical YAML's `source: "acme.inputs.csv"` reference requires the sibling CSV to keep its original filename for `mc demo --model mosaic-plugin/examples/models/acme-marketing.yaml` to resolve.
4. **Hooks are not shipped.** `mosaic-plugin/hooks/` contains only a `README.md` placeholder. Per handoff §I, hooks "matter less than skills/agents/commands"; the canonical hook-spec format couldn't be verified from inside the build session, and the headline acceptance gate doesn't need hooks.
5. **`mc mcp` parser body is 318 lines, over the 250-line trigger #10 budget.** Surfaced via SPEC QUESTION; user authorized "keep" with rationale: the implementation works end-to-end, all four MCP message kinds + 5 tools verified, no new deps, sync-only — and the handoff's own estimate was 300–500 lines as realistic. Final counts: parser body 318 lines (~245 non-comment); separate emitter 66 lines.
6. **Locked-surfaces diff against `phase-3d-friendly-formula-syntax` is 55 lines, not 0.** All 55 lines are from the inherited `5ea0f02` rename commit (Cargo.toml descriptions + `mc-core/src/lib.rs` doc-comment update from "MarketingCubes" → "Mosaic"). They landed before Phase 4A started. **Phase 4A added 0 lines to the locked crates** (`git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns 0 lines).

Plus a non-deviation worth flagging: during the end-to-end proof I caught a **bug in two of the plugin's own skills** (`skills/authoring/SKILL.md` and `skills/testing/SKILL.md` documented `canonical_inputs: { rows: [...] }` which doesn't match the actual `ParsedInputSet` schema — the right shape is `canonical_inputs: { columns: [...], inline: { rows: [[...]] } }`). Both skills were corrected in the same Phase 4A commit. The iteration loop the plugin teaches caught the bug; documenting that as a feature rather than a deviation.

---

## 4. Rationale per deviation

### 4.1 Missing runtimescope reference

**What the handoff said:** "A working Claude Code plugin lives at `/Users/edwinlovettiii/runtimescope/plugin/`. … This is your canonical reference for the current Claude Code plugin format."

**What I did:** Surfaced via SPEC QUESTION. User confirmed the runtimescope repo is at GitHub but not cloned to this host (user is SSH'd into a different machine). User authorized using the locally-cached vercel/0.40.1 + superpowers/5.0.7 plugins as the substitute reference: "They're real, loaded-in-this-session ground truth. They beat both runtimescope and ADR-0008's sketches when there's a conflict."

**Rationale:** the cached plugins are what Claude Code itself loaded in the current session; they're a stronger reference than a documentation sketch. Trigger #1's escape hatch ("Document the actual current spec and what changed before adapting") was used as designed.

### 4.2 Plugin manifest path / field shape

**What ADR-0008 Decision 3 sketched:**

```json
{
  "name": "mosaic", "displayName": "Mosaic — ...",
  "skills": "./skills", "agents": "./agents", "commands": "./commands",
  "mcpServers": "./.mcp.json", "hooks": "./hooks"
}
```
…at `mosaic-plugin/plugin.json`.

**What the canonical format actually uses (per cached vercel + superpowers):**

```json
{
  "name": "mosaic", "version": "0.1.0", "description": "...",
  "author": {"name": "..."}, "repository": "...", "license": "...", "keywords": [...],
  "commands": ["./commands/foo.md", ...],
  "agents": ["./agents/foo.md", ...]
}
```
…at `mosaic-plugin/.claude-plugin/plugin.json`. Skills auto-discovered from `skills/<name>/SKILL.md`; hooks not present in either reference's manifest; `.mcp.json` is a sibling file at root.

**What I did:** adopted the canonical format. ADR-0008 Decision 3's strategic content (skills as markdown, agents as system prompts, commands as CLI invocations, MCP server config, marketing-mix only, no provider-specific tags) all carries forward unchanged; only the packaging shape differs.

**Rationale:** ADR-0008's sketch was an architectural intent based on docs that proved stale. The runtimescope reference would have been the corrective; in its absence the cached plugins are next-best evidence. Per the SPEC QUESTION user confirmation, this shape is binding for Phase 4A.

### 4.3 Plugin CSV filename

**What the handoff manifest declared:** `mosaic-plugin/examples/models/acme-marketing.inputs.csv`.

**What I shipped:** `mosaic-plugin/examples/models/acme.inputs.csv`.

**Why:** the byte-identical Acme YAML (`acme-marketing.yaml`) carries `source: "acme.inputs.csv"`. Renaming the CSV to `acme-marketing.inputs.csv` would break `mc demo --model mosaic-plugin/examples/models/acme-marketing.yaml`'s `resolve_inputs` stage. Three options were considered:
1. Edit the YAML's `source:` field to point at the renamed file → loses byte-identity.
2. Ship two copies of the CSV (one under each name) → wasteful + drift risk.
3. Keep the CSV's original name → byte-identity preserved on both files; demo round-trips cleanly.

Option 3 chosen. The byte-identity test (`tests/example_byte_identity.rs`) compares `acme.inputs.csv` ↔ source `acme.inputs.csv` instead of using the renamed name from the handoff manifest.

### 4.4 Hooks deferred

**What the handoff scope item 7 declared:** two hook files, `pre-commit-lint.json` + `post-edit-validate.json`, in `mosaic-plugin/hooks/`.

**What I shipped:** `mosaic-plugin/hooks/README.md` only — a placeholder noting the deferral.

**Why:** per handoff §I, hooks "matter less than skills/agents/commands" and "if hook format / spec details are unclear, ship minimal/no hooks." The cached canonical references (vercel, superpowers) use a `hooks/hooks.json` shape that I couldn't verify against a live Claude Code install from inside the build session. Shipping a structurally-wrong hook would either fail to load or trigger unexpected behavior; shipping an empty placeholder is safer and authorized by §I. The fresh-instance acceptance gate doesn't need hooks. Documented as a Phase 4A.1 follow-up candidate.

### 4.5 `mc mcp` parser size (318 lines, over 250 budget)

**What the handoff said:** "If hand-rolled JSON-RPC parsing exceeds 250 lines OR the MCP lifecycle … cannot be implemented cleanly without a real JSON parser or an MCP SDK, STOP and surface."

**What I did:** Stopped at 318 lines. Surfaced via SPEC QUESTION with full line counts, the working-state evidence (4 stdin/stdout transcripts of all MCP message kinds), and the explicit ask: keep or fallback?

**User's decision:** keep, with rationale recorded:

> "Trigger #10 worked as designed. The threshold is 'stop and surface so we can decide together,' not 'absolute hard limit.' You stopped at 318. We're deciding. Done. … You're at the bottom of the handoff's own 300–500 realistic estimate. The 250-cap was the optimistic anchor; 318 is the realistic anchor. … Every other constraint held cleanly: no new deps, no serde_json, no tokio, no async, no MCP SDK, sync-only, zero warnings, Phase 3B diagnostics module reused verbatim."

**Final line counts:**

| Region | Lines | What |
|---|---:|---|
| `enum JsonValue` + `impl` helpers | 45 | Value type + 6 accessor methods |
| `struct ParseCursor` + `parse_json` | 19 | Cursor + entry point |
| `impl ParseCursor` (parse_value, parse_object, parse_array, parse_string, read_hex4, parse_bool, parse_null, parse_number) | 254 | All parsing methods including UTF-8 multibyte handling + `\uXXXX` surrogate pairs |
| **Parser body total** | **318** | (~245 non-comment, non-blank) |
| **Emitter (separate)** | 66 | `json_emit` + `json_emit_string` |

**The four constraints that DID hold cleanly** (per the user's confirmation language):

1. **No new dependencies.** Hand-rolled parser + emitter; reuses Phase 3B's `mc_model::diagnostics_to_json` verbatim for the diagnostic envelope. Zero new crates.
2. **No async.** Sync-only. No tokio, no async-trait, no async fn.
3. **No MCP SDK.** Hand-rolled JSON-RPC 2.0 lifecycle (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping`).
4. **No toolchain bump.** `rust-toolchain.toml` unchanged at 1.78. Cargo.lock pins (`clap`, `clap_lex`, `half`, `indexmap`, `hashbrown`) all unchanged.

**The working-state evidence (the 4 stdin/stdout transcripts):**

```
$ echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | mc mcp
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"mosaic","version":"0.1.0"}}}

$ echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | mc mcp
{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"mosaic.demo",...},{"name":"mosaic.model.validate",...},{"name":"mosaic.model.inspect",...},{"name":"mosaic.model.lint",...},{"name":"mosaic.model.test",...}]}}

$ echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"mosaic.model.validate","arguments":{"path":"acme.yaml"}}}' | mc mcp
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{schema_version: \"1.0\", diagnostics: []}"}],"isError":false,"exit_code":0,...}}

$ echo '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"mosaic.model.test","arguments":{"path":"acme.yaml"}}}' | mc mcp
# 9/9 goldens pass; envelope shape {schema_version, skipped, goldens[]}
```

**Binding policy going forward:** The 318-line allowance is a Phase 4A scope-specific decision; it does not loosen the 250-line cap for future `mc mcp` extensions. Any future addition that pushes the parser further fires trigger #10 again.

### 4.6 Locked-surfaces diff vs phase-3d tag

**What the handoff said:** "`git diff phase-3d-friendly-formula-syntax -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns zero lines."

**Actual:** 55 lines.

**Why:** the rename commit `5ea0f02` (*docs: rename MarketingCubes V2 → Mosaic*) landed on `main` between `phase-3d-friendly-formula-syntax` and Phase 4A start. It edited 4 files: `mc-core/Cargo.toml`, `mc-core/src/lib.rs`, `mc-fixtures/Cargo.toml`, `mc-model/Cargo.toml` — all description strings + a doc-comment to update the project name. CLAUDE.md §"Project name + naming convention (rename note)" explicitly lists this update as binding (`Cargo.toml descriptions and lib.rs/main.rs lead doc-comments: updated to use "Mosaic" so cargo doc and cargo metadata reflect the current name`).

**Phase 4A vs `5ea0f02` baseline:** `git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns **0 lines**. Phase 4A added zero changes to the locked crates; the 55-line diff is inherited.

**What this means for the gate:** the *intent* of the locked-surfaces gate is "Phase 4A doesn't touch the kernel/fixtures/model layer." That intent is met. The literal `phase-3d-friendly-formula-syntax` baseline in the handoff was written before the rename commit landed.

---

## 5. Acceptance criteria — complete

| # | Criterion | Status |
|---:|---|---|
| 1 | `mosaic-plugin/` exists with the full directory tree from scope item 1 | ✓ (with the .claude-plugin/ + auto-discovered-skills shape per §3 deviation 2) |
| 2 | `plugin.json` valid per current Claude Code plugin spec | ✓ (lints clean against vercel/superpowers reference shape; `tests/plugin_lint.rs` enforces) |
| 3 | Every SKILL.md / agent / command has valid frontmatter + non-trivial body | ✓ enforced by `tests/plugin_lint.rs` (non-stub assertion: skill body > 200 chars) |
| 4 | `.mcp.json` invokes `mc mcp` and the server starts cleanly | ✓ manual smoke shows valid JSON-RPC, 5 tools listed |
| 5 | `mc mcp` smoke test in `tests/mcp_smoke.rs` passes | ✓ 8/8 tests pass (initialize, tools/list, tools/call ×3, unknown tool, parse error, notification) |
| 6 | Plugin example YAML + CSV byte-identical to source | ✓ enforced by `tests/example_byte_identity.rs`; CSV name kept as `acme.inputs.csv` per §3 deviation 3 |
| 7 | End-to-end fresh-instance proof | ⊕ in-session best-effort: produced `MyCo_Marketing_Q1_2026` from plugin content alone; validate/lint/test all green; transcript at [`phase-4a-proof/transcript.md`](phase-4a-proof/transcript.md). Real fresh-instance verification is the user's post-review step (see §6). |
| 8 | All 396 existing tests still pass; new total ≥ 396 + Phase 4A additions | ✓ 416/0 (was 396/0; +20 from Phase 4A) |
| 9 | Locked surfaces unchanged | ✓ 0 lines vs `5ea0f02` baseline (the rename commit); 55 lines vs `phase-3d-friendly-formula-syntax` are all from the inherited rename per §3 deviation 6 |
| 10 | Toolchain unchanged | ✓ `rust-toolchain.toml` and Cargo.lock pins all intact |
| 11 | JSON envelope `schema_version` stays `"1.0"` | ✓ `tests/schema_stability.rs` still passes (Phase 3C contract) |
| 12 | CLI carry-forwards behave identically | ✓ `mc demo / model {validate,inspect,lint,test}` all unchanged; both demo-equivalence diffs (source + plugin example) are empty |
| 13 | Plugin lints clean (no broken cross-links, consistent frontmatter, no provider-specific tags) | ✓ enforced by `tests/plugin_lint.rs` (10 tests including: marketing-mix is only domain, MC3008 documented retired, examples/adapters is Phase 4B placeholder) |

---

## 6. Acceptance criteria — deferred

| # | Criterion | Reason | Closure condition |
|---:|---|---|---|
| 7 (real-environment portion) | Fresh Claude Code instance with plugin installed produces working YAML | Cannot install a plugin into a separate Claude Code session from inside the build session. The in-session proof at `phase-4a-proof/transcript.md` is the best-effort substitute. | User runs `/mosaic-init marketing-mix` in a fresh Claude Code instance with `mosaic-plugin/` loaded; documents the transcript as a Phase 4A.1 amendment if anything diverges from the in-session proof. |
| Hooks | Two hooks (`pre-commit-lint.json`, `post-edit-validate.json`) | Canonical hook-spec format couldn't be verified from inside the build session. Per handoff §I, deferring is authorized; the headline gate doesn't need hooks. | A Phase 4A.1 amendment lands after the hook-spec format is verified against a live Claude Code install. |

---

## 7. Implemented files / modules

### Plugin (NEW — `mosaic-plugin/`, sibling to `crates/`)

- [`mosaic-plugin/.claude-plugin/plugin.json`](../../mosaic-plugin/.claude-plugin/plugin.json) — manifest in canonical Claude Code shape
- [`mosaic-plugin/README.md`](../../mosaic-plugin/README.md) — install instructions + plugin overview
- [`mosaic-plugin/.mcp.json`](../../mosaic-plugin/.mcp.json) — MCP server config invoking `mc mcp`
- [`mosaic-plugin/skills/authoring/SKILL.md`](../../mosaic-plugin/skills/authoring/SKILL.md) — top-level YAML structure + four-stage pipeline
- [`mosaic-plugin/skills/debugging/SKILL.md`](../../mosaic-plugin/skills/debugging/SKILL.md) — full MC1xxx–MC4xxx code registry through Phase 3D + JSON envelope shape + MC3008-retired rule
- [`mosaic-plugin/skills/schema-design/SKILL.md`](../../mosaic-plugin/skills/schema-design/SKILL.md) — dim order, MeasureRole, aggregation rules, rule constraints
- [`mosaic-plugin/skills/formulas/SKILL.md`](../../mosaic-plugin/skills/formulas/SKILL.md) — Phase 3D formula syntax (operators, `if_null`, MC1003–MC1006)
- [`mosaic-plugin/skills/testing/SKILL.md`](../../mosaic-plugin/skills/testing/SKILL.md) — canonical_inputs + test_fixtures + goldens + `--fixture` filter
- [`mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md`](../../mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md) — Acme as canonical marketing-mix reference
- [`mosaic-plugin/agents/mosaic-architect.md`](../../mosaic-plugin/agents/mosaic-architect.md) — designs schema from natural-language requirements
- [`mosaic-plugin/agents/mosaic-author.md`](../../mosaic-plugin/agents/mosaic-author.md) — writes YAML from architect's plan
- [`mosaic-plugin/agents/mosaic-debugger.md`](../../mosaic-plugin/agents/mosaic-debugger.md) — reads diagnostic envelopes, proposes fixes
- [`mosaic-plugin/agents/mosaic-validator.md`](../../mosaic-plugin/agents/mosaic-validator.md) — runs validate → lint → test sequence
- [`mosaic-plugin/commands/mosaic-init.md`](../../mosaic-plugin/commands/mosaic-init.md) — scaffold a new model
- [`mosaic-plugin/commands/mosaic-validate.md`](../../mosaic-plugin/commands/mosaic-validate.md)
- [`mosaic-plugin/commands/mosaic-inspect.md`](../../mosaic-plugin/commands/mosaic-inspect.md)
- [`mosaic-plugin/commands/mosaic-lint.md`](../../mosaic-plugin/commands/mosaic-lint.md)
- [`mosaic-plugin/commands/mosaic-test.md`](../../mosaic-plugin/commands/mosaic-test.md)
- [`mosaic-plugin/commands/mosaic-author.md`](../../mosaic-plugin/commands/mosaic-author.md) — end-to-end NL → YAML pipeline
- [`mosaic-plugin/hooks/README.md`](../../mosaic-plugin/hooks/README.md) — placeholder; hooks deferred per §3 deviation 4
- [`mosaic-plugin/examples/models/acme-marketing.yaml`](../../mosaic-plugin/examples/models/acme-marketing.yaml) — byte-identical to `crates/mc-model/examples/acme.yaml`
- [`mosaic-plugin/examples/models/acme.inputs.csv`](../../mosaic-plugin/examples/models/acme.inputs.csv) — byte-identical to `crates/mc-model/examples/acme.inputs.csv` (kept original name per §3 deviation 3)
- [`mosaic-plugin/examples/adapters/README.md`](../../mosaic-plugin/examples/adapters/README.md) — Phase 4B placeholder

**`/mosaic-explain` deferred to Phase 4A.2** (per handoff scope item 5; needs a `mc model trace <coord>` CLI verb that doesn't exist yet).

### Rust workspace

- [`crates/mc-cli/src/mcp.rs`](../../crates/mc-cli/src/mcp.rs) — NEW. `mc mcp` MCP server (JSON-RPC 2.0 over stdio); 318-line parser body + 66-line emitter; surfaces 5 tools wrapping `mc model {validate,inspect,lint,test}` + `mc demo`. No new deps. Per §3 deviation 5.
- [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) — modified. Added `mod mcp;`, the `"mcp"` clap arm, and one `mc mcp` line in `print_help`. No other behavior change.

### Tests

- [`crates/mc-cli/tests/mcp_smoke.rs`](../../crates/mc-cli/tests/mcp_smoke.rs) — NEW. 8 tests covering MCP lifecycle. Uses `env!("CARGO_BIN_EXE_mc")` to find the binary; takes ownership of stdin (`child.stdin.take()`) so dropping it sends EOF.
- [`crates/mc-cli/tests/example_byte_identity.rs`](../../crates/mc-cli/tests/example_byte_identity.rs) — NEW. 2 tests: plugin YAML and plugin CSV byte-identical to source.
- [`crates/mc-cli/tests/plugin_lint.rs`](../../crates/mc-cli/tests/plugin_lint.rs) — NEW. 10 tests over `mosaic-plugin/` content (manifest keys, .mcp.json shape, frontmatter validity per type, no provider-specific tags, markdown-only, marketing-mix is sole domain, MC3008 retirement positive control, examples/adapters Phase 4B placeholder).

### Documentation

- [`docs/reports/phase-4a-completion-report.md`](phase-4a-completion-report.md) — this file.
- [`docs/reports/phase-4a-proof/transcript.md`](phase-4a-proof/transcript.md) — end-to-end in-session proof transcript.
- [`docs/reports/phase-4a-proof/myco_marketing_q1_2026.yaml`](phase-4a-proof/myco_marketing_q1_2026.yaml) — proof YAML (3-channel, 3-market, Q1_2026 marketing-mix model produced from plugin content alone).
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — updated: Phase 4A flipped `proposed` → `complete`; build/test gate row updated to 416/0.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — updated: Phase 4A row flipped to **complete**, status section similarly; Phase 4B promoted to **proposed** (next-to-start).

---

## 8. Known follow-ups for the next phase

These are explicit hooks left in the code or surfaced during this phase. **They are not scheduled.**

1. **Phase 4A.1 (small amendment):** ship the two hooks (`pre-commit-lint.json`, `post-edit-validate.json`) once the canonical Claude Code hook-spec format is verified against a live install. See §3 deviation 4.
2. **Phase 4A.2 (small amendment):** add `mc model trace <coord>` CLI verb + the `/mosaic-explain` slash command that consumes it. The kernel has rule-chain trace per PERF.md §6.4; surfacing it as a CLI verb requires touching `mc-model` (which Phase 4A's locked-surfaces rule blocked).
3. **Phase 4B (Python reference adapters)** — promoted to `proposed` in MASTER_PHASE_PLAN.md. Two adapters under `mosaic-plugin/examples/adapters/`:
   - `anthropic-python/` — ~150 line iteration loop using the Anthropic Python SDK
   - `openai-python/` — ~150 line iteration loop using the OpenAI Python SDK
4. **Real fresh-instance proof.** §6 lists this as a deferred user task. Any divergence from the in-session proof (in `phase-4a-proof/transcript.md`) becomes a Phase 4A.1 amendment.
5. **Schema doc consistency.** During the end-to-end proof I caught and fixed two skill-bugs (`canonical_inputs` shape was wrong in `skills/authoring/SKILL.md` and `skills/testing/SKILL.md`). Phase 4A.1 should sweep all skill examples through the actual model crate's parser to catch any other documented-but-wrong shapes.

The previous phase's follow-ups (Phase 3D's none) carry forward unchanged.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit.

- **No new Rust dependencies.** `cargo metadata --no-deps` shows the same 4 mc-core runtime deps (`smallvec`, `ahash`, `thiserror`, `once_cell`); same mc-fixtures + mc-cli deps; same Phase 3A mc-model deps (`serde`, `serde_yaml`, `thiserror`). Cargo.lock pins (`clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`, `indexmap → 2.7.0`, `hashbrown → 0.15.5`) all unchanged.
- **No banned imports** (`tokio`, `async-trait`, `rayon`, `serde_json`, `anyhow`, `reqwest`, `hyper`, MCP SDK crate). Confirmed:
  ```bash
  $ grep -rn "use tokio\|use rayon\|use serde_json\|use anyhow\|use reqwest\|use hyper" crates/
  # zero matches
  ```
- **No `unsafe` / `async` / threads** in any new code.
- **No `unwrap()` / `expect()` / `panic!()` in `crates/mc-cli/src/mcp.rs`** beyond what's already authorized for `mc-cli` per CLAUDE.md §2.3 carve-out.
- **No `MC3008` reintroduced anywhere** — `tests/plugin_lint.rs::debugging_skill_documents_mc3008_retired` is a positive control; the debugging skill flags it as retired.
- **Marketing-mix is the only domain schema** under `mosaic-plugin/skills/domain-schemas/` — `tests/plugin_lint.rs::marketing_mix_is_only_domain_schema` enforces.
- **Locked input contracts unchanged** — `git diff 5ea0f02 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns 0 lines (per §3 deviation 6).
- **No Phase 4B Python files shipped.** `mosaic-plugin/examples/adapters/README.md` is the only file under `examples/adapters/`; `tests/plugin_lint.rs::examples_adapters_is_phase_4b_placeholder` enforces.
- **No `<anthropic_specific>` / `[OpenAI:` / `[Claude:` / `[GPT:` / `[Anthropic:` strings under `skills/`, `agents/`, `commands/`** — `tests/plugin_lint.rs::no_provider_specific_tags_in_plugin_content` enforces.
- **JSON envelope `schema_version` stays `"1.0"`** — `tests/schema_stability.rs` (Phase 3C) still passes; `mc mcp` reuses Phase 3B's envelope verbatim via `mc_model::diagnostics_to_json`.

---

*Phase 4A ships pending the user's commit + tag + post-review fresh-instance verification.*
