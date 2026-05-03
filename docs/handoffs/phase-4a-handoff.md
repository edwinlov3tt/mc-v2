# Phase 4A Handoff — Mosaic Claude Code Plugin

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 4A.
> **You inherit a green Phase 3D** (commit `d5ab355`, tag
> `phase-3d-friendly-formula-syntax`).
>
> **This phase ships the Mosaic Claude Code plugin**, plus one small
> Rust addition (`mc mcp` subcommand) so the plugin can drive the CLI
> over MCP. Everything else is markdown + JSON: skills (progressive
> disclosure knowledge), agents (autonomous specialists), commands
> (slash shortcuts), `.mcp.json`, hooks, and a marketing-mix example
> model.
>
> **Read [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) BEFORE this handoff.** ADR-0008 is the Accepted strategic gate
> for Phase 4. It is the load-bearing context for what's in scope and
> why. The "Strategic centerpiece (read this first)" section at the
> top of ADR-0008 is the single most important paragraph in the entire
> Phase 4 corpus — internalize it before writing a single skill.
>
> **Hard rule:** Phase 4A touches `mosaic-plugin/` (NEW top-level
> directory, sibling to `crates/`) and **the `mc-cli` crate only**.
> Within `mc-cli`, source changes are limited to `src/mcp.rs` (NEW) +
> a small wiring touch in `src/main.rs`; new test files under
> `crates/mc-cli/tests/` are allowed. It does NOT touch
> `crates/mc-core/`, `crates/mc-fixtures/`, `crates/mc-model/`,
> `docs/specs/`, or any kernel/fixture/model file. The locked-surfaces
> guarantee from Phases 2D / 3A / 3B / 3C / 3D carries forward.

---

## The one paragraph you must internalize before writing code

**The Mosaic plugin is the institutional knowledge of Mosaic, in a
form any agent framework — present or future — can consume. This is
the actual moat. Not the runtime, not the prompts, not the SDK
adapters.** The plugin teaches an AI agent everything it needs to
author a Mosaic model: the schema, the diagnostic-code registry, the
formula grammar, the validator/linter behavior, the test-fixture
pattern, and the canonical Acme reference. Loading the plugin into any
agent (Claude Code natively, Anthropic SDK loop reading the markdown,
OpenAI SDK doing the same, future providers via the same pattern)
gives that agent Mosaic-authoring competence.

You are building the cargo. The runtime is the vehicle. The cargo
ages on a ~10+ year half-life; the vehicle ages on ~3 years. **Optimize
the markdown, not the wiring.**

If at any point during implementation you find yourself thinking
"let me just bake this prompt detail into Rust code" or "I'll
hardcode this routing logic in `mc mcp`" — stop. The prompt detail
goes in `skills/` or `agents/`. The routing logic is the LLM reading
the agent's markdown. The Rust runtime is dumb on purpose.

---

## ADR-0008 amendments quick-reference (compactor insurance)

The 9 acceptance amendments folded into ADR-0008 on 2026-05-03 after
parallel GPT + Desktop reviews. Read the full ADR for context, but
this is the at-a-glance shape:

| # | Amendment | How it shows up in Phase 4A |
|---|---|---|
| **A** | **Drop `crates/mc-author/`. Phase 4B = Python adapters under `mosaic-plugin/examples/adapters/`. No SDK deps in Rust workspace. No tokio / async / reqwest.** | Phase 4A ships the plugin only; `examples/adapters/` is a placeholder README pointing at Phase 4B. The Rust workspace stays sync + dep-bounded. |
| **B** | **Phase 4C dissolved.** No vague TBD bucket. After 4B, next phase is Phase 5 (actuals); future schemas / providers / production polish are demand-driven phases. | The handoff makes no reference to Phase 4C beyond confirming it does not exist. MASTER_PHASE_PLAN.md says "No Phase 4C." explicitly. |
| **C** | **"Knowledge embuing" / plugin-as-institutional-knowledge elevated to top-level strategic centerpiece.** Future implementers will optimize for the wrong thing if they think the runtime is the deliverable. | The "one paragraph you must internalize" section above this table. |
| **D** | **Default provider for Phase 4B example adapters: Claude (Anthropic).** | Phase 4A doesn't ship adapters. When Phase 4B runs, `examples/adapters/anthropic-python/` is the first/canonical reference; `openai-python/` is the cross-provider proof. |
| **E** | **Plugin location: in-repo at `mosaic-plugin/`** (single source of truth, atomic commits with kernel changes). Once stable, extract to its own repo for marketplace distribution. | Phase 4A creates `mosaic-plugin/` at the workspace root, sibling to `crates/`. |
| **F** | **Phase 4A ships ONLY one domain schema: marketing-mix (Acme).** FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning are demand-driven future phases. | Only `mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md` exists. The hard-rules list and final checklist enforce this. |
| **G** | **Phase 4B starts with Anthropic Python + OpenAI Python only.** Defer TypeScript, Codex, Gemini, Mistral, Ollama, cost tracking, prompt hardening, schema marketplace. | Phase 4A does not start any adapter; the constraint is documented in the placeholder README so Phase 4B inherits it. |
| **H** | **The `mc mcp` subcommand in `mc-cli` is the single Rust addition Phase 4A needs.** ~150 lines target, no new deps, hand-rolled JSON serialization. | Scope item 9 in the prompt body. **See SPEC QUESTION trigger #10 below — the 150-line budget is optimistic-not-expected.** |
| **I** | **ADR-first flow confirmed for Phase 4** per process-notes §1 self-test (fails questions 1–3). | ADR-0008 was Accepted before this handoff was drafted, unlike the brief Phase 3D handoff-first experiment. Reverting to ADR-first for Phase 4 onward. |

If you read this table and a section of the prompt body below seems
to disagree, the ADR wins — but flag the discrepancy as a SPEC
QUESTION before acting on it; it likely means a compact dropped
something.

---

## Where Phase 3D ended

- **Phase 3D commit / tag:** `d5ab355` — *phase-3d: friendly formula syntax* — tag `phase-3d-friendly-formula-syntax`.
- **Test status:** 396 / 0 passing across all targets. 10/10 deterministic.
- **Demos:** `cargo run --release --bin mc -- demo` matches brief §4.6. `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` produces byte-for-byte identical output.
- **Headline carry-forwards (still hold):** `mc model lint` zero warnings; `mc model test` 9/9 goldens pass at ~32 ms; equivalence between Rust and YAML+CSV paths byte-identical on 2,520 coords; round-trip stable on all 5 Acme rules.
- **Acme YAML:** all 5 rules use formula form (`body: "Spend / CPC"` etc.). Structured form still loads (see `_acme_with_bad_golden.yaml`).
- **Toolchain:** Rust 1.78. Cargo.lock pins from Phase 1B (`clap`, `clap_lex`, `half`) + Phase 3A (`indexmap → 2.7.0`, `hashbrown → 0.15.5`). **Do not bump.** ADR-0008 Decision 11 is explicit: Phase 4 does NOT trigger the toolchain bump.
- **`mc-core`, `mc-fixtures` deps unchanged** since Phase 2D (mc-core) and Phase 1A (mc-fixtures). `mc-model` deps unchanged since Phase 3A.
- **Diagnostic-code registry through Phase 3D:** MC1001–MC1006 (parse), MC2001–MC2025 (validation; MC3008 retired and promoted to MC2011), MC3001–MC3007 + MC3009–MC3011 (lint; MC3008 permanently retired), MC4xxx (reserved). Stable JSON envelope: `{ "schema_version": "1.0", "diagnostics": [...] }`, deterministic emission order `(severity desc, code asc, yaml_pointer asc, message asc)`.

For the full Phase 3D audit see [`../reports/phase-3d-completion-report.md`](../reports/phase-3d-completion-report.md). For the Phase 4 strategic context see [`../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md`](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md) — re-read §0 (Hierarchy of authority), §1.1 (deviation log), §2.7 (no traits "for testability"), §3.1 (forbidden patterns), §6 (the gate), §11 (SPEC QUESTION format).

