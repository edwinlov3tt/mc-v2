# ADR-0008: Phase 4 — LLM-Assisted Authoring + Mosaic Plugin Ecosystem

**Status:** Accepted (with project-owner amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-03 (Proposed); 2026-05-03 (Accepted, same day after GPT + Desktop reviews)
**Deciders:** project owner
**Phase:** 4 precondition (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3D shipped at `d5ab355` (tag `phase-3d-friendly-formula-syntax`), giving the project the friendly-formula authoring layer that closes the last major UX gap before LLM authoring. Phase 4 is the next phase. **Per [`../process-notes.md`](../process-notes.md) §1's self-test, Phase 4 returns to the ADR-first flow** — it fails questions 1–3 (new deps on LLM providers, contract changes via the plugin schema, kernel-adjacent in spirit if not in code) and is too large for handoff-first.

---

## Strategic centerpiece (read this first)

**The Mosaic plugin is the institutional knowledge of Mosaic, in a form any agent framework — present or future — can consume. This is the actual moat. Not the runtime, not the prompts, not the SDK adapters.**

The plugin is a structured package (skills as markdown, agents as system prompts, commands as CLI invocations, examples as working YAML) that teaches an AI agent everything it needs to know to author a Mosaic model: the schema, the diagnostic-code registry, the formula grammar, the validator/linter behavior, the test-fixture pattern, and the canonical Acme reference. Loading the plugin into any agent — Claude Code via native plugin install, Anthropic SDK loop reading the plugin's markdown, OpenAI SDK loop with the same content, future providers via the same pattern — gives that agent Mosaic-authoring competence.

**Future implementers reading this ADR will optimize for the wrong thing if they think the runtime is the deliverable.** The runtime is the *vehicle*; the knowledge is the *cargo*. Consequently, **Phase 4 ships the cargo (Phase 4A: the plugin) plus minimal vehicle proofs (Phase 4B: Python reference adapters in `mosaic-plugin/examples/adapters/`).** It does NOT ship a Rust LLM client crate. Per the GPT + Desktop reviews:

- The "not locked in" goal is achieved by the plugin (4A) plus the diagnostic envelope contract (Phase 3B). A Rust LLM-client crate adds NO incremental lock-in protection.
- Plugin content (markdown + JSON + YAML) ages on a ~10+ year half-life. A Rust LLM client ages on ~3-year half-life (provider API churn, SDK breaking changes, tokio major versions).
- Users who want CLI / programmatic access run the Python reference adapters or use one of the ~30 existing ecosystems (Claude Code, Anthropic SDK, OpenAI SDK, langchain, instructor, crewai, goose, aider, cursor, cline, continue.dev, etc.).

The plugin-is-the-moat framing is the load-bearing decision in this ADR. Everything else flows from it.

---

## Context

Phases 3A → 3D built the deterministic foundation: YAML model authoring (3A), validation + diagnostics (3B), test fixtures (3C), friendly formula syntax (3D). Mosaic now has a complete, testable, lint-clean, formula-friendly authoring path. **Every piece an LLM needs to emit a working Mosaic model exists** — schema, validator, diagnostic codes (MC1xxx–MC4xxx with stable meanings), golden tests, formula syntax.

Phase 4's job is to **close the loop**: the LLM emits YAML; the system runs validate/lint/test; structured diagnostics feed back to the LLM; the LLM iterates; success means a working Mosaic model.

**Three things make Phase 4 substantially bigger than prior phases:**

1. **External provider deps.** Phase 4 introduces dependencies on Anthropic / OpenAI / Codex SDKs. These are the first runtime deps outside the workspace's local crates since Phase 1A. The provider-abstraction strategy matters from day one.

2. **The plugin ecosystem (NEW major concept).** Mosaic-authoring knowledge needs to be packaged so it's portable across LLM environments. The natural shape is a **Claude Code plugin** (skills + agents + commands + MCP server + hooks), which doubles as the source-of-truth knowledge package that SDK adapters can also consume. This is genuinely new architectural surface — not an extension of existing Phase 3 work.

3. **The "not locked to one model" constraint.** Project-owner direction (2026-05-03): the system must support Claude API, OpenAI, Codex, and future providers. **The plugin is the abstraction.** The structured knowledge it holds (skills as markdown, agents as system prompts + tool-use specs, commands as CLI invocations, MCP server surfaces as tool calls) is provider-agnostic; provider-specific adapters translate it into the right SDK calls.

**The strategic insight:** the Mosaic plugin is the artifact that makes "any AI agent can build Mosaic models" true. It is not a downstream consumer of Phase 4 — it IS Phase 4's load-bearing deliverable. Multi-provider runtime is then a translator over the plugin's structured knowledge.

This ADR proposes Phase 4 as a multi-deliverable phase with explicit sub-phase decomposition (4A/4B/4C). It commits the strategic shape; the sub-phase handoffs commit the implementation contracts.

---

## Decisions needed

The 11 decisions below are listed in dependency order — answering #1 informs #2, etc.

### Decision 1: what does Phase 4 deliver?

**Question:** What does the user observe at Phase 4 exit?

**My recommendation:** Phase 4 ships when **all** of the following hold:

1. A user (human or programmatic) gives Mosaic a natural-language description of a planning model — e.g., *"Build me a marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"* — and Mosaic produces a YAML model file that:
   - Passes `mc model validate` (no MC1xxx / MC2xxx errors).
   - Passes `mc model lint` (zero MC3xxx warnings, OR documented intentional warnings explained by the user).
   - Passes `mc model test` against either user-provided or LLM-generated golden tests.
   - Compiles to a working `mc_core::Cube` that the user can run via `mc demo --model`.

2. The Mosaic Claude Code plugin (Phase 4A deliverable) is installable, gives any Claude Code instance the skills/agents/commands/MCP server needed to author Mosaic models, and the Acme model can be re-authored from a natural-language prompt via the plugin in a single `/mosaic-init` invocation.

3. **At least two providers** are demonstrably wired (Phase 4B): Claude API (Anthropic SDK) and OpenAI (OpenAI SDK). The same structured prompts produce equivalent Mosaic models regardless of provider. Codex / future providers are documented as a Phase 4C extension following the same adapter pattern.

4. The iteration loop is observable: an `mc author` (or equivalent) CLI subcommand emits the LLM's YAML proposals, runs validate/lint/test, feeds structured diagnostics back to the LLM, and converges (or surfaces a SPEC QUESTION-style failure if it can't) within N iterations. N is tunable; recommended default 5.

5. **`mc-core` and `mc-fixtures` remain LOCKED.** Same guarantee as Phases 3A–3D. The kernel does not change for LLM authoring.

**Why all five together:** any subset misses the point. Just shipping a plugin without runtime is "we have a manual but no machinery." Just shipping the runtime without the plugin is "we hardcoded the prompts; portability breaks the moment you change provider." Just supporting one provider violates the explicit "not locked in" direction.

### Decision 2: multi-provider strategy

**Question:** How does Mosaic stay "not locked in" to any single LLM provider?

**My recommendation:** **The plugin is the abstraction.** Provider-specific adapters translate the plugin's structured knowledge into provider-specific API calls. Concretely:

```
                     ┌─────────────────────────────────┐
                     │   Mosaic plugin (the source)    │
                     │   - skills/                     │
                     │   - agents/                     │
                     │   - commands/                   │
                     │   - .mcp.json                   │
                     │   - examples/                   │
                     └────────────┬────────────────────┘
                                  │ (structured knowledge)
              ┌───────────────────┼────────────────────┐
              │                   │                    │
    ┌─────────▼────────┐  ┌───────▼─────────┐  ┌──────▼──────────┐
    │ Claude Code      │  │  Anthropic SDK   │  │ OpenAI / Codex  │
    │ (loads plugin    │  │  adapter         │  │ adapter         │
    │  natively)       │  │  (mc-author)     │  │ (mc-author)     │
    └──────────────────┘  └─────────────────┘  └─────────────────┘
                                  │                    │
                          ┌───────┴────────────────────┴───────┐
                          │   mc-cli / mc-model (validate /     │
                          │   lint / test / inspect)            │
                          └──────────────────────────────────────┘
                                              │
                                  ┌───────────▼──────────┐
                                  │  mc-core (LOCKED)    │
                                  └──────────────────────┘
```

**The contract:** the plugin's `skills/`, `agents/`, and `commands/` are markdown / structured files. They describe Mosaic-authoring knowledge in a provider-agnostic way (no `<anthropic_specific_tag>` or `[OpenAI: ...]` annotations in the source). Provider adapters in `mc-author` (Phase 4B) read these files and translate them into provider-specific API calls (system prompts, tool definitions, function-calling specs).

**Why this works:**

- Skills as markdown are LLM-readable across all providers.
- Agents as system prompts + structured tool-use specs translate cleanly to Anthropic's tool use, OpenAI's function calling, or Codex's similar primitives.
- Commands as CLI invocations are provider-agnostic at execution time (any provider's response can produce the same `mc model validate <path>` shell command).
- The MCP server is Claude-Code-specific by protocol but exposes the same underlying `mc-cli` verbs that adapters call directly — no functional difference, just transport.

**Hard rule:** no provider-specific content in the plugin's `skills/`, `agents/`, or `commands/` directories. Provider-specific code lives in `mc-author`'s adapter modules.

### Decision 3: the Mosaic Claude Code plugin (Phase 4A)

**Question:** What's in the Mosaic plugin?

**My recommendation:** Mirror Claude Code's plugin spec. Proposed structure:

```
mosaic-plugin/                          # ships as a separate top-level dir or repo
├── plugin.json                         # manifest (name, version, description)
├── README.md                           # what this plugin does + install instructions
│
├── skills/                             # progressive-disclosure knowledge
│   ├── authoring/
│   │   └── SKILL.md                    # "How to write a Mosaic YAML model"
│   ├── debugging/
│   │   └── SKILL.md                    # "How to read MC1xxx–MC4xxx diagnostics"
│   ├── schema-design/
│   │   └── SKILL.md                    # "How to design dims, hierarchies, measures, rules"
│   ├── formulas/
│   │   └── SKILL.md                    # "Phase 3D formula syntax — the friendlier rule body form"
│   ├── testing/
│   │   └── SKILL.md                    # "How to write canonical_inputs + golden tests"
│   └── domain-schemas/
│       └── marketing-mix/              # Phase 4A ships ONLY this domain schema (per amendment F)
│           └── SKILL.md                # Marketing-mix schema patterns (Acme is the reference)
│
├── agents/                             # autonomous specialists
│   ├── mosaic-architect.md             # designs schemas from natural-language requirements
│   ├── mosaic-author.md                # writes YAML from a finalized schema design
│   ├── mosaic-debugger.md              # parses diagnostic JSON envelopes + proposes fixes
│   └── mosaic-validator.md             # runs validate/lint/test cycles + reports
│
├── commands/                           # slash-command shortcuts
│   ├── mosaic-init.md                  # /mosaic-init <domain> — scaffold a new model
│   ├── mosaic-validate.md              # /mosaic-validate [path]
│   ├── mosaic-inspect.md               # /mosaic-inspect [path]
│   ├── mosaic-lint.md                  # /mosaic-lint [path]
│   ├── mosaic-test.md                  # /mosaic-test [path]
│   ├── mosaic-explain.md               # /mosaic-explain <coord> — trace a computed value
│   └── mosaic-author.md                # /mosaic-author "natural language description"
│
├── .mcp.json                           # MCP server config (surfaces mc-cli verbs as tool calls)
│
├── hooks/                              # event-driven automation
│   ├── pre-commit-lint.json            # pre-commit hook: run mc model lint on YAML changes
│   └── post-edit-validate.json         # after edit: run mc model validate
│
├── examples/                           # canonical example models + reference adapters (Phase 4B)
│   ├── models/
│   │   ├── acme-marketing.yaml         # Phase 4A example (mirrors crates/mc-model/examples/acme.yaml)
│   │   └── acme-marketing.inputs.csv
│   └── adapters/                       # Phase 4B deliverable
│       ├── anthropic-python/           # ~150 line reference iteration loop (Anthropic SDK)
│       │   ├── README.md
│       │   ├── pyproject.toml
│       │   └── author.py
│       └── openai-python/              # ~150 line reference iteration loop (OpenAI SDK)
│           ├── README.md
│           ├── pyproject.toml
│           └── author.py
```

**Why this shape:**

- **Skills** are Claude Code's progressive-disclosure mechanism. A user asks "how do I write a Mosaic rule?"; the relevant skill loads automatically with the right context. No manual skill invocation needed.
- **Agents** are the specialists for distinct authoring tasks — schema design, code authoring, error debugging, validation. Each has its own system prompt + trigger conditions; Claude Code routes to the right one based on user intent.
- **Commands** are user-typed shortcuts (`/mosaic-init`, `/mosaic-validate`) for the most common operations. They invoke `mc-cli` with the right args.
- **MCP server** lets Claude Code call `mc-cli` directly as tool calls — no shell-out needed; the agent reasons over the diagnostic output structurally.
- **Hooks** automate the boring stuff (pre-commit lint, post-edit validate) so the user doesn't have to remember to run them.
- **Examples** let the LLM see real working models. The Acme model is the canonical reference; future schema families add their own examples.

**Plugin location:** the plugin lives at `mosaic-plugin/` (sibling to `crates/`) in the Mosaic repo for now. Once stable, it can be extracted to its own repo for distribution via the Claude Code marketplace. Mosaic's main repo holds the source-of-truth version; downstream distributions are clones.

### Decision 4: the plugin as portable knowledge package

**Question:** How is the plugin's content reusable outside Claude Code (i.e., from raw Anthropic SDK / OpenAI SDK / Codex)?

**My recommendation:** **The plugin's content is structured markdown + JSON.** Provider adapters in `mc-author` (Phase 4B) read the same files Claude Code reads and translate them into provider-specific API calls.

Concretely:

| Plugin asset | Claude Code uses it as | Adapter uses it as |
|---|---|---|
| `skills/<name>/SKILL.md` | Auto-loaded skill (progressive disclosure) | System prompt prefix when the topic comes up; cached and inserted into context |
| `agents/<name>.md` | Autonomous agent invocation | System prompt for a tool-use loop; the agent's described "when to use" → adapter's routing logic |
| `commands/<name>.md` | Slash command | CLI shortcut + LLM-readable description of what it does (the LLM can decide when to invoke) |
| `.mcp.json` | Native MCP server connection | Adapter calls `mc-cli` directly, doesn't need MCP transport |
| `hooks/<name>.json` | Native hook execution | Adapter has its own pre/post-step hooks; same JSON schema |
| `examples/<file>` | Reference material in skill context | Same — fed as in-context examples for few-shot learning |

**The portability rule (binding):**

- Plugin content is **markdown + JSON only**. No code, no executables, no provider-specific tags.
- Plugin content is **describable in plain prose**. A skill that says "to write a rule body, use formula syntax like `Spend / CPC` per Phase 3D" works whether the consuming LLM is Claude, GPT-4, or anything else.
- **Stable diagnostic codes** (MC1xxx–MC4xxx, established Phase 3B onward) are the cross-provider error vocabulary. Every adapter feeds back the same codes; every LLM iterates against the same registry.

**The "knowledge embuing" pattern:** loading the plugin into a fresh AI agent (Claude Code on first install, OR an Anthropic SDK loop on first invocation, OR an OpenAI agent at startup, OR a Codex session) gives it the complete authoring competence — what a Mosaic model is, how to write one, how to read errors, how to fix them, what each domain schema looks like.

This is the "instruction manual" model. The plugin IS the instruction manual; the LLM IS the assembler. Different brands of assemblers can read the same manual.

### Decision 5: NO new Rust crate for Phase 4 (acceptance amendment A)

**Question:** Where does the LLM scaffolding code live?

**Decision (Accepted, per acceptance amendment A from both GPT + Desktop reviews):** **No new Rust crate.** Phase 4 ships the plugin (4A) and Python reference adapters in `mosaic-plugin/examples/adapters/` (4B). The Rust workspace stays at 4 crates:

```
crates/
├── mc-core/        # LOCKED kernel
├── mc-fixtures/    # LOCKED Rust-side reference
├── mc-model/       # YAML + validate + lint + inspect + test + formula  (LOCKED for Phase 4)
└── mc-cli/         # CLI verbs (mc demo, mc model {validate,inspect,lint,test})  (LOCKED for Phase 4 except for the new `mc mcp` MCP-server subcommand needed by the plugin)
```

**Phase 4B's reference adapters live in:**

```
mosaic-plugin/
├── examples/
│   └── adapters/
│       ├── anthropic-python/   # ~150 line reference iteration loop
│       │   ├── README.md
│       │   ├── pyproject.toml
│       │   └── author.py
│       └── openai-python/      # ~150 line reference iteration loop
│           ├── README.md
│           ├── pyproject.toml
│           └── author.py
```

Each Python adapter is a complete working example: reads the plugin's `skills/`, `agents/`, `commands/`, and `examples/`; calls the provider's API; runs the iteration loop against `mc-cli`'s diagnostic JSON envelope; produces a working Mosaic YAML.

**Why Python (not Rust):**

- Plugin content (markdown + JSON + YAML) ages on a ~10+ year half-life. Rust LLM clients age on ~3-year half-life (provider API churn, SDK breaking changes, tokio major versions).
- Python is the lingua franca of LLM tooling — Anthropic and OpenAI both ship official Python SDKs that are kept current; community ecosystems (langchain, instructor, etc.) target Python first.
- Users who want CLI access run `python -m mosaic_author.anthropic` (or similar) from the example adapter. CI / scripts / non-interactive contexts use the same.
- The Rust workspace stays sync + dep-bounded. **No tokio. No async. No reqwest. No HTTP deps.** No toolchain bump required.

**The aesthetic argument for `mc author` as a CLI verb alongside `mc demo` and `mc model {validate, lint, test, inspect}` doesn't survive cost-benefit analysis.** The plugin + Python adapters cover the same use cases at a fraction of the maintenance burden.

**The single small Rust change in Phase 4A:** `mc-cli` gains a new `mc mcp` subcommand that runs the standard MCP server protocol over stdio and dispatches tool calls to the existing `mc model {validate, inspect, lint, test}` implementations. This is ~150 lines and has zero new external deps (the MCP-stdio protocol is JSON-RPC over stdin/stdout — implementable with the existing hand-rolled JSON serialization Phase 3B established).

**Alternate route flagged in acceptance amendment A:** if the project owner has a non-aesthetic reason to need a Rust LLM client later (e.g., shipping `mc-author` as an embedded library to a Rust customer with a contractual no-Python requirement), that's a future phase — built as a feature-flagged optional workspace member that does NOT default-build, does NOT bump the workspace toolchain, and does NOT relax the locked-surfaces guarantee. The default `cargo build --workspace` stays at Rust 1.78, sync, no async runtime. Phase 4 is NOT that phase.

### Decision 6: the iteration loop

**Question:** How does the LLM consume diagnostic feedback?

**My recommendation:** consume Phase 3B's stable JSON envelope (`{schema_version: "1.0", diagnostics: [...]}`) directly. Diagnostics carry stable codes (MC1xxx–MC4xxx), severity, model_path (human-friendly), and suggestion. The LLM iterates against the structured feedback, not free-form error strings.

**Loop shape:**

```
1. User prompt → Mosaic plugin's mosaic-architect agent designs a schema
2. mosaic-author agent emits YAML
3. mc model validate <yaml> --format json → diagnostic envelope
4. If errors:
     mosaic-debugger agent reads diagnostics by code; proposes specific fixes
     loop (max N iterations, default 5)
5. If validate passes → mc model lint <yaml> --format json
6. If warnings the user wants fixed → mosaic-debugger fixes them
7. mc model test <yaml> → if goldens fail and user provided expected values, mosaic-debugger reconciles
8. SUCCESS → present final YAML to user for review
```

**Convergence criteria:**

- All errors fixed (validate exits 0 with no MC1xxx/MC2xxx codes).
- Lint warnings either fixed OR documented (user can opt-out per warning).
- Goldens pass (if any goldens declared).

**Failure criteria:**

- Same error code repeats N+1 times → declare "LLM cannot resolve MC<code>; manual intervention needed" (this is a SPEC-QUESTION-shaped escalation, but for end users not Claude Code).
- User cancels.
- Token budget exceeded (provider-specific).

**The diagnostic JSON envelope is the contract.** Phase 4 does NOT modify Phase 3B's `Diagnostic` struct shape (per ADR-0006 amendment #20 + ADR-0007). New diagnostic codes (if any) extend the existing namespace; the envelope shape stays at `schema_version: "1.0"`.

### Decision 7: sub-phase decomposition (acceptance amendment B — Phase 4C dissolved)

**Question:** How is Phase 4 broken into shippable increments?

**Decision (Accepted, per acceptance amendment B from both reviews — Phase 4C dissolved per process-notes "no vague TBD buckets" rule):** Two sub-phases, each with its own handoff + completion report:

| Sub-phase | Deliverable | Acceptance gate |
|---|---|---|
| **Phase 4A — Mosaic Plugin** | The Claude Code plugin: skills, agents, commands, MCP server, hooks, examples. **One domain schema only: marketing-mix (Acme).** `mc-cli` gains `mc mcp` subcommand for the MCP server. | Fresh Claude Code instance with plugin installed produces a YAML for an Acme-shaped marketing-mix model that passes validate/lint/test. |
| **Phase 4B — Python reference adapters** | Two Python adapters under `mosaic-plugin/examples/adapters/`: `anthropic-python/` and `openai-python/`. Each ~150 lines. Each consumes the plugin's `skills/`, `agents/`, `commands/`, `examples/` and runs the iteration loop against `mc-cli`'s diagnostic JSON envelope. | `python examples/adapters/anthropic-python/author.py "marketing-mix for 5-channel B2C SaaS"` AND `python examples/adapters/openai-python/author.py "..."` both produce a YAML that passes `mc model validate/lint/test`. |

**No Phase 4C.** Per process-notes §"no vague TBD buckets" + GPT review point 8 + Desktop refinement B: Phase 4C is dissolved. After Phase 4B ships, the next phase is **Phase 5 (actuals import)**. Future additions — TypeScript adapters, Codex/Gemini/Ollama adapters, additional domain schemas (FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning), schema marketplace, cost tracking, prompt hardening — are each their own demand-driven phases (not pre-named in the roadmap; named when a real customer/proof requires them).

**Why this decomposition:**

- **4A first** because the plugin is the source of truth; the Python adapters in 4B consume what 4A produces. Building 4B without 4A means hardcoding prompts in Python, which violates the plugin-as-portable-knowledge rule (Decision 4).
- **4B second** because once the plugin exists, building the Python adapters is mostly translation work (read plugin's content, call provider API, run iteration loop). The hard work — the *knowledge* — is in 4A.
- **No 4C** because pre-naming a vague follow-on phase is how 4C grows into 4D, 4E, 4F. If the next concrete need is "production polish on the Python adapters" → that's a Phase 4B amendment / 4B.1 mini-phase. If the next concrete need is "a TypeScript adapter for a customer who runs Node" → that's its own phase tied to that customer's demand.

Each sub-phase gets its own ADR under this Phase 4 umbrella (Phase 4A handoff lands at this ADR's Acceptance; Phase 4B handoff lands separately after 4A ships).

### Decision 8: out of scope for Phase 4

**Question:** What is *not* Phase 4?

**My recommendation:** the following are out of scope. Each is named here so the implementer can't rationalize "while we're at it":

| Out of scope | Phase / disposition |
|---|---|
| **Real-world actuals import (CSV / API)** | Phase 5 — separate concern; LLM authoring is for *building* models, actuals are for *populating* them with real data |
| **DuckDB / external storage** | Phase 5+ |
| **UI editor** | Phase 6 — Phase 4 is CLI + Claude Code plugin only |
| **Model-backed cells / probabilistic forecasting** | Phase 6B+ research track per POSITIONING.md "moat-but-trap" rule |
| **Custom LLM training / fine-tuning** | Out of scope indefinitely. Mosaic uses off-the-shelf provider models; it does not train them. |
| **A Rust LLM client crate (`mc-author`)** | Per acceptance amendment A (Decision 5). If a future demand-driven need surfaces, that's a separate phase with feature-flag scaffolding; Phase 4 does NOT build it. |
| **Anthropic / OpenAI / any SDK in the Rust workspace** | Per amendment A. SDKs are consumed from Python in `mosaic-plugin/examples/adapters/` only. |
| **`tokio`, `async`, `reqwest`, `hyper`, HTTP deps in the Rust workspace** | Per amendment A. The Rust workspace stays sync. |
| **Rust toolchain bump** | Per Decision 11 amendment. Stays at Rust 1.78. ADR-0009 NOT triggered by Phase 4. |
| **TypeScript adapters** | Per GPT point 6. Future demand-driven phase if a real customer needs it. |
| **Codex / Gemini / Mistral / Ollama / other-provider adapters** | Per GPT point 6 + Desktop F. Future demand-driven phases. |
| **Cost tracking / token budget telemetry** | Per GPT point 6. Demand-driven follow-on. |
| **Prompt-injection / adversarial defenses beyond schema validation** | Per GPT point 6. Phase 4 relies on the validator + lint to catch malformed LLM output. Adversarial-prompt-injection hardening is a separate concern (probably Phase 6/7 alongside auth). |
| **Schema marketplace v0** | Per GPT point 6. Future phase tied to multi-customer demand. |
| **Additional domain schemas in Phase 4A (FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning)** | Per GPT point 7 + Desktop F. Phase 4A ships marketing-mix (Acme) ONLY. Each additional schema is its own demand-driven phase (Phase 4D, 4E, etc., not pre-named). |
| **`mc-core` changes** | LOCKED. Phase 4 doesn't touch the kernel. |
| **`mc-fixtures` changes** | LOCKED. Same. |
| **`mc-model` changes** | LOCKED for Phase 4. Phase 4 emits against the Phase 3D schema; doesn't extend it. |
| **`mc-cli` changes (except the new `mc mcp` MCP-server subcommand)** | LOCKED for Phase 4 except for the new `mc mcp` subcommand needed to surface mc-cli verbs to Claude Code's MCP integration. Estimated ~150 lines, no new deps. |
| **Modifying the `Diagnostic` struct shape** | Per ADR-0006 amendment #20: adding codes is fine; struct fields require `schema_version` bump. Phase 4 may add MC4xxx codes (the reserved namespace) for LLM-specific concerns, but doesn't change the shape. |
| **Schema changes to the YAML model format** | Phase 3A schema + Phase 3C additions + Phase 3D formula syntax are the contract. Phase 4 emits against this contract; it does not modify it. |
| **A "Mosaic Cloud" hosted authoring service** | Phase 7+ if ever. Phase 4 is library + CLI + plugin only. |
| **Prompt-injection defenses beyond schema validation** | Phase 4 relies on the validator + lint to catch malformed LLM output. Adversarial-prompt-injection hardening is a separate concern (probably Phase 6/7 alongside auth). |
| **Multi-cube models / cross-cube references** | Per ADR-0004 Decision 5 — future Phase 3 sub-phase if ever. |
| **Auto-fix beyond LLM iteration** | The `mc model fix` CLI from Phase 3B's deferred list stays deferred. Phase 4's iteration loop is fix-via-LLM, not deterministic fix. |

**Hard rule:** no source change in `crates/mc-core/`, `crates/mc-fixtures/src/`, `crates/mc-model/src/`, or `crates/mc-cli/src/main.rs` (modulo wiring `mc author` as a new subcommand). The new code is in `crates/mc-author/` + `mosaic-plugin/`.

### Decision 9: where the prompts live

**Question:** Are LLM prompts in the plugin (markdown) or in `mc-author` (Rust string constants)?

**My recommendation: in the plugin (markdown).** Per Decision 4's portability rule, the plugin is the source of truth. Adapters in `mc-author` read the markdown and translate it.

**Why this matters:**

- Plugin content can be edited without recompiling Rust.
- Plugin content can be updated independently of the Rust crate version (e.g., refining a skill's prompt based on real-world feedback doesn't require a `cargo publish`).
- Plugin content is reviewable as documentation — humans can read `skills/authoring/SKILL.md` and understand what the LLM is being told. Prompts buried in Rust string constants are opaque to non-Rust readers.
- Plugin content is testable independently — a skill can be evaluated on its own (does it teach the right thing?) without spinning up the whole Rust crate.

**The exception:** any *binding contract* between the plugin and the runtime — e.g., the structure of a tool-use response, the JSON schema for a diagnostic-feedback message — lives as a Rust type in `mc-author` AND as a documented schema in the plugin. The Rust type is the canonical machine-readable contract; the plugin doc is the human-readable explanation. Drift between them is a CI failure (runtime tests assert the plugin's claimed schema matches the Rust type).

### Decision 10: what's the kernel's role in Phase 4?

**Question:** Does the kernel need any awareness of LLM authoring?

**My recommendation: no.** The kernel is locked.

**Why this is non-trivial to defend:** there will be temptation to add a "model author metadata" field to `mc_core::Cube` (e.g., "this cube was authored by Claude" or "iteration count: 3"). Resist. The kernel doesn't care how a cube was authored; it just runs cubes.

LLM-authoring metadata (provider used, model name, iteration count, total tokens, cost) lives in:

1. The model file's `metadata` block (which already accepts free-form fields like `author`, `created`, etc.) — *if* the user opts in.
2. `mc-author`'s output logs / report files — for runtime concerns like cost tracking.

But not in `mc-core`. The kernel is a math engine; LLM-provenance is an authoring-layer concern.

### Decision 11: toolchain implications (acceptance amendment — no bump triggered)

**Question:** Does Phase 4 require a Rust toolchain bump?

**Decision (Accepted, per acceptance amendments A + GPT point 3): No. Phase 4 does NOT trigger a Rust toolchain bump.**

**Why no bump:**

- No new Rust crate (Decision 5 amendment).
- No Anthropic / OpenAI SDK in the Rust workspace (the SDKs are consumed from Python in `mosaic-plugin/examples/adapters/`).
- No tokio / async / reqwest / hyper / HTTP deps added.
- The single Rust addition — `mc mcp` subcommand in `mc-cli` for the MCP server — uses the existing hand-rolled JSON serialization established in Phase 3B (no `serde_json` added). MCP-stdio is JSON-RPC over stdin/stdout; trivially implementable in stable Rust 1.78.

**`rust-toolchain.toml` stays pinned at 1.78. `Cargo.lock` Phase 1B + Phase 3A pins (clap → 4.4.18, clap_lex → 0.6.0, half → 2.4.1, indexmap → 2.7.0, hashbrown → 0.15.5) all stay intact.**

**The long-anticipated toolchain ADR** (formerly tracked as "ADR-0005 equivalent" before that slot was used by Phase 3B) is **NOT triggered by Phase 4**. It may still trigger in a future phase for unrelated reasons (e.g., a future Rust crate added for a different concern), but Phase 4 is not the trigger.

---

## The Mosaic plugin in detail

Because the plugin ecosystem is the major new architectural concept, this section drills in more.

### Plugin manifest (`plugin.json`)

```json
{
  "name": "mosaic",
  "displayName": "Mosaic — Large Numbers Model authoring",
  "version": "0.1.0",
  "description": "Author, validate, lint, test, and inspect Mosaic YAML model files. Mosaic is a multidimensional engine for building large numerical models across finance, marketing, prospecting, sports betting, sales forecasting, and analytics.",
  "author": "Mosaic project",
  "license": "MIT OR Apache-2.0",
  "repository": "https://github.com/edwinlov3tt/mc-v2",
  "skills": "./skills",
  "agents": "./agents",
  "commands": "./commands",
  "mcpServers": "./.mcp.json",
  "hooks": "./hooks"
}
```

The schema follows Claude Code's plugin manifest format. (Exact field names + structure to be confirmed against current Claude Code plugin spec at handoff time.)

### A skill example: `skills/formulas/SKILL.md`

```markdown
---
name: mosaic-formula-syntax
description: How to author Mosaic rule bodies using formula syntax (Phase 3D). Use whenever a user asks how to write a rule, what operators are supported, or how to translate between structured-tree and formula form.
trigger_keywords: ["formula", "rule body", "mc model", "Spend / CPC", "if_null"]
---

# Mosaic Formula Syntax

Mosaic rule bodies can be authored in two forms — both produce identical cube
behavior; choose whichever is more readable for the rule.

## Formula form (recommended for human authors)

```yaml
- target_measure: Revenue
  body: "Customers * AOV"
```

## Structured form (verbose; useful when generating programmatically)

```yaml
- target_measure: Revenue
  body:
    mul:
      - { ref: "Customers" }
      - { ref: "AOV" }
```

## Operators supported

- `+`, `-`, `*`, `/`, parentheses, unary `+` / `-`
- `if_null(primary, fallback)` — fallback when the primary is null

## NOT supported (don't use these — they'll fire MC1004)

- `min`, `max`, `if`, `==`, `<`, `>`, conditional expressions, string/bool literals
- Cross-cube references (`DB(...)`)

If you need an operator that's not supported, the structured tree doesn't have
it either — that's a Phase 3E or later concern.

## Example: Acme's 5 rules

```yaml
- body: "Spend / CPC"
- body: "Clicks * CVR"
- body: "Leads * Close_Rate"
- body: "Customers * AOV"
- body: "Revenue * (1 - COGS_Rate)"
```

## When you see MC1003–MC1006 errors

- MC1003: unbalanced parens. Count your open vs close.
- MC1004: unexpected token, OR you used an unknown function. Only `if_null` is
  recognized as a function call; everything else is a measure ref.
- MC1005: trailing operator. You ended a formula with `+` or similar.
- MC1006: invalid number. Check for `1..5`, `1e`, or `1.2.3`.

For full diagnostic-code reference, see the `mosaic-debugger` agent.
```

This is what a skill looks like. Markdown + frontmatter; portable; readable.

### An agent example: `agents/mosaic-debugger.md`

```markdown
---
name: mosaic-debugger
description: Read Mosaic diagnostic JSON envelopes and propose specific fixes. Trigger when the user has run mc model validate / lint / test and gotten errors or warnings, or when an LLM-authored YAML failed to validate.
when_to_use:
  - User runs mc-cli and gets MC1xxx, MC2xxx, MC3xxx, or MC4xxx codes
  - YAML model authoring loop fails the validate/lint/test gate
  - User asks "what does MC<NNNN> mean?"
tools:
  - mc-cli (via MCP server)
  - file editing
---

# Mosaic Debugger

You are a specialist in Mosaic diagnostic codes. The user has a YAML model
that's producing errors or warnings; your job is to read the diagnostics,
explain them in user-friendly terms, and propose specific fixes.

## Process

1. **Get the diagnostics in JSON form.** Run
   `mc model {validate,lint} <path> --format json`. The output is an envelope
   `{ "schema_version": "1.0", "diagnostics": [...] }`.

2. **For each diagnostic, look up its code.** Use the registry below. Diagnostics
   are sorted (severity desc, code asc, yaml_pointer asc) so the most
   severe / earliest issue comes first.

3. **For each fix, propose the YAML edit + the rationale.** Don't just say "fix
   the typo" — show the before/after.

4. **Re-run after fixes.** New errors may surface that the original errors masked.
   Iterate until clean.

## Code registry (Phases 3A → 3D)

[Full table of MC1001–MC1006 + MC2001–MC2025 + MC3001–MC3011 (with MC3008
permanently retired) + MC4xxx reserved.]

[For each code: what it means, what fires it, the fix pattern, an example.]

## Anti-patterns

- Don't suppress lint warnings just to make output cleaner; understand each.
- Don't change the YAML schema to fit the data; change the data to fit the schema.
- Don't ignore MC1xxx parse errors and try to fix downstream errors first; parse
  must succeed before validate runs.
```

This is what an agent looks like. System prompt + clear when-to-use + tool access.

### MCP server config (`.mcp.json`)

```json
{
  "mcpServers": {
    "mosaic": {
      "command": "mc",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

The MCP server (which `mc-cli` would expose via a new `mc mcp` subcommand in Phase 4A) surfaces `mc model {validate, inspect, lint, test}` and `mc demo` as MCP tool calls. Claude Code can invoke them directly without shelling out.

**Implementation note:** the `mc mcp` subcommand is the smallest piece of new `mc-cli` code in Phase 4A. It runs the standard MCP server protocol over stdio and dispatches tool calls to the existing `mc-cli` verb implementations. Probably ~150 lines.

---

## Out of scope (recap)

The Decision 8 table is the binding list. Highlights:

- **No real actuals import.** Phase 5.
- **No DuckDB / external storage.** Phase 5+.
- **No UI editor.** Phase 6.
- **No model-backed cells.** Phase 6B+.
- **No custom LLM training.** Indefinitely.
- **No `mc-core`/`mc-fixtures`/`mc-model` source changes.** Locked.
- **No `Diagnostic` struct shape change.** Adding MC4xxx codes is fine; struct fields are not.
- **No schema changes to the YAML model format.** Phase 3A–3D contract holds.
- **No hosted authoring service.** Phase 7+ if ever.

---

## Accepted decisions — TL;DR

Phase 4 ships against:

1. **Phase 4 delivers** end-to-end LLM-authored Mosaic models that pass validate/lint/test, via the Mosaic plugin (4A) + Python reference adapters (4B). No Rust LLM client (Decision 1).
2. **Multi-provider via plugin abstraction:** plugin holds structured knowledge; Python reference adapters translate to provider-specific API calls. Default provider is Claude (Anthropic) per amendment D (Decision 2).
3. **Mosaic Claude Code plugin (Phase 4A)** at `mosaic-plugin/` — skills + agents + commands + MCP + hooks + examples. Marketing-mix (Acme) is the ONLY domain schema in 4A; future schemas are demand-driven phases (Decision 3 + amendment F).
4. **Plugin = institutional knowledge of Mosaic, in agent-framework-agnostic form.** This is the actual moat (see Strategic Centerpiece section). Markdown + JSON only; no code, no provider-specific tags. Stable diagnostic codes (MC1xxx–MC4xxx) are the cross-provider error vocabulary (Decision 4).
5. **NO new Rust crate** (per amendment A). Phase 4B ships Python reference adapters under `mosaic-plugin/examples/adapters/` (`anthropic-python/` + `openai-python/` minimum, ~150 lines each). Single small Rust addition: `mc-cli` gains `mc mcp` subcommand for the MCP server (Decision 5).
6. **Iteration loop consumes Phase 3B JSON envelope.** Up to N iterations (default 5) before declaring failure (Decision 6).
7. **Two sub-phases: 4A plugin, 4B Python adapters.** Phase 4C dissolved per "no vague TBD buckets" rule + amendment B + GPT point 8. After 4B, next phase is Phase 5 (actuals); future schemas / providers / production-polish are demand-driven (Decision 7).
8. **Hard locks carry forward** — `mc-core`, `mc-fixtures`, `mc-model` all locked. `mc-cli` locked except for the new `mc mcp` subcommand. **No tokio. No async. No reqwest. No Anthropic/OpenAI SDK in the workspace.** (Decision 8).
9. **Prompts live in the plugin (markdown)**, not in any runtime. Adapters read markdown and translate (Decision 9).
10. **Kernel role unchanged.** No `mc-core` awareness of LLM authoring (Decision 10).
11. **NO toolchain bump.** Rust stays at 1.78. Cargo.lock pins all stay intact. ADR-0009 NOT triggered by Phase 4 (Decision 11 amendment).

The Acme model becomes the first LLM-round-trip proof: a user prompts "marketing-mix for 5-channel B2C SaaS" via Claude Code (with the plugin installed) OR via `python examples/adapters/{anthropic,openai}-python/author.py "..."` and gets a YAML that passes `mc model validate/lint/test`.

---

## Acceptance amendments

This ADR was Proposed and Accepted on 2026-05-03 with project-owner amendments after parallel reviews from GPT and Claude Desktop. Both reviews converged on the same major restructure (drop the Rust LLM client crate; runtime moves to Python reference adapters under `mosaic-plugin/examples/adapters/`) plus several refinements. Captured here as the audit trail.

| # | Source | Amendment (one-line) | Where it landed |
|---|---|---|---|
| **A** | **Both reviews (the major restructure)** | **Drop `crates/mc-author/`. Phase 4B = Python reference adapters under `mosaic-plugin/examples/adapters/` (`anthropic-python/` + `openai-python/` ~150 lines each). No tokio, no async, no reqwest, no SDK deps in the Rust workspace. No Rust toolchain bump.** | Decision 5 rewritten; Decision 11 rewritten (no bump); Decision 7 sub-phase table updated; Decision 8 out-of-scope table extended |
| **B** | Both reviews | Phase 4C dissolved per "no vague TBD buckets" rule. After 4B, next phase is Phase 5 (actuals); future additions (more schemas, more providers, production polish, schema marketplace) are demand-driven phases. | Decision 7 updated; out-of-scope table extended; "what's next after 4B" clarified |
| **C** | Desktop | Elevate "knowledge embuing" / plugin-as-institutional-knowledge to a top-level strategic centerpiece BEFORE the decisions list. Future implementers will optimize for the wrong thing if they think the runtime is the deliverable. The runtime is the *vehicle*; the knowledge is the *cargo*. | New "Strategic centerpiece (read this first)" section added immediately after the header, before Context |
| **D** | Desktop | Default provider for the example adapters: Claude (Anthropic). Project's institutional tooling is Claude-based; defaults match. | Decision 2 (multi-provider strategy); previously open question #6, now confirmed |
| **E** | Desktop | Plugin location: in-repo at `mosaic-plugin/` (single source of truth, atomic commits with kernel changes, easier review). Once stable, extract to its own repo for marketplace distribution. | Decision 3 (plugin structure); previously open question #1, now confirmed |
| **F** | Both reviews | Phase 4A ships ONLY one domain schema: marketing-mix (Acme). Defer FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning to subsequent demand-driven phases. Reason: shipping 6 domain schemas at once means 6× the maintenance burden when the plugin format or skill schema evolves. | Decision 3 (plugin structure — `domain-schemas/` shows only `marketing-mix/`); Decision 7 (Phase 4A acceptance gate); out-of-scope table updated |
| **G** | GPT point 6 | Phase 4B starts with Anthropic Python + OpenAI Python only. Defer TypeScript, Codex, Gemini, Mistral, Ollama, cost tracking, prompt hardening, schema marketplace. | Decision 7 (Phase 4B deliverable scope); out-of-scope table updated |
| **H** | Process | The `mc mcp` subcommand in `mc-cli` is the single Rust addition Phase 4A needs (MCP server over stdio, surfacing existing `mc model {validate,inspect,lint,test}` verbs as MCP tool calls). ~150 lines, no new deps (uses Phase 3B's hand-rolled JSON serialization). | Decision 5 (the single small Rust change); out-of-scope table notes mc-cli is locked except for this subcommand |
| **I** | Process | ADR-first flow confirmed for Phase 4 per process-notes §1 self-test. Fails questions 1–3 (new deps in Python adapters even if not Rust; contract surface via plugin schema; kernel-adjacent in spirit). | Header note (already present); confirmed at Acceptance |

No remaining open questions. Phase 4A handoff lands at this ADR's Acceptance commit.

---

## Alternatives considered (whole-ADR scope)

1. **Skip the plugin; just ship `mc-author` with hardcoded prompts.** Rejected — violates the "not locked in" direction. Hardcoded prompts in Rust are provider-coupled, harder to update, opaque to non-Rust readers.
2. **Plugin without runtime — ship Phase 4A only, defer Phase 4B indefinitely.** Rejected — without a runtime, only Claude Code users get Mosaic LLM authoring. The "any AI agent can be embued" goal requires the SDK adapters.
3. **Bundle Phase 4A + Phase 4B into one phase.** Rejected — too big to ship cleanly. Decomposed sub-phases let each have its own acceptance gate.
4. **Use a single LLM provider (Anthropic) for Phase 4; defer multi-provider to "if it becomes a problem."** Rejected — the project owner's explicit direction is "not locked in." Building single-provider first and bolting on multi-provider later usually produces a leaky abstraction; building the abstraction up-front is easier when the surface is small.
5. **Train a custom Mosaic-authoring LLM.** Rejected — out of scope indefinitely. Mosaic is an authoring substrate, not an LLM platform.
6. **Build the plugin in a different format (custom Mosaic plugin spec, not Claude Code's).** Rejected — Claude Code's plugin format is well-designed, has an existing ecosystem, and the portability rule (Decision 4) means the same content also serves SDK adapters. No reason to invent a parallel format.
7. **Ship Phase 4 as just the plugin (skills + agents + commands), no Rust runtime.** Rejected — the runtime is what makes the system usable from CI, scripts, and non-interactive contexts. Plugin-only makes Phase 4 dependent on Claude Code being open and active.
8. **Build the runtime first, then the plugin.** Rejected — the plugin is the source-of-truth knowledge package. Building runtime first means hardcoding prompts that get refactored into the plugin later, which is wasted work.
9. **Skip Phase 4; jump to Phase 5 (actuals import).** Rejected — actuals without LLM authoring means the user manually authors models in YAML (Phase 3D's friendly form is OK but not transformative). LLM authoring is the differentiator that makes Mosaic accessible to non-engineers.
10. **Defer the plugin ecosystem to Phase 6 (UI), tied to a graphical authoring experience.** Rejected — the plugin is provider-agnostic infrastructure; tying it to UI scope conflates concerns. UI consumes the plugin's content via Phase 6's editor; that doesn't mean the plugin should wait for UI.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 4 row; will be updated to add 4A/4B/4C decomposition at this ADR's acceptance.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase status; will be updated to add Phase 4 as `proposed` once Accepted.
- [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md) — Mosaic as LNM platform; this ADR is the first phase that delivers the "AI-powered" half of "AI-powered Large Numbers Model."
- [`../process-notes.md`](../process-notes.md) §1 — handoff-first vs ADR-first sequencing; Phase 4 returns to ADR-first per the 5-question self-test.
- [`0004-phase-3a-model-definition-format.md`](0004-phase-3a-model-definition-format.md) — Phase 3A schema (the contract LLMs emit against).
- [`0005-phase-3b-model-qa-linter-diagnostics.md`](0005-phase-3b-model-qa-linter-diagnostics.md) — Diagnostic shape + JSON envelope + stable codes (the cross-provider error vocabulary Phase 4 consumes).
- [`0006-phase-3c-model-test-fixtures.md`](0006-phase-3c-model-test-fixtures.md) — Phase 3C schema + `mc model test` (the "did it produce the right values?" gate Phase 4's iteration loop runs).
- [`0007-phase-3d-friendly-formula-syntax.md`](0007-phase-3d-friendly-formula-syntax.md) — formula syntax (the friendlier surface LLMs author against).
- [`../specs/`](../specs/) — kernel contracts (Phase 4 doesn't touch).
- [`../../CLAUDE.md`](../../CLAUDE.md) — project name + naming convention rule (the Mosaic plugin name + crate names follow the established convention).
- [Claude Code plugin development docs] — referenced for plugin.json / SKILL.md / agent / command / hook / MCP-server formats; exact links + version compat checked at Phase 4A handoff time.

---

## Notes

This ADR is the strategic gate for Phase 4 the way ADR-0003 was for Phase 2, ADR-0004 was for Phase 3A, ADR-0005 for Phase 3B, ADR-0006 for Phase 3C, and ADR-0007 for Phase 3D. It scopes the Phase 4 umbrella (decompose into 4A / 4B / 4C); the sub-phase ADRs (probably ADR-0009 / 0010 / 0011 numerically) commit the implementation contracts.

If this ADR is amended after Acceptance, the amendment lands as `0008-amendment-N.md` (append-only, mirroring the ADR-0003 / 0004 / 0005 / 0006 / 0007 pattern).

**The Mosaic plugin is the most important new architectural concept in Phase 4.** It is not just "a place to put prompts" — it is the project's institutional knowledge in portable, AI-agent-readable form. Once the plugin exists, *any* AI agent (Claude Code, Anthropic SDK loop, OpenAI SDK loop, Codex session, future providers) can be embued with Mosaic-authoring competence. This is the load-bearing piece that makes the "not locked in" direction true.

**Phase 4 does NOT trigger the long-anticipated toolchain bump** (per acceptance amendment A + Decision 11 amendment). The Python adapters consume Anthropic / OpenAI SDKs in their own runtime; the Rust workspace stays at 1.78 with no SDK deps. Whatever future phase eventually triggers the Rust toolchain bump will get its own ADR.

**One philosophical note on the plugin / adapter relationship:** the plugin is the *source*; the Python adapters are *consumers*. Consumers don't write back to the source. If a real-world Mosaic deployment surfaces a lesson worth folding into the plugin (e.g., "GPT-4 reliably misunderstands formula precedence; add a clarifying sentence to skills/formulas/SKILL.md"), the lesson becomes a plugin update — committed to the Mosaic repo, distributed via marketplace. This keeps the knowledge artifact stable and centrally maintained, even as the runtime ecosystem grows.

**On the "no Rust LLM client" decision (acceptance amendment A):** this is the load-bearing engineering call in this ADR. The temptation to build `mc-author` as a Rust crate is real (consistency with the existing CLI, single-language workspace, no Python interop). The case against is stronger: plugin content + Python adapters cover the same use cases at a fraction of the maintenance burden, and the Rust workspace stays sync + dep-bounded + 1.78-pinned. If a real-world Rust customer materializes with a contractual no-Python requirement, *that's* the trigger for `mc-author` — as a feature-flagged optional workspace member that doesn't default-build. Phase 4 is not that trigger.
