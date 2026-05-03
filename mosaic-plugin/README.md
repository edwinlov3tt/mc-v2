# Mosaic — Claude Code plugin

Author, validate, lint, test, and inspect [Mosaic](https://github.com/edwinlov3tt/mc-v2) YAML models from any Claude Code session.

Mosaic is an AI-powered Large Numbers Model platform — a multidimensional engine for building large numerical models with deterministic semantics, structured diagnostics, and stable error codes that AI agents can iterate against.

This plugin packages the institutional knowledge needed to author Mosaic models: skills (authoring, debugging, schema design, formula syntax, testing, marketing-mix domain), agents (architect → author → debugger → validator), slash commands, and an MCP server that exposes the `mc model {validate,inspect,lint,test}` verbs as tool calls.

Phase 4A ships **one domain schema: marketing-mix** (Acme is the canonical reference). FP&A, sports-betting, prospect-scoring, sales-forecasting, demand-planning are demand-driven future phases per [ADR-0008 amendment F](https://github.com/edwinlov3tt/mc-v2/blob/main/docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md).

## Install precondition

The plugin invokes the `mc` CLI (built from the Mosaic Rust workspace) over MCP. Install it first:

```bash
git clone https://github.com/edwinlov3tt/mc-v2.git
cd mc-v2
cargo install --path crates/mc-cli
mc demo                     # smoke check — should run the Acme demo end-to-end
mc mcp < /dev/null          # MCP server starts; type Ctrl-D to exit
```

Once `mc` is on `PATH`, install this plugin into Claude Code (the plugin install path depends on your Claude Code distribution; place `mosaic-plugin/` under your plugins directory or symlink it).

## What you get

### Slash commands

| Command | What it does |
|---|---|
| `/mosaic-init <domain>` | Scaffold a new Mosaic model. Phase 4A supports `marketing-mix` only. |
| `/mosaic-validate [path]` | Run `mc model validate` on a model file. |
| `/mosaic-inspect [path]` | Render the model summary (dim counts, measures, rules, goldens). |
| `/mosaic-lint [path]` | Run `mc model lint` and surface any MC3xxx warnings. |
| `/mosaic-test [path]` | Run `mc model test` and report goldens passed/failed. |
| `/mosaic-author "<description>"` | End-to-end natural-language → working YAML pipeline. |

`/mosaic-explain <coord>` is **deferred to Phase 4A.2** — it requires a `mc model trace <coord>` CLI verb that doesn't exist yet, and shipping a degraded version would mislead the LLM.

### Agents

- **mosaic-architect** — designs schemas from natural-language requirements (dim list, measure classification, rule list).
- **mosaic-author** — writes YAML from the architect's plan; runs validate; hands off to mosaic-debugger on errors.
- **mosaic-debugger** — reads diagnostic JSON envelopes, looks up codes (MC1xxx–MC4xxx), proposes specific YAML edits.
- **mosaic-validator** — runs the full validate → lint → test sequence; reports clean or specific failure summary.

### Skills

| Skill | Teaches |
|---|---|
| `authoring` | End-to-end YAML structure: metadata, dimensions, measures, rules, canonical_inputs, test_fixtures. |
| `debugging` | The full MC1001–MC4xxx diagnostic-code registry through Phase 3D. The JSON envelope shape. Fix patterns. |
| `schema-design` | Dim order (binding), hierarchies, measure roles, aggregation rules (Sum vs WeightedAverage). |
| `formulas` | Phase 3D formula syntax: operators, `if_null`, identifier rules, MC1003–MC1006 errors. |
| `testing` | `canonical_inputs` (always-loaded) vs `test_fixtures` (named overlays); golden assertions; `--fixture` filter. |
| `domain-schemas/marketing-mix` | The marketing-mix domain pattern via Acme: 6 dims, 11 measures, 5 rules, hierarchies. |

### MCP server

The plugin's `.mcp.json` invokes `mc mcp`, which speaks JSON-RPC 2.0 over stdio and surfaces 5 tools:

- `mosaic.demo` — run the Acme demo (with optional `--model <path>`).
- `mosaic.model.validate` — parse + validate; returns a Phase 3B `{schema_version,diagnostics}` envelope.
- `mosaic.model.inspect` — model summary as text or JSON.
- `mosaic.model.lint` — lint warnings as the same envelope.
- `mosaic.model.test` — run goldens; returns `{schema_version,skipped,goldens}` envelope.

All tools return `{exit_code, stdout, ...}` so agents can iterate against structured feedback without parsing free-form text.

## Authoring loop

A typical end-to-end author session:

1. User: `/mosaic-author "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality"`
2. **mosaic-architect** designs the schema (dims, measures, rules); plan rendered for user review.
3. **mosaic-author** emits YAML; runs `mosaic.model.validate` via MCP.
4. If errors, **mosaic-debugger** parses the diagnostic envelope, looks up codes, proposes edits, re-runs.
5. **mosaic-validator** runs validate → lint → test; reports clean or specific failures.

Stable diagnostic codes (MC1xxx–MC4xxx) are the cross-provider error vocabulary. The same loop runs identically against Anthropic, OpenAI, or any other provider that consumes the plugin's markdown — Phase 4B (Python reference adapters) is the proof of that portability.

## Reference example

`examples/models/acme-marketing.yaml` (+ sibling `acme.inputs.csv`) is the canonical Acme reference: 6 dimensions × 11 measures × 5 rules × 2,520 input cells. It is byte-identical to the source-of-truth at `crates/mc-model/examples/acme.yaml` (and CSV) in the Mosaic repo.

## Hooks

`hooks/README.md` is a placeholder. Phase 4A ships no hooks; the canonical Claude Code hook spec was deferred for verification in a future phase per the Phase 4A handoff §I (skills/agents/commands carry the deliverable; hooks are decoration).

## Adapters

`examples/adapters/README.md` is a placeholder for **Phase 4B** — Python reference adapters (`anthropic-python/`, `openai-python/`) that consume this plugin's content and run the iteration loop against `mc-cli`. Phase 4A does not ship adapter code.

## Plugin format note

The manifest at `.claude-plugin/plugin.json` follows the canonical Claude Code plugin format observed in `vercel/0.40.1` and `superpowers/5.0.7` (cached locally under `~/.claude/plugins/cache/claude-plugins-official/`). [ADR-0008 Decision 3](https://github.com/edwinlov3tt/mc-v2/blob/main/docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) sketched the manifest at the plugin root with `displayName`, `skills`, `mcpServers`, `hooks` keys; the canonical format places it in `.claude-plugin/` and uses `commands[]` + `agents[]` arrays with skills auto-discovered. The Phase 4A completion report documents this divergence (authorized via SPEC QUESTION resolution, not silent drift).

## License

MIT OR Apache-2.0, matching the Mosaic project.