---

## Phase 4A prompt (verbatim — this is your contract)

> We are starting Mosaic Phase 4A: the Mosaic Claude Code Plugin.
>
> **Context.** Phases 3A → 3D built the deterministic Mosaic authoring foundation: YAML schema (3A), validation + diagnostics (3B), test fixtures (3C), friendly formula syntax (3D). Mosaic now has a complete, lint-clean, formula-friendly authoring path. Phase 4A is the first phase of the LLM-authoring half — and per [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md), the load-bearing deliverable is **the Mosaic plugin**: a portable, agent-framework-agnostic knowledge package that teaches any AI agent how to author Mosaic models. Phase 4B (Python reference adapters) consumes the plugin; Phase 4A produces it.
>
> **Goal.** Ship a complete, installable Mosaic Claude Code plugin at `mosaic-plugin/` (new top-level directory, sibling to `crates/`) such that:
>
> 1. A fresh Claude Code instance with the plugin installed can produce a YAML for an Acme-shaped marketing-mix model from a single `/mosaic-init marketing-mix` invocation, and the resulting YAML passes `mc model validate / lint / test` with zero documented warnings.
> 2. The plugin's content is **markdown + JSON only** — no provider-specific tags, no executable code in skills/agents/commands, no `<anthropic_specific>` or `[OpenAI: ...]` annotations anywhere in `skills/`, `agents/`, or `commands/`. Provider-specific runtime code (when it ships) lives in `mosaic-plugin/examples/adapters/` (Phase 4B, NOT this phase).
> 3. The plugin teaches the entire Mosaic authoring surface: schema (Phase 3A), diagnostic codes (Phases 3A/3B/3C/3D — MC1001–MC1006 + MC2001–MC2025 + MC3001–MC3011 with MC3008 retired), formula grammar (Phase 3D), test fixtures (Phase 3C), the canonical Acme reference, and the marketing-mix domain pattern.
> 4. `mc-cli` gains exactly one new subcommand — `mc mcp` — which runs the standard MCP server protocol over stdio and dispatches tool calls to the existing `mc model {validate, inspect, lint, test}` and `mc demo` implementations. ~150 lines. NO new dependencies. Uses Phase 3B's hand-rolled JSON serialization.
> 5. The plugin's `.mcp.json` invokes `mc mcp` so that Claude Code can call Mosaic's CLI verbs as MCP tool calls (no shell-out; the agent reasons over diagnostic JSON envelopes structurally).
> 6. The plugin contains exactly ONE domain schema — `domain-schemas/marketing-mix/` (Acme is the reference). FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning, and any other domain are **explicitly out of scope** for Phase 4A (per ADR-0008 amendment F). Each is its own demand-driven future phase when a real customer or proof requires it.
>
> **Phase 4A scope** (binding contract):
>
> 1. **Create `mosaic-plugin/` directory** at the workspace root (sibling to `crates/`, `docs/`, `research/`). The full structure (mirrors ADR-0008 Decision 3, with marketing-mix as the only domain schema):
>
>    ```
>    mosaic-plugin/
>    ├── plugin.json                         # manifest
>    ├── README.md                           # what + install instructions
>    │
>    ├── skills/
>    │   ├── authoring/SKILL.md              # how to write a Mosaic YAML model end-to-end
>    │   ├── debugging/SKILL.md              # how to read MC1xxx-MC4xxx diagnostics + fix patterns
>    │   ├── schema-design/SKILL.md          # designing dims, hierarchies, measures, rules
>    │   ├── formulas/SKILL.md               # Phase 3D formula syntax (operators, precedence, if_null, MC1003-MC1006)
>    │   ├── testing/SKILL.md                # canonical_inputs + golden tests + fixture pattern (Phase 3C)
>    │   └── domain-schemas/
>    │       └── marketing-mix/SKILL.md      # marketing-mix patterns; Acme as reference
>    │
>    ├── agents/
>    │   ├── mosaic-architect.md             # designs schemas from natural-language requirements
>    │   ├── mosaic-author.md                # writes YAML from a finalized schema design
>    │   ├── mosaic-debugger.md              # reads diagnostic JSON + proposes specific fixes
>    │   └── mosaic-validator.md             # runs validate/lint/test cycles + reports
>    │
>    ├── commands/                           # 6 commands in Phase 4A; /mosaic-explain deferred to 4A.2 (needs CLI trace verb that doesn't exist yet)
>    │   ├── mosaic-init.md                  # /mosaic-init <domain> — scaffold a new model
>    │   ├── mosaic-validate.md              # /mosaic-validate [path]
>    │   ├── mosaic-inspect.md               # /mosaic-inspect [path]
>    │   ├── mosaic-lint.md                  # /mosaic-lint [path]
>    │   ├── mosaic-test.md                  # /mosaic-test [path]
>    │   └── mosaic-author.md                # /mosaic-author "natural language description"
>    │
>    ├── .mcp.json                           # MCP server config — invokes `mc mcp`
>    │
>    ├── hooks/
>    │   ├── pre-commit-lint.json            # pre-commit: run `mc model lint` on YAML changes
>    │   └── post-edit-validate.json         # after edit: run `mc model validate`
>    │
>    └── examples/
>        ├── models/
>        │   ├── acme-marketing.yaml         # mirrors crates/mc-model/examples/acme.yaml
>        │   └── acme-marketing.inputs.csv   # mirrors crates/mc-model/examples/acme.inputs.csv
>        └── adapters/                       # EMPTY in Phase 4A. Created by Phase 4B.
>            └── README.md                   # placeholder pointing at Phase 4B
>    ```
>
> 2. **`plugin.json` manifest** — follows the current Claude Code plugin spec. Reference shape (exact field names + version compat verified against the current Claude Code plugin docs at handoff execution time):
>
>    ```json
>    {
>      "name": "mosaic",
>      "displayName": "Mosaic — Large Numbers Model authoring",
>      "version": "0.1.0",
>      "description": "Author, validate, lint, test, and inspect Mosaic YAML model files. Phase 4A ships the marketing-mix domain (Acme reference); additional domains (FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning) land as demand-driven future phases.",
>      "author": "Mosaic project",
>      "license": "MIT OR Apache-2.0",
>      "repository": "https://github.com/edwinlov3tt/mc-v2",
>      "skills": "./skills",
>      "agents": "./agents",
>      "commands": "./commands",
>      "mcpServers": "./.mcp.json",
>      "hooks": "./hooks"
>    }
>    ```
>
>    If the current Claude Code plugin spec uses different field names (e.g., `mcp_servers` snake_case vs `mcpServers` camelCase), adapt to the actual spec. Cross-reference: an existing plugin already on this machine lives at `/Users/edwinlovettiii/runtimescope/plugin/` — read its `plugin.json` for the canonical field shape.
>
> 3. **Skills (markdown with frontmatter)** — each `SKILL.md` follows Claude Code's progressive-disclosure pattern. Frontmatter declares `name`, `description`, `trigger_keywords`. Body is the actual teaching content. Each skill MUST be self-sufficient — a fresh Claude Code instance loading just that skill (without any other Mosaic context) should be able to act on its content.
>
>    Required skill content:
>
>    - **`skills/authoring/SKILL.md`** — end-to-end "how to write a Mosaic YAML model": top-level structure (`metadata`, `dimensions`, `measures`, `rules`, `canonical_inputs`, `test_fixtures`); the four-stage pipeline (parse → validate → resolve_inputs → compile); when to use `mc model validate` vs `inspect` vs `lint` vs `test`. Cross-references to the deeper skills (formulas, debugging, etc.).
>    - **`skills/debugging/SKILL.md`** — the full diagnostic-code registry through Phase 3D (MC1001–MC1006 + MC2001–MC2025 + MC3001–MC3011 with MC3008 retired, MC4xxx reserved). For each code: what it means, what fires it, the typical fix pattern, an example before/after. The JSON envelope shape (`schema_version: "1.0"`, deterministic emission order). MUST include the explicit policy notes: "MC1004 covers both unexpected tokens AND unknown function calls in Phase 3D" and "MC3008 is permanently retired — was promoted to MC2011 (validation) in Phase 3B; do not introduce any lint code MC3008".
>    - **`skills/schema-design/SKILL.md`** — how to think about dimensions (Scenario, Version, Time, Channel, Market, Measure — exactly this order; immutable per ADR-0001 and the brief), hierarchies (single tree per dim in Phase 1; weights in `[0.0, 1.0]`), measures (`Input` vs `Derived`; aggregation rules — Sum / WeightedAverage / Min / Max; weight measures for WeightedAverage), rules (declared dependencies; scope: AllLeaves; targets a single measure; well-typed body). The dim-order rule is non-negotiable; an LLM that emits a different order produces invalid YAML.
>    - **`skills/formulas/SKILL.md`** — Phase 3D formula syntax. Operators (`+ - * /`, parens, unary `+`/`-`); function calls (only `if_null(primary, fallback)`; no `min`/`max`/`if`/comparisons); identifiers (case-sensitive measure names); numbers (F64; integer literals auto-promote; scientific notation OK). The unary minus desugaring (`Sub([Const(F64(0.0)), x])`). The MC1003–MC1006 error patterns. The Acme rule examples (`Spend / CPC`, `Customers * AOV`, `Revenue * (1 - COGS_Rate)`). Document the fallback option to author in structured form for cases where formula syntax falls short — both forms are equally valid (see Phase 3D's `_acme_with_bad_golden.yaml`).
>    - **`skills/testing/SKILL.md`** — how to write `canonical_inputs` (always-loaded reference inputs, one row per coord, can be inline tabular YAML OR sibling CSV per Phase 3C); how to write `test_fixtures` (named multi-fixture, scoped to a single test); how to write golden assertions (exact-match within 1e-9; reference fixtures by name); the `mc model test --fixture <name>` filter flag (filter-only, NOT injection); the snapshot/rollback mechanism between goldens; the perf gate (`mc model test acme.yaml < 500 ms` is the contract; ~32 ms is the current measured baseline at HEAD).
>    - **`skills/domain-schemas/marketing-mix/SKILL.md`** — the marketing-mix domain pattern. Acme is the canonical reference. The 6 dims (Scenario, Version, Time, Channel, Market, Measure); the 11 measures (Spend, CPC, Clicks, CVR, Leads, Close_Rate, Customers, AOV, Revenue, COGS_Rate, Gross_Profit); the 5 rules (clicks_rule, leads_rule, customers_rule, revenue_rule, gross_profit_rule); the hierarchy structure (channel rollups; market rollups). Common variations (different channel mixes; different time grains; weekly vs monthly; multiple scenarios). Reference the `examples/models/acme-marketing.yaml` example.
>
> 4. **Agents (markdown with system-prompt frontmatter + tool-use specs)** — each agent is an autonomous specialist with a clear "when to use" trigger. Frontmatter declares `name`, `description`, `when_to_use`, `tools`. Body is the system prompt + working procedure.
>
>    Required agent content:
>
>    - **`agents/mosaic-architect.md`** — designs the schema from natural-language requirements. "User says 'marketing-mix model for 5-channel B2C SaaS'; you produce: dim list (with element membership), measure list (Input vs Derived classification), rule list (target_measure + body + declared_dependencies)." Output is a structured plan, NOT a YAML file (that's mosaic-author's job). Hands off to mosaic-author once the plan is reviewed.
>    - **`agents/mosaic-author.md`** — writes the actual YAML from the architect's plan. Knows the YAML schema cold (cross-references `skills/authoring/SKILL.md`). Emits the file; runs `mc model validate` via the MCP tool; if errors, hands off to mosaic-debugger.
>    - **`agents/mosaic-debugger.md`** — reads diagnostic JSON envelopes by code; proposes specific YAML edits with before/after rationale. Cross-references `skills/debugging/SKILL.md` for the full code registry. Iterates until validate is clean, then hands off to mosaic-validator.
>    - **`agents/mosaic-validator.md`** — runs the full validate → lint → test sequence; reports result. If lint warnings exist, asks the user whether to fix or document each. If golden assertions fail, hands back to mosaic-debugger for reconciliation. Final output is "model is clean and ready" or a specific failure summary.
>
> 5. **Commands (markdown describing the slash command + its CLI mapping)** — each command file describes what the slash command does, its arguments, and the underlying `mc-cli` invocation. Frontmatter declares `name`, `description`, `arguments`. Body is the command logic + example usage.
>
>    Required command content (6 commands; `/mosaic-explain` deferred — see note below):
>
>    - **`/mosaic-init <domain>`** — scaffolds a new model. Default domain: `marketing-mix`. Drops a starter YAML referencing the domain skill. Phase 4A only supports `marketing-mix` (per ADR-0008 amendment F).
>    - **`/mosaic-validate [path]`** — calls `mc model validate <path>` via MCP; renders the diagnostic envelope.
>    - **`/mosaic-inspect [path]`** — calls `mc model inspect <path>` via MCP; renders the model summary.
>    - **`/mosaic-lint [path]`** — calls `mc model lint <path>` via MCP; renders any warnings.
>    - **`/mosaic-test [path]`** — calls `mc model test <path>` via MCP; reports goldens passed/failed.
>    - **`/mosaic-author "<natural-language description>"`** — invokes the mosaic-architect agent → mosaic-author → mosaic-debugger → mosaic-validator pipeline. End-to-end "natural language → working YAML".
>
>    **`/mosaic-explain <coord>` is DEFERRED to Phase 4A.2.** The command would walk a trace tree for a computed value, but `mc-cli` does not currently expose a `trace` verb (the kernel has internal rule-chain trace per PERF.md §6.4, but no CLI surface for it). Phase 4A's locked-surfaces rule blocks adding a kernel-touching CLI verb, so a 4A `/mosaic-explain` would degrade to "the LLM walks the YAML's rule body AST and pretty-prints which measures it references" — meaningfully weaker than the other commands. Per the GPT/Desktop review consolidation, ship six strong commands now; add `/mosaic-explain` in Phase 4A.2 alongside a real `mc model trace <coord>` CLI verb.
>
> 6. **`.mcp.json`** — MCP server configuration. Single server entry pointing at the `mc mcp` subcommand:
>
>    ```json
>    {
>      "mcpServers": {
>        "mosaic": {
>          "command": "mc",
>          "args": ["mcp"],
>          "env": {}
>        }
>      }
>    }
>    ```
>
>    Assumes `mc` is on the PATH (Phase 4A's plugin README documents the install precondition: `cargo install --path crates/mc-cli` or equivalent).
>
> 7. **Hooks (JSON event configurations)** — minimal Phase 4A hooks; the plugin can grow more later.
>
>    - **`hooks/pre-commit-lint.json`** — fires on git pre-commit; runs `mc model lint` on any modified `.yaml` file under a `mosaic/` or similar conventional path. Format follows current Claude Code hook spec.
>    - **`hooks/post-edit-validate.json`** — fires after the user edits a Mosaic YAML file in the editor; runs `mc model validate` and surfaces any errors inline.
>
> 8. **Examples (`examples/models/acme-marketing.yaml` + `acme-marketing.inputs.csv`)** — canonical reference Acme model + inputs CSV. **These MUST be byte-identical content to `crates/mc-model/examples/acme.yaml` + `crates/mc-model/examples/acme.inputs.csv`** (the source of truth; the plugin example is a copy for plugin self-containment, NOT a divergent fork). A test asserts byte-identical content. If the source files change in a future phase, the plugin example updates in lockstep — but the source is canonical.
>
> 9. **`mc mcp` subcommand in `mc-cli`** — the single Rust addition. ~150 lines. Lives in `crates/mc-cli/src/` (probably as a new module like `mcp.rs` invoked from `main.rs`). Behavior:
>
>    - Reads JSON-RPC 2.0 messages on stdin, one per line (newline-delimited) OR per the MCP protocol's framing convention (verify against current MCP spec at handoff execution time).
>    - Surfaces these tools: `mosaic.demo`, `mosaic.model.validate`, `mosaic.model.inspect`, `mosaic.model.lint`, `mosaic.model.test`. Each tool wraps the existing CLI verb implementation; the tool's input schema mirrors the CLI args.
>    - Each tool returns a structured response: `{ "exit_code": 0|N, "stdout": "...", "diagnostics": [...] }` where `diagnostics` is the existing Phase 3B JSON envelope when the verb is one that emits diagnostics. The diagnostic envelope MUST be the same `schema_version: "1.0"` shape — no new envelope shape for MCP transport.
>    - Hand-rolled JSON serialization (the same module Phase 3B established for `--format json`). NO `serde_json` dep added. NO `tokio`, NO `async`, NO `reqwest`, NO HTTP. Stdio only.
>    - Wrapped in the existing CLI's error-handling pattern. If the underlying verb panics, `mc mcp` returns a JSON-RPC error response, not a process crash.
>    - Tested via `tests/mcp_smoke.rs` in `mc-cli`: spawn `mc mcp` as a subprocess; pipe JSON-RPC messages to its stdin; assert the responses parse, contain the expected tool names on `tools/list`, and produce the right diagnostics envelope shape on a `tools/call` invocation against `mosaic.model.validate`. Smoke test only; doesn't replicate the full MCP protocol conformance suite.
>
>    **MCP spec note:** the MCP wire protocol details (initialization handshake, tool listing, tool calling, error responses) MUST be cross-referenced against the current MCP specification at execution time. The MCP protocol has a versioned spec; pin to whichever version Claude Code currently consumes.
>
> **Hard rules:**
>
> - **`crates/mc-core/` is LOCKED.** No source change, no Cargo.toml change. `git diff phase-3d-friendly-formula-syntax -- crates/mc-core/` returns zero lines.
> - **`crates/mc-fixtures/` is LOCKED.** No source change. `git diff phase-3d-friendly-formula-syntax -- crates/mc-fixtures/` returns zero lines.
> - **`crates/mc-model/` is LOCKED.** No source change, no schema extension, no new validator, no new diagnostic code in Phase 4A. `git diff phase-3d-friendly-formula-syntax -- crates/mc-model/` returns zero lines (modulo possibly the `examples/acme.yaml` example file IF you fix something there — but ideally even that stays untouched).
> - **`crates/mc-cli/` only gets the `mc mcp` subcommand + its tests.** All other CLI behavior (`mc demo`, `mc model {validate, inspect, lint, test}`, the `--model` and `--format` flags) is unchanged.
> - **No new dependencies in any Rust crate.** The `mc mcp` subcommand uses only what's already in the workspace. No `serde_json`, no `tokio`, no `async-trait`, no `jsonrpc-*`, no MCP SDK crate.
> - **No Rust toolchain bump.** Rust stays at 1.78. Cargo.lock pins all stay intact (`clap`, `clap_lex`, `half`, `indexmap`, `hashbrown`).
> - **Plugin content is markdown + JSON only.** No `.py`, no `.ts`, no `.rs`, no `.sh`, no compiled artifacts in `mosaic-plugin/skills/`, `agents/`, `commands/`, or `hooks/`. (`examples/adapters/` is reserved for Phase 4B Python content — it stays empty in 4A modulo the placeholder README.)
> - **No provider-specific tags in `skills/`, `agents/`, or `commands/`.** No `<anthropic_specific>`, no `[OpenAI: ...]`, no model-specific instructions. Skills describe Mosaic; they don't describe how Claude vs GPT differs in handling them.
> - **Marketing-mix is the ONLY domain schema** (per ADR-0008 amendment F). No FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning skill files. Each is its own demand-driven future phase.
> - **The Acme example in `examples/models/` is byte-identical to the source-of-truth at `crates/mc-model/examples/`.** A test enforces this.
> - **No async, no tokio, no rayon, no threads.** Phase 4A's Rust addition is sync.
> - **No `unwrap()` / `expect()` / `panic!()` in `crates/mc-cli/src/`** beyond what's already present (Phase 1A allows the existing CLI carve-out per CLAUDE.md §2.3 — `mc-cli` may use `expect("static reason")`). The new `mc mcp` code follows the same rules.
> - **Diagnostic JSON envelope `schema_version` stays at `"1.0"`.** Phase 4A may add MC4xxx reserved-namespace codes IF an LLM-specific concern surfaces (none expected), but the struct shape doesn't change. ADR-0006 amendment #20 still binds.
> - **All 396 existing tests must still pass.** New total ≥ 396 + Phase 4A `mc mcp` test additions + the byte-identical-Acme-example test.
> - **You did NOT start Phase 4B.** No Python files. No SDK installs. The `mosaic-plugin/examples/adapters/` directory is a placeholder for Phase 4B; create it with a README that says "Phase 4B deliverable — not yet shipped".
>
> **Acceptance gate (the headline + supporting):**
>
> Headline: **A fresh Claude Code instance with the Mosaic plugin installed produces a YAML for an Acme-shaped marketing-mix model from a single `/mosaic-init marketing-mix` (or equivalent end-to-end agent) invocation, and the resulting YAML passes `mc model validate / lint / test` with zero documented warnings.**
>
> Supporting:
>
> 1. `mosaic-plugin/` directory exists with the full structure listed in scope item 1.
> 2. `mosaic-plugin/plugin.json` is valid per current Claude Code plugin spec (lint/parse cleanly).
> 3. Every `SKILL.md`, agent `.md`, and command `.md` has valid frontmatter + non-trivial body content (not stubs). Each skill is independently usable.
> 4. `.mcp.json` references `mc mcp` and the MCP server starts cleanly when invoked manually (`echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | mc mcp` returns a valid JSON-RPC response listing the 5 expected tools). **Caveat: if SPEC QUESTION trigger #10 fires and Phase 4A.1 fallback is taken, this gate becomes "the `.mcp.json` is structurally valid; runtime integration is Phase 4A.2."**
> 5. `mc mcp` smoke test in `crates/mc-cli/tests/mcp_smoke.rs` passes — spawns the subprocess, sends a `tools/list` and a `tools/call` for `mosaic.model.validate` against a known-good YAML, asserts response shapes. (Skip if Phase 4A.1 fallback taken.)
> 6. `examples/models/acme-marketing.yaml` and `examples/models/acme-marketing.inputs.csv` are byte-identical to `crates/mc-model/examples/acme.yaml` and `crates/mc-model/examples/acme.inputs.csv` respectively. A `tests/example_byte_identity.rs` test asserts this (in `mc-cli` or `mc-model` — your call; don't add to mc-fixtures).
> 7. **End-to-end fresh-instance proof:** with the plugin installed, a fresh Claude Code instance prompted with `/mosaic-init marketing-mix` (or `/mosaic-author "marketing-mix for a 5-channel B2C SaaS with monthly seasonality"`) produces a YAML that passes `mc model validate` AND `mc model lint` (zero warnings) AND `mc model test` (any goldens declared pass within 1e-9). Document the test transcript in the Phase 4A completion report (commands run + outputs).
> 8. All 396 existing tests still pass; new total ≥ 396 + Phase 4A test additions.
> 9. Locked surfaces: `git diff phase-3d-friendly-formula-syntax -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` returns zero lines.
> 10. Toolchain: `rust-toolchain.toml` unchanged; Cargo.lock pins intact.
> 11. JSON envelope `schema_version` stays at `"1.0"`. `tests/schema_stability.rs` still passes.
> 12. CLI carry-forwards: `mc demo`, `mc model validate / inspect / lint / test`, `mc demo --model <path>` all behave identically. The Phase 3D demo-equivalence diff is still empty.
> 13. **Plugin lints clean.** Markdown lints (no broken cross-links between skills/agents/commands; consistent frontmatter shape; no provider-specific tags). Add a `tests/plugin_lint.rs` (lives in `mc-cli` since `mc-cli` is the only crate touched, OR a small standalone shell script under `mosaic-plugin/.scripts/`) that performs minimal sanity checks: every skill has frontmatter; every agent has `when_to_use`; every command has `description`; no `<anthropic_specific>` / `[OpenAI:` strings anywhere in skills/agents/commands.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (≥ 396 + new tests)
> - `cargo run --release --bin mc -- demo` (matches brief §4.6 — Rust path, unchanged)
> - `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` (byte-identical to Rust path, unchanged)
> - `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` (exits 0)
> - `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` (snapshot unchanged)
> - `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` (exits 0; ZERO warnings)
> - `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` (9/9 goldens pass)
> - `echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | cargo run --release --bin mc -- mcp` (returns valid JSON-RPC; lists 5 tools)
> - `diff <(cargo run --release --bin mc -- demo) <(cargo run --release --bin mc -- demo --model mosaic-plugin/examples/models/acme-marketing.yaml)` (empty — proves plugin example matches)
> - 10 consecutive `cargo test --workspace -q` (deterministic)
> - `git diff phase-3d-friendly-formula-syntax -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/` (zero lines)
>
> **Documentation requirements:**
> - Append `docs/reports/phase-4a-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 4A from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 4A status row from `proposed` → `complete`.
> - **Do NOT modify ADR-0008.** Accepted contract. Amendments go in `0008-amendment-N.md`.
> - **Do NOT modify ADR-0004 / 0005 / 0006 / 0007.** Inherited contracts.
> - **Do NOT modify the brief, engine-semantics doc, or any spec.** Locked.
> - **Do NOT modify CLAUDE.md** unless an actual operating-rule learning surfaces (rare; flag it as a SPEC QUESTION first).
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
> 1. The current Claude Code plugin spec uses materially different field names / structure than ADR-0008 Decision 3 sketches (e.g., the manifest expects a different schema; skills must be in a different layout). Document the actual current spec and what changed before adapting.
> 2. The current MCP protocol spec uses framing / handshake / tool-call shape that doesn't fit "newline-delimited JSON-RPC over stdio". Document the actual MCP spec version and adapt before implementing `mc mcp`.
> 3. The plugin's `examples/models/acme-marketing.yaml` byte-identity test conflicts with how Claude Code packages plugins (e.g., the plugin distribution format requires a transformation that changes bytes). If so, propose alternate identity check (e.g., parse equality) and surface.
> 4. The `mc mcp` subcommand needs a dependency you can't avoid (e.g., MCP wire protocol mandates a specific framing library). Surface BEFORE adding the dep.
> 5. A skill/agent/command needs to embed Rust-side knowledge that's only currently expressed in `mc-core` source code (e.g., "the kernel rejects NaN at writeback"). Decide: is the knowledge a published guarantee (then it goes in the skill markdown freely) or implementation detail (then the skill should NOT depend on it). Surface if unclear.
> 6. The fresh-instance end-to-end proof produces a YAML that lints with non-zero warnings. Decide: is the warning the LLM's mistake (then the skill/agent needs better prompting) or a flaw in the lint rule (then surface — but DO NOT change the lint rule in Phase 4A; that's mc-model territory).
> 7. The plugin format requires a top-level field that depends on something Phase 4B will produce (e.g., declaring "supportedAdapters": ["python-anthropic", "python-openai"]). Phase 4A's plugin manifest should NOT declare 4B deliverables; if the plugin spec demands it, surface and propose a workaround (e.g., declare them but document them as "shipped in Phase 4B").
> 8. A skill body grows past ~500 lines OR you find yourself stretching to fill content. Skills should be focused. If you need to write 500 lines of a single skill, the skill should probably be split. Surface if the natural structure isn't clear.
> 9. The Acme model emitted by the LLM in the end-to-end proof passes validate/lint/test but is NOT structurally equivalent to `build_acme_cube()` — i.e., the LLM produced a different but equivalent marketing-mix model. Decide: does the acceptance gate require structural identity to Acme, or just "a passing marketing-mix model"? Per the prompt language ("Acme-shaped"), structural identity is NOT required — but the proof transcript should show what the LLM actually produced.
> 10. **The `mc mcp` parsing-budget trigger (load-bearing — read carefully).** The "~150 lines, no new deps, hand-rolled JSON-RPC" budget in scope item 9 is **optimistic, not expected.** *Emitting* JSON is what Phase 3B's `--format json` module already does (~50–100 lines walking a struct and writing strings). *Parsing* incoming JSON-RPC requests from stdin is a different category of work — a correct hand-rolled JSON tokenizer (string escapes including `\uXXXX`, embedded-newline rejection, integer-vs-float disambiguation, nested objects, scientific notation) plus a JSON-RPC envelope validator (`jsonrpc: "2.0"`, `id`, `method`, `params`) plus an MCP message-shape validator on top is realistically **300–500 lines minimum**. Phase 3B's existing module almost certainly only emits, not parses. **If hand-rolled JSON-RPC parsing exceeds 250 lines OR the MCP lifecycle (initialization handshake, tools/list, tools/call, error responses) cannot be implemented cleanly without a real JSON parser or an MCP SDK, STOP and surface.** Do NOT add `serde_json`, `tokio`, `async-trait`, `reqwest`, or any JSON-RPC / MCP crate without explicit approval. **The Phase 4A.1 fallback (rollback plan #1 below — ship the plugin content first, defer MCP integration) is the documented escape hatch; consider it BEFORE adding any dep.** A plugin that ships skills + agents + commands + .mcp.json config without a working `mc mcp` is still a meaningful Phase 4A deliverable; the MCP integration becomes Phase 4A.2 with a properly-scoped dep budget.
>
> **Rollback plan (in case complexity explodes):**
>
> If the plugin scope balloons (e.g., MCP protocol turns out to need an entire JSON-RPC framework, or Claude Code's plugin spec has incompatible requirements), **stop and write a SPEC QUESTION**. Two recovery paths:
> 1. **Narrow Phase 4A.1**: ship the plugin's skills + agents + commands + .mcp.json + hooks WITHOUT the `mc mcp` subcommand. The plugin still works for direct shell-out; MCP integration becomes Phase 4A.2. Requires ADR amendment.
> 2. **Skip the end-to-end proof for 4A**: ship the plugin content; defer the "fresh Claude Code instance produces working YAML" gate to Phase 4A.2 once a real install path exists. Requires ADR amendment + documented alternative gate (e.g., manual review of skill quality).
>
> Either fallback is a Phase 4A.1 amendment, not a Phase 4A scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 4A Mosaic Claude Code Plugin
>
> Build:    cargo build --release --workspace ✓
> Format:   cargo fmt --check --all ✓
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓
> Tests:    cargo test --workspace [N] / 0 (was 396 / 0)
> Demo (Rust):     cargo run --release --bin mc -- demo ✓
> Demo (YAML — mc-model source):  cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml ✓
> Demo (YAML — plugin example):   cargo run --release --bin mc -- demo --model mosaic-plugin/examples/models/acme-marketing.yaml ✓ (byte-identical)
> Validate:        mc model validate <acme.yaml> ✓
> Inspect:         mc model inspect <acme.yaml> ✓
> Lint:            mc model lint <acme.yaml> ✓ (ZERO warnings — Phase 3B carry-forward)
> Test:            mc model test <acme.yaml> ✓ (9/9 goldens pass)
> MCP smoke:       echo '{"jsonrpc":"2.0",...}' | mc mcp → valid JSON-RPC, 5 tools listed ✓
> Plugin lint:     no provider-specific tags, all frontmatter valid ✓
> Determinism:     10 / 10 identical
> End-to-end:      fresh Claude Code with plugin → /mosaic-init marketing-mix → validate/lint/test all green
>                  (transcript attached in completion report)
> Locked surfaces: mc-core / mc-fixtures / mc-model 0-line diff vs phase-3d-friendly-formula-syntax ✓
>
> Source manifest:
> - mosaic-plugin/                                        (NEW dir, sibling to crates/)
> - mosaic-plugin/plugin.json                             (NEW — manifest)
> - mosaic-plugin/README.md                               (NEW)
> - mosaic-plugin/skills/authoring/SKILL.md               (NEW)
> - mosaic-plugin/skills/debugging/SKILL.md               (NEW)
> - mosaic-plugin/skills/schema-design/SKILL.md           (NEW)
> - mosaic-plugin/skills/formulas/SKILL.md                (NEW)
> - mosaic-plugin/skills/testing/SKILL.md                 (NEW)
> - mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md  (NEW)
> - mosaic-plugin/agents/mosaic-architect.md              (NEW)
> - mosaic-plugin/agents/mosaic-author.md                 (NEW)
> - mosaic-plugin/agents/mosaic-debugger.md               (NEW)
> - mosaic-plugin/agents/mosaic-validator.md              (NEW)
> - mosaic-plugin/commands/mosaic-init.md                 (NEW)
> - mosaic-plugin/commands/mosaic-validate.md             (NEW)
> - mosaic-plugin/commands/mosaic-inspect.md              (NEW)
> - mosaic-plugin/commands/mosaic-lint.md                 (NEW)
> - mosaic-plugin/commands/mosaic-test.md                 (NEW)
> - mosaic-plugin/commands/mosaic-author.md               (NEW)
> # /mosaic-explain DEFERRED to Phase 4A.2 (needs mc model trace verb that doesn't exist yet)
> - mosaic-plugin/.mcp.json                               (NEW)
> - mosaic-plugin/hooks/pre-commit-lint.json              (NEW)
> - mosaic-plugin/hooks/post-edit-validate.json           (NEW)
> - mosaic-plugin/examples/models/acme-marketing.yaml     (NEW — byte-identical copy of crates/mc-model/examples/acme.yaml)
> - mosaic-plugin/examples/models/acme-marketing.inputs.csv  (NEW — byte-identical copy)
> - mosaic-plugin/examples/adapters/README.md             (NEW — placeholder for Phase 4B)
> - crates/mc-cli/src/mcp.rs                              (NEW — `mc mcp` subcommand, ~150 lines)
> - crates/mc-cli/src/main.rs                             (modified — wire mcp subcommand into clap)
> - crates/mc-cli/tests/mcp_smoke.rs                      (NEW — JSON-RPC roundtrip smoke test)
> - crates/mc-cli/tests/example_byte_identity.rs          (NEW — plugin example matches mc-model source)
> - crates/mc-cli/tests/plugin_lint.rs                    (NEW — sanity checks on mosaic-plugin/ content)
> - docs/reports/phase-4a-completion-report.md            (NEW)
> - docs/CURRENT_STATE.md                                 (updated — flip Phase 4A proposed → complete)
> - docs/roadmap/MASTER_PHASE_PLAN.md                     (updated)
>
> Implementation summary:
> - <one paragraph: plugin architecture, skill/agent/command organization, mc mcp protocol shape, fresh-instance proof transcript pointer>
>
> Deviations:
> - <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The reference plugin already on this machine

A working Claude Code plugin lives at `/Users/edwinlovettiii/runtimescope/plugin/`. Read its `plugin.json`, look at one of its skills, look at one of its agents, look at one of its commands, look at its `.mcp.json` and a hook file. **This is your canonical reference for the current Claude Code plugin format.** ADR-0008's Decision 3 sketch is the architectural intent; the runtimescope plugin is the proof-of-format. If they disagree, the runtimescope plugin format wins (it's what actually loads in Claude Code today).

Don't blindly copy structure — that plugin solves a different problem. But use it as ground truth for the field names, file extensions, frontmatter conventions, and directory layout that current Claude Code expects.

### B. The diagnostic-code registry (this is the LLM's grounding rails)

The plugin's `skills/debugging/SKILL.md` is the load-bearing teaching artifact. Get it right and the LLM can debug almost anything; get it wrong and the iteration loop fails. The full registry through Phase 3D:

**MC1xxx — parse errors (text input is malformed; YAML or formula):**
- MC1001 — YAML parse error (Phase 3B)
- MC1002 — YAML structure error / wrong type (Phase 3B)
- MC1003 — formula unbalanced/unexpected paren (Phase 3D)
- MC1004 — formula unexpected token OR unknown function call (Phase 3D; covers both per ADR-0007 amendment #25)
- MC1005 — formula expected expression (Phase 3D)
- MC1006 — formula invalid number literal (Phase 3D)

**MC2xxx — validation errors (text parsed but model is structurally wrong):**
- MC2001–MC2010 — structural validators (Phase 3A) — see ADR-0004 Decision 6's 10-row table
- MC2011 — empty rule body (Phase 3B; promoted from MC3008 lint rule)
- MC2012–MC2025 — fixture validators (Phase 3C) — see ADR-0006 Decision 7

**MC3xxx — lint warnings:**
- MC3001–MC3007, MC3009–MC3011 (Phase 3B) — quality concerns (style, redundancy, etc.)
- **MC3008 — PERMANENTLY RETIRED.** Was promoted to MC2011 (validation). Skill must explain that no MC3008 lint rule exists and the code MUST NOT be reintroduced.

**MC4xxx — reserved.** Phase 4 may add LLM-specific codes here if needed; none currently planned.

**The JSON envelope (Phase 3B contract):**
```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC2001",
      "severity": "error",
      "path": "/dimensions/0",
      "message": "...",
      "suggestion": "..."
    }
  ]
}
```
Sorted by `(severity desc, code asc, yaml_pointer asc, message asc)` — deterministic across runs. The `--format json` flag on every `mc model` verb emits this.

### C. The four-stage `mc-model` pipeline

The plugin's `skills/authoring/SKILL.md` should teach this clearly:

```
YAML bytes
    │
    ├──[ParseError]──► MC1xxx
    ▼
