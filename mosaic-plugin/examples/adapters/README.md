# Mosaic adapters

Reference Python adapters that consume the Mosaic plugin's content
(`skills/`, `agents/`, `commands/`, `examples/models/acme-marketing.yaml`)
and author working Mosaic YAML models via different LLM providers. Each
adapter is a single self-contained `author.py` plus a `pyproject.toml` and
`README.md` — designed to be read end-to-end as a worked example, not
installed as a framework.

## Adapters in this directory

| Adapter | Provider | Role | Default model |
|---|---|---|---|
| [`anthropic-python/`](anthropic-python/) | Anthropic | **Default per ADR-0008 amendment D** — canonical reference | `claude-opus-4-7` |
| [`openai-python/`](openai-python/) | OpenAI | Cross-provider proof per ADR-0008 amendment G | `gpt-5.5` |

Both adapters consume the **same plugin content** and run the **same
iteration loop** against `mc model {validate,lint,test} --format json`. The
plugin is provider-agnostic; only each adapter's `author.py` is
provider-coupled (it imports `anthropic` or `openai`). Adding a future
provider means adding one new adapter directory under this path; it never
means modifying the plugin's `skills/` / `agents/` / `commands/`.

See each adapter's README for install and usage instructions. They follow
the same shape: install precondition (`mc` on PATH), Python ≥ 3.10,
`pip install -e .`, set the provider's API-key env var, run
`python author.py "..."`.

## Why two providers (and not more)

Per [ADR-0008](../../../docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) amendment G, Phase 4B ships **Anthropic + OpenAI Python only**. Two providers are sufficient to prove portability — one is "we can drive
Claude," two is "we can drive any LLM that can read markdown and emit YAML."
Adding a third before there's a real customer demand would be premature
generalization (and would carry maintenance burden when the plugin format
or skill schema evolves).

The following are **explicit non-goals** for Phase 4B:

- **TypeScript / Codex / Gemini / Mistral / Ollama / Vertex AI / Bedrock
  adapters** — demand-driven future phases, scoped against actual
  consumer needs.
- **Native MCP-from-Python integration** — these adapters use subprocess
  (`mc model ...`); the MCP server (Phase 4A's `mc mcp`) is the right tool
  for Claude Code, which has native MCP support.
- **Production polish** — no async, no streaming, no adapter-level retries,
  no rate limiting, no cost tracking, no telemetry, no prompt hardening,
  no schema marketplace.
- **Alternative domains** — marketing-mix only (per ADR-0008 amendment F).

## The portability claim

The point of shipping two adapters is this: if both produce a valid Mosaic
YAML for the same prompt — different in their structural choices but both
passing `mc model validate / lint / test` — then the plugin's
institutional knowledge has been embued into two different LLM
environments via the same shape. The runtime is the *vehicle*; the plugin
is the *cargo* (per ADR-0008 strategic centerpiece + amendment C).

The Phase 4B proof transcripts at `docs/reports/phase-4b-proof/` capture
both adapters' end-to-end runs against the same canonical prompt, including
observed flake rates and structural divergences between the two providers'
outputs.

## Adding a new adapter (future demand-driven phase only)

If a future phase adds a new provider:

1. Create a new sibling directory under `mosaic-plugin/examples/adapters/`
   (e.g., `gemini-python/`, `typescript/`, etc.).
2. Mirror the existing shape: `pyproject.toml` with one first-party SDK
   dep, `README.md` with install instructions, `author.py` with the same
   plugin-content-loading + iteration-loop shape.
3. Do **not** modify the plugin's `skills/` / `agents/` / `commands/`.
   Provider-specific tweaks live in the adapter's own prompt-construction
   logic, not in the shared knowledge package.
4. Add a row to the table above and a short note in this index.

The plugin is the source; the adapters are consumers. Consumers do not
write back to the source.
