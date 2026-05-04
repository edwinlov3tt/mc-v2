# mosaic-openai-adapter

Phase 4B Python reference adapter — consumes the Mosaic plugin's content
and authors Mosaic YAML models via the OpenAI Python SDK.

This is the **cross-provider proof** per [ADR-0008](../../../../docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) amendment G. The default provider is Anthropic (amendment D); see
[`../anthropic-python/`](../anthropic-python/) for the canonical reference.

## Install precondition: `mc` on PATH

The adapter shells out to `mc model {validate,lint,test} --format json` via
subprocess. Install the Rust CLI from the workspace root:

```bash
cargo install --path crates/mc-cli --locked
which mc        # expected: ~/.cargo/bin/mc
```

## Install the adapter

Requires Python ≥ 3.10. From this directory:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e .
# OR (if you use uv):
uv pip install -e .
```

The only first-party dependency is the `openai` SDK (per ADR-0008
amendment G — no `pyyaml` / `pydantic` / `httpx` / etc.).

## Set the API key

```bash
export OPENAI_API_KEY=...   # NEVER pass via CLI arg; never log
```

The SDK reads the env var directly.

## Usage

```bash
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"
```

The adapter:

1. Walks up to the plugin root (`mosaic-plugin/`) and concatenates every
   `*.md` under `skills/`, `agents/`, `commands/`, plus the canonical
   `examples/models/acme-marketing.yaml`, into a single system prompt.
2. Appends a response-format instruction telling the model to emit YAML in
   a single `` ```yaml ``...`` ` `` `` fenced block with no surrounding prose.
3. Calls OpenAI's `responses.create` with the system + user message in the
   `input=[...]` array.
4. Extracts the YAML from the first ` ```yaml ` fence in the response (or
   falls back to the raw response if no fence is present).
5. Iterates against `mc model {validate,lint,test} --format json` (subprocess)
   for up to N=5 rounds, feeding structured diagnostics + golden failures
   back to the model after each round.
6. On convergence: writes the YAML to `output.yaml` (or `--output <path>`)
   and exits 0. On non-convergence: writes the last failed YAML to
   `output.failed.yaml`, prints the last diagnostic envelope, and exits 2.

### Flags

| Flag | Default | Behavior |
|---|---|---|
| `--output <path>` | `output.yaml` | Where to write the converged YAML. |
| `--max-iterations <N>` | `5` | Iteration cap before declaring failure. |
| `--strict` | off | Treat MC3xxx lint warnings as blocking errors. |

## Example output

```
$ python author.py "marketing-mix model for a 5-channel B2C SaaS ..."
[mosaic] plugin root: /path/to/mosaic-plugin
[mosaic] system prompt: 130,234 chars
[mosaic] calling gpt-5.5 (initial draft)...
[mosaic][iter 1] converged: validate/lint/test all pass

[mosaic] Converged in 1 iteration(s). YAML written to output.yaml
```

Then verify by hand:

```bash
mc model validate output.yaml
mc model lint output.yaml
mc model test output.yaml
```

All three should exit 0 with no warnings (per the Acme reference standard).

## What this adapter is, and isn't

This is a **reference adapter, not a production framework.** Per ADR-0008
amendments A + G, the goal is to prove the plugin's institutional content
is portable across LLM providers — Anthropic + OpenAI ship in Phase 4B as
the proof, with future providers becoming separate demand-driven phases.

There is no:
- async / streaming / concurrency
- adapter-level retries beyond what the SDK does internally
- rate limiting, exponential backoff, cost tracking, telemetry
- prompt hardening or adversarial-input handling
- partial-completion resumption
- alternative-domain support (marketing-mix only — ADR-0008 amendment F)

If you want a production framework, fork this adapter and build one. The
plugin content (`mosaic-plugin/skills/`, `agents/`, `commands/`) is the
institutional knowledge; this script is a thin wire.

## Troubleshooting

- **`mc: command not found`** — install with `cargo install --path crates/mc-cli`
  from the workspace root (Rust 1.78+ required).
- **`openai.AuthenticationError`** — set `OPENAI_API_KEY` in the env.
- **Convergence failure (max_iterations exceeded)** — the last failed YAML
  and last diagnostic envelope are written to `output.failed.yaml` for
  inspection. Common causes: prompt is genuinely under-specified, model
  string is stale (OpenAI ships new flagships ~quarterly — verify `MODEL`
  in `author.py` is current), API key has insufficient credit.