ParsedModel
    │
    ├──[ValidationError]──► MC2xxx
    ▼
ValidatedModel  ◄── (Optional) ResolveInputs reads sibling CSVs ──► MC2012-MC2025
    │
    ├──[EngineError]──► passthrough
    ▼
mc_core::Cube
```

`mc_model::load(path)` runs all four stages but does NOT apply inputs to the cube; `mc model test` is the only consumer of `apply_canonical_inputs` / `apply_fixture`. The `Diagnostic` envelope unifies output across all stages so the LLM doesn't have to special-case which stage emitted which code.

### D. The dim-order rule is non-negotiable

Every Mosaic cube has its dims in this order, exactly:

```
[Scenario, Version, Time, Channel, Market, Measure]
```

This is brief §3 + ADR-0001. An LLM that emits a different order produces invalid YAML. The `skills/schema-design/SKILL.md` MUST state this rule prominently and explain why (the kernel's `CellCoordinate` is positional against `cube.dimensions`; reordering breaks the storage contract).

### E. MeasureRole constraints in Phase 1

Phase 1 supports `MeasureRole::{Input, Derived}` — NOT `Both`. The brief change-log explicitly excludes `Both`. The plugin's `skills/schema-design/SKILL.md` should mention this so LLMs don't try to model "this measure is sometimes input and sometimes derived" (the workaround is two separate measures + a rule).

### F. Aggregation rules per measure (the WeightedAverage trap)

Acme's measures use a mix of aggregation rules:

- **Sum:** Spend, Clicks, Leads, Customers, Revenue, Gross_Profit
- **WeightedAverage (weighted by Spend):** CPC, CVR, Close_Rate, AOV, COGS_Rate

The plugin's `skills/schema-design/SKILL.md` must teach that ratios and rates use WeightedAverage with an explicit weight measure, NEVER simple averaging. This is the most common LLM mistake when authoring Mosaic models — defaulting to Sum or simple Mean for everything. CLAUDE.md §2.10 names this as the canonical Acme failure mode.

### G. The MCP protocol — version + framing pin

Cross-reference the current MCP specification (`https://modelcontextprotocol.io/specification` or whatever the canonical link is at execution time) before implementing `mc mcp`. The protocol has versioned shape; pin to the version Claude Code currently consumes. Key concerns:

- **Initialization handshake:** what does the server send on first stdin message?
- **Tool listing:** `tools/list` response shape (name, description, inputSchema).
- **Tool calling:** `tools/call` request (params: name, arguments) → response (content array).
- **Error responses:** JSON-RPC error code conventions; MCP-specific error codes if any.
- **Framing:** newline-delimited JSON, or length-prefixed, or other?

**If the MCP spec mandates anything Phase 4A's "no new deps" rule can't accommodate, file a SPEC QUESTION.** Don't quietly add a dep.

### H. The fresh-instance end-to-end proof

The acceptance gate's headline is "fresh Claude Code instance with plugin produces working YAML". This means:

1. Install the plugin into a Claude Code instance that has NEVER seen this repo (or use a fresh chat in the existing instance with the plugin loaded but with no project context).
2. Issue `/mosaic-init marketing-mix` (or `/mosaic-author "marketing-mix model for 5-channel B2C SaaS with monthly seasonality"`).
3. Watch the agent pipeline: mosaic-architect designs → mosaic-author writes → mosaic-debugger fixes → mosaic-validator confirms.
4. Take the resulting YAML and run `mc model validate / lint / test` on it.
5. **Document the full transcript** in the completion report — the prompts used, the agent transitions, the YAML produced, the validate/lint/test outputs.

This is a Phase 4A acceptance criterion that can ONLY be checked by actually doing it. It's not a `cargo test` you can automate at this stage. The completion report's transcript is the evidence.

### I. Why hooks/ matters less than skills/agents/commands

In Phase 4A, hooks are nice-to-have but not load-bearing. The two specified hooks (`pre-commit-lint.json`, `post-edit-validate.json`) make the developer-loop nicer but the plugin works without them. If hook format / spec details are unclear, ship minimal/no hooks and document the gap; don't burn time on hook polish at the cost of skill quality.

The skills + agents + commands ARE the deliverable. The hooks are decoration.

---

## Pointers to existing files you will most likely touch

| Why | File | Phase 4A action |
|---|---|---|
| Plugin manifest | `mosaic-plugin/plugin.json` | new — root-level config |
| Plugin README | `mosaic-plugin/README.md` | new — install instructions, quick-start |
| Authoring skill | `mosaic-plugin/skills/authoring/SKILL.md` | new — end-to-end YAML authoring |
| Debugging skill | `mosaic-plugin/skills/debugging/SKILL.md` | new — full diagnostic-code registry through Phase 3D |
| Schema-design skill | `mosaic-plugin/skills/schema-design/SKILL.md` | new — dims/hierarchies/measures/rules |
| Formulas skill | `mosaic-plugin/skills/formulas/SKILL.md` | new — Phase 3D formula syntax |
| Testing skill | `mosaic-plugin/skills/testing/SKILL.md` | new — canonical_inputs + fixtures + goldens |
| Marketing-mix domain skill | `mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md` | new — Acme as canonical reference |
| Agents | `mosaic-plugin/agents/mosaic-{architect,author,debugger,validator}.md` | new — 4 agent specs |
| Commands | `mosaic-plugin/commands/mosaic-{init,validate,inspect,lint,test,author}.md` | new — 6 slash commands (`/mosaic-explain` deferred to Phase 4A.2) |
| MCP config | `mosaic-plugin/.mcp.json` | new — points at `mc mcp` |
| Hooks | `mosaic-plugin/hooks/{pre-commit-lint,post-edit-validate}.json` | new — minimal Phase 4A hooks |
| Plugin example model | `mosaic-plugin/examples/models/acme-marketing.yaml` | new — byte-identical copy of `crates/mc-model/examples/acme.yaml` |
| Plugin example inputs | `mosaic-plugin/examples/models/acme-marketing.inputs.csv` | new — byte-identical copy |
| Phase 4B placeholder | `mosaic-plugin/examples/adapters/README.md` | new — placeholder pointing at Phase 4B |
| MCP subcommand | `crates/mc-cli/src/mcp.rs` | new — JSON-RPC over stdio, ~150 lines |
| CLI wiring | [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) | modify — add `Mcp` variant to clap subcommand enum, dispatch to `mcp::run()` |
| MCP smoke test | `crates/mc-cli/tests/mcp_smoke.rs` | new — spawn subprocess, send JSON-RPC, assert response |
| Byte-identity test | `crates/mc-cli/tests/example_byte_identity.rs` | new — `mosaic-plugin/examples/models/acme-marketing.yaml` matches `crates/mc-model/examples/acme.yaml`. **Resolve `mosaic-plugin/` from `CARGO_MANIFEST_DIR` by walking up two parents to the workspace root** (`Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("mosaic-plugin")`). Cargo runs tests with `CARGO_MANIFEST_DIR=crates/mc-cli`. |
| Plugin sanity-lint test | `crates/mc-cli/tests/plugin_lint.rs` | new — frontmatter check, no provider-specific tags. **Same `CARGO_MANIFEST_DIR` walk** as above to find the plugin root. |
| Phase 4A completion report | `docs/reports/phase-4a-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 4A from `proposed` → `complete` |

**Do not touch:**

- **`crates/mc-core/`** — entire crate locked.
- **`crates/mc-fixtures/`** — entire crate locked.
- **`crates/mc-model/`** — entire crate locked. No schema additions, no new validators, no new diagnostic codes, no new lint rules, no new fixture support.
- **`crates/mc-cli/src/main.rs`** — only the new `Mcp` clap variant + dispatch line. No other behavior change. No existing flag modification. No `--model` semantics change.
- **`docs/specs/`** — locked.
- **`docs/decisions/0001-*` through `0008-*`** — Accepted; amendments go in `0008-amendment-N.md` etc.
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock` (existing pins)** — `clap`, `clap_lex`, `half`, `indexmap`, `hashbrown` all stay.
- **PERF.md** — Phase 4A doesn't touch performance documentation. The kernel didn't change.
- **The `Diagnostic` struct shape** — adding codes is fine; struct fields are not. Phase 4A adds NO new codes.
- **MC3008** — permanently retired. No exceptions.
- **`crates/mc-model/examples/acme.yaml` and `acme.inputs.csv`** — these are the source of truth; the plugin example mirrors them, NOT the other way around.
- **Phase 4B Python adapters** — DO NOT START. The `mosaic-plugin/examples/adapters/` directory gets a placeholder README ONLY.
- **CLAUDE.md** — operating manual; not a Phase 4A deliverable.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

source $HOME/.cargo/env

# Pre-4A gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                               # 396 / 0
cargo run --release --bin mc -- demo
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml    # zero warnings
cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml    # 9/9 goldens

# Demo equivalence — must remain empty throughout (now AND for the plugin example copy)
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)
# expected: zero output

# After plugin example exists:
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model mosaic-plugin/examples/models/acme-marketing.yaml)
# expected: zero output

# After mc mcp ships — manual smoke check:
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | cargo run --release --bin mc -- mcp
# expected: valid JSON-RPC response listing 5 tools

# Iteration loop (Rust side):
cargo build -p mc-cli
cargo test -p mc-cli
cargo test -p mc-cli -- mcp_smoke
cargo test -p mc-cli -- example_byte_identity
cargo test -p mc-cli -- plugin_lint

# Reference plugin format check — read these for canonical Claude Code plugin shape:
ls /Users/edwinlovettiii/runtimescope/plugin/
cat /Users/edwinlovettiii/runtimescope/plugin/plugin.json | head -30
ls /Users/edwinlovettiii/runtimescope/plugin/skills/ 2>/dev/null || true
ls /Users/edwinlovettiii/runtimescope/plugin/agents/ 2>/dev/null || true
ls /Users/edwinlovettiii/runtimescope/plugin/commands/ 2>/dev/null || true

# Verify locked surfaces:
git diff phase-3d-friendly-formula-syntax -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/
# expected: zero output

# Determinism gate (10 runs, identical pass/fail):
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Plugin sanity lint (manual, before the test exists):
grep -rn "<anthropic_specific" mosaic-plugin/ || echo "ok: no anthropic_specific tags"
grep -rn "\[OpenAI:" mosaic-plugin/ || echo "ok: no OpenAI tags"
find mosaic-plugin/skills mosaic-plugin/agents mosaic-plugin/commands -type f \! -name "*.md" | grep -v README || echo "ok: skills/agents/commands are markdown-only"
```

---

## Final checklist before you call Phase 4A done

- [ ] `mosaic-plugin/` exists at workspace root with the full directory tree from scope item 1.
- [ ] `plugin.json` is valid per current Claude Code plugin spec.
- [ ] All 6 skill files exist with valid frontmatter + non-trivial body.
- [ ] All 4 agent files exist with valid frontmatter + non-trivial body.
- [ ] All 6 command files exist with valid frontmatter + non-trivial body. (`/mosaic-explain` is deferred to Phase 4A.2; do NOT ship a degraded version.)
- [ ] `.mcp.json` references `mc mcp`.
- [ ] Both hook files exist with the current Claude Code hook spec shape.
- [ ] `mosaic-plugin/examples/models/acme-marketing.yaml` is byte-identical to `crates/mc-model/examples/acme.yaml`.
- [ ] `mosaic-plugin/examples/models/acme-marketing.inputs.csv` is byte-identical to `crates/mc-model/examples/acme.inputs.csv`.
- [ ] `mosaic-plugin/examples/adapters/README.md` exists as a Phase 4B placeholder; the directory contains no Python or other adapter code.
- [ ] `crates/mc-cli/src/mcp.rs` exists implementing the MCP server (~150 lines, no new deps).
- [ ] `crates/mc-cli/src/main.rs` wires the `mcp` subcommand into clap; no other behavior change.
- [ ] `crates/mc-cli/tests/mcp_smoke.rs` passes (subprocess spawn + JSON-RPC roundtrip).
- [ ] `crates/mc-cli/tests/example_byte_identity.rs` passes (plugin example matches mc-model source).
- [ ] `crates/mc-cli/tests/plugin_lint.rs` passes (frontmatter check, no provider-specific tags).
- [ ] No `<anthropic_specific>` / `[OpenAI:` strings anywhere under `mosaic-plugin/skills/`, `agents/`, `commands/`.
- [ ] `marketing-mix` is the ONLY domain schema under `mosaic-plugin/skills/domain-schemas/`.
- [ ] No new dependencies in any Rust crate.
- [ ] `mc-core` Cargo.toml + src/ unchanged. `mc-fixtures` src/ + Cargo.toml unchanged. `mc-model` src/ + Cargo.toml unchanged.
- [ ] `rust-toolchain.toml` not bumped. Cargo.lock pins intact.
- [ ] No `unsafe`. No `async` / `tokio` / `rayon` / threads.
- [ ] All 396 existing tests still pass; new total ≥ 396 + Phase 4A additions.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] MC3008 still retired; no new diagnostic codes added in Phase 4A.
- [ ] JSON envelope `schema_version` stays at `"1.0"`.
- [ ] The `mc demo --model ...` plugin-example diff is empty (proves byte-identity round-trips through the kernel).
- [ ] **End-to-end fresh-instance proof:** transcript captured in completion report showing `/mosaic-init marketing-mix` → working YAML → validate/lint/test all green.
- [ ] Completion report at `docs/reports/phase-4a-completion-report.md` written from template.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 4A from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 4B (Python adapters), Phase 5 (actuals), or Phase 6 (UI).**
- [ ] **You did NOT modify ADR-0008 or any earlier ADR.**
- [ ] **You did NOT add a second domain schema** (FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning are all out of scope per ADR-0008 amendment F).

If you are uncertain at any point, the resolution order is:

1. The Phase 4A prompt above.
2. **ADR-0008** — the strategic gate. Read the "Strategic centerpiece" + Decisions 1–11 + the 9 acceptance amendments (A–I).
3. ADR-0004 / ADR-0005 / ADR-0006 / ADR-0007 — inherited contracts.
4. The current Claude Code plugin spec + current MCP protocol spec (cross-referenced at execution time).
5. The reference plugin at `/Users/edwinlovettiii/runtimescope/plugin/`.
6. `crates/mc-model/examples/acme.yaml` and the Phase 3A/3B/3C/3D completion reports.
7. The brief and `engine-semantics.md`.
8. `CLAUDE.md` (operating manual).
9. `docs/strategy/POSITIONING.md` (Mosaic-as-LNM framing).
10. `docs/process-notes.md` (carry-forward rules).
11. `docs/roadmap/MASTER_PHASE_PLAN.md`.
12. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 3A / 3B / 3C / 3D)

**Read this handoff fully + read ADR-0008 fully before writing any code (or any markdown).** Phase 4A's contract is the body of this handoff; ADR-0008 is the strategic gate behind it. The "Strategic centerpiece" section of ADR-0008 is the load-bearing single paragraph.

**The plugin is the deliverable. The runtime is the wiring.** Spend your time on the markdown. Spend less time on the `mc mcp` subcommand. If you find yourself optimizing the JSON-RPC framing for elegance instead of writing the next skill file — switch back. The skill quality is what makes the acceptance gate pass.

**Source-bounded.** Phase 4A touches `mosaic-plugin/` (NEW) and `crates/mc-cli/src/` (only the new `mcp.rs` + a clap wiring line in `main.rs`). It does NOT touch the kernel, fixtures, or model layer. The 0-line diff against `phase-3d-friendly-formula-syntax` for `crates/mc-core/`, `crates/mc-fixtures/`, and `crates/mc-model/` is a hard gate.

**The acceptance gate is "fresh Claude Code instance produces working YAML."** This is the single load-bearing test. Every other gate (locked surfaces, build/clippy/fmt, MCP smoke test, byte-identity, plugin lint) is a precondition. If the fresh-instance proof fails — the LLM produces YAML that doesn't pass validate/lint/test — Phase 4A doesn't ship until the skills/agents teach the LLM well enough that it does.

**Diagnostic codes are forever.** MC1xxx–MC4xxx semantics are stable. The plugin's `skills/debugging/SKILL.md` documents the registry as it stands at Phase 3D; Phase 4A adds no new codes. If a new code is genuinely needed for Phase 4A scope, that's a SPEC QUESTION (it would mean modifying `mc-model`, which is locked).

**Markdown-only in skills/agents/commands.** No code. No executables. No provider-specific tags. The portability rule (ADR-0008 Decision 4) is binding — it's what makes Phase 4B (Python adapters) work, and it's what makes future Codex / OpenAI / TypeScript adapters work without rewriting the plugin.

**Marketing-mix is the only domain.** Per ADR-0008 amendment F. If a skill or agent body would benefit from a second domain example (e.g., to illustrate "schemas can have different shapes"), use a hypothetical / sketched example, NOT a full `domain-schemas/<other>/SKILL.md` file. Adding a second domain folder is out of scope.

**Hand-rolled wins.** No `serde_json`, no MCP SDK crate, no JSON-RPC framework. The `mc mcp` subcommand is ~150 lines of straightforward serialization. Pulling in a dep would add toolchain risk + maintenance burden.

**Do not pick the next phase.** Phase 4A's deliverable is the plugin + the `mc mcp` subcommand. If the work surfaces opportunities for Phase 4B (Python adapters), Phase 5 (actuals), or Phase 6 (UI), note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 4A handoff drafted 2026-05-03 immediately after [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) was Accepted (same day, after parallel GPT + Claude Desktop reviews converged on the major restructure: drop the Rust LLM client crate; Phase 4B = Python reference adapters under `mosaic-plugin/examples/adapters/`; no Phase 4C; marketing-mix only). Phase 4A returns to ADR-first sequencing (per [`../process-notes.md`](../process-notes.md) §1) — ADR-0008 was Accepted before this handoff was drafted, unlike the brief Phase 3D handoff-first parallel experiment. Phase 4A is large and architecturally novel enough that the strategic alignment a Proposed → Accepted ADR cycle forces is load-bearing.*
