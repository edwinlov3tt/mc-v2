# Phase 4B Handoff — Python Reference Adapters

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 4B.
> **You inherit a green Phase 4A** (commit `36af56c`, tag
> `phase-4a-mosaic-plugin`, 416 / 0 tests).
>
> **This phase ships two Python reference adapters** under
> `mosaic-plugin/examples/adapters/` that consume the Phase 4A
> plugin's content and produce working Mosaic YAML via two LLM
> providers (Anthropic + OpenAI). The adapters prove the plugin
> is portable across LLM environments — the load-bearing claim
> behind ADR-0008's strategic centerpiece.
>
> **Read [ADR-0008](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) BEFORE this handoff.** ADR-0008's amendment table is the
> binding strategic context for Phase 4B; the relevant amendments
> are **A** (no Rust LLM client; Python adapters under `examples/
> adapters/`), **D** (default provider Claude/Anthropic), and **G**
> (start with Anthropic + OpenAI Python only; defer everything
> else). The handoff has an at-a-glance amendments quick-reference
> below as compactor insurance, but the ADR is canonical.
>
> **Process note:** this handoff was drafted under the
> **handoff-first parallel flow** (per [`../process-notes.md`](../process-notes.md) §1) after applying the 5-question self-test
> (all 5 yes — see §"Self-test result" below). ADR-0010 (if
> needed) lands in parallel with implementation.
>
> **Hard rule:** Phase 4B touches `mosaic-plugin/examples/
> adapters/anthropic-python/` and `mosaic-plugin/examples/
> adapters/openai-python/` (NEW), plus a small modify of
> `mosaic-plugin/examples/adapters/README.md` (replace the Phase
> 4B placeholder with an adapter-list pointer). It does NOT touch
> the Rust workspace (`crates/`), the plugin's `skills/` /
> `agents/` / `commands/` / `.claude-plugin/` / `.mcp.json` /
> `examples/models/` / `hooks/`, the kernel, the model layer, or
> any spec/decision document. The locked-surfaces guarantee from
> Phases 2D / 3A / 3B / 3C / 3D / 4A carries forward.

---

## The one paragraph you must internalize before writing code

**Phase 4B is the *portability proof*, not a production framework.** Two
adapters, two providers, ~150 lines each. Both adapters read the SAME
plugin content (the markdown skills + agent system prompts + command
descriptions + Acme example), and both produce a working Mosaic YAML
for the same user prompt. **If both pass `mc model validate / lint /
test`, the portability claim is proven.** Different LLMs will produce
different valid models for the same prompt — that's expected and not a
failure; the gate is "valid Mosaic model," not "byte-identical output."

You are NOT building a production iteration framework. You are NOT
optimizing for token cost or latency. You are NOT hardening against
adversarial prompts. You are NOT adding retries or partial-completion
resumption. You are demonstrating that the plugin's structured
knowledge (Phase 4A's deliverable) is consumable by any LLM, in any
language, via the same shape — and that the deliverable was right.

If at any point you find yourself reaching for production polish
(rate limiting, exponential backoff, streaming responses, structured
output schemas, function calling, custom telemetry, multi-turn
context management beyond what the iteration loop needs), STOP. That's
explicit Phase 4B out-of-scope per ADR-0008 amendment G. Phase 4B is
a reference, not a framework. Future demand-driven phases harden it.

**Provider coupling clarification (binding):** the *plugin content* is
provider-agnostic — markdown skills, agent system prompts, command
descriptions, the Acme example. Each *adapter's `author.py`* is
necessarily provider-coupled (it does `import anthropic` or `import
openai`). Adding a provider in a future demand-driven phase means
adding one new adapter directory under `mosaic-plugin/examples/
adapters/`; **it never means modifying the plugin's `skills/` /
`agents/` / `commands/`.** The knowledge artifact is portable; the
runtime files are necessarily one-per-provider.

---

## ADR-0008 amendments quick-reference (compactor insurance)

The 9 acceptance amendments folded into ADR-0008 on 2026-05-03. Phase
4A applied amendments A (in part), B, C, E, F, H, I directly. Phase
4B applies the remaining commitments. Read the full ADR for context;
this is the at-a-glance shape:

| # | Amendment | How it shows up in Phase 4B |
|---|---|---|
| **A** | **No new Rust crate; Phase 4B is Python adapters under `mosaic-plugin/examples/adapters/`. No SDK deps in Rust workspace. No tokio / async / reqwest in Rust.** | This phase is the Python-adapter half of amendment A. The Rust workspace stays untouched (0-line diff vs `phase-4a-mosaic-plugin`). |
| **B** | Phase 4C dissolved. After 4B, next phase is Phase 5 (actuals). | Phase 4B does not pre-name a Phase 4B.1 / 4C / etc.; if a real customer demand surfaces (TypeScript adapter, Codex/Gemini, production polish), it's a separate demand-driven phase scoped at that point. |
| **C** | "Knowledge embuing" / plugin-as-institutional-knowledge is the strategic centerpiece. | Phase 4B is the proof. Both adapters consume the plugin verbatim; neither hardcodes Mosaic-authoring knowledge in Python. The plugin is the source; Python is a consumer. |
| **D** | **Default provider for example adapters: Claude (Anthropic).** | `anthropic-python/` is the canonical/reference adapter; `openai-python/` is the cross-provider proof. Both ship; `anthropic-python/` gets first/primary positioning in README content. |
| **E** | Plugin location in-repo at `mosaic-plugin/`. | Adapters live at `mosaic-plugin/examples/adapters/<provider>-python/` — already created as placeholder by Phase 4A. |
| **F** | Phase 4A ships ONLY marketing-mix domain. | Phase 4B inherits this — adapters demonstrate the marketing-mix authoring path; no other domain is exercised. |
| **G** | **Phase 4B starts with Anthropic Python + OpenAI Python only. Defer TypeScript, Codex, Gemini, Mistral, Ollama, cost tracking, prompt hardening, schema marketplace.** | This is the load-bearing scope rule for 4B. Two adapters, two providers, no others. |
| **H** | Single Rust addition in 4A is `mc mcp` subcommand. | Phase 4B does NOT touch `mc mcp` (or any Rust). Adapters call `mc model {validate,inspect,lint,test}` via subprocess, NOT via MCP. (Native MCP-from-Python is a future demand-driven phase if a real customer needs it.) |
| **I** | ADR-first flow confirmed for Phase 4 per process-notes §1 self-test. | Phase 4B uses **handoff-first parallel flow** (this handoff) per the same process-notes §1 self-test — all 5 questions yes for 4B because the strategic decisions are committed in ADR-0008 amendments A + D + G; ADR-0010 (if any new strategic surface emerges) lands in parallel. See "Self-test result" §below. |

If you read this table and a section of the prompt body below seems
to disagree, the ADR wins — but flag the discrepancy as a SPEC
QUESTION before acting on it; it likely means a compact dropped
something.

---

## Self-test result (handoff-first eligibility)

Per [`../process-notes.md`](../process-notes.md) §1's 5-question
self-test, Phase 4B passes all 5:

| # | Question | Phase 4B answer |
|---|---|---|
| 1 | Kernel change? | **No.** Rust workspace untouched. |
| 2 | Runtime dep to any crate? | **No** in the Rust workspace (the question's intent). Python `anthropic` + `openai` SDKs are isolated to per-adapter `pyproject.toml` files in `mosaic-plugin/examples/adapters/<provider>-python/` — first Python deps in the project, but scoped to two reference subdirectories explicitly designed for this. |
| 3 | Contract shape change (Diagnostic / schema_version / mc-core public API)? | **No.** Adapters consume the existing Phase 3B JSON envelope verbatim. |
| 4 | < ~1500 LOC added across all crates? | **Yes.** ~300–500 total: 2 × ~150-line `author.py` + 2 × ~50-line README + 2 × ~30-line `pyproject.toml`. |
| 5 | Strategic decisions derivable from prior ADRs? | **Yes.** ADR-0008 amendments A + D + G commit the entire shape (Python adapters, Anthropic default, two providers only, scoped location). |

**Result: handoff-first parallel flow appropriate.** ADR-0010 (if
needed for any new strategic surface) drafts in parallel with
implementation; if it surfaces a substantive change, the implementer
gets a SPEC QUESTION mid-flight. The decisions in this handoff are
the binding contract until that happens.

---

## Where Phase 4A ended

- **Phase 4A commit / tag:** `36af56c` — *phase-4a: mosaic claude code plugin (skills + agents + commands + .mcp.json + mc mcp subcommand)* — tag `phase-4a-mosaic-plugin`. Pushed to `origin/main` 2026-05-03.
- **Test status:** 416 / 0 passing across all targets. 10/10 deterministic.
- **Plugin shipped at `mosaic-plugin/`** with the canonical Claude Code shape (manifest at `.claude-plugin/plugin.json`; commands + agents as arrays; skills auto-discovered).
- **`mc mcp` subcommand** at `crates/mc-cli/src/mcp.rs` (318-line parser body + 66-line emitter; no new deps; surfaces 5 tools — `mosaic.demo`, `mosaic.model.{validate,inspect,lint,test}`).
- **`mc` binary:** must be on PATH for Phase 4B adapters to call `mc model ...` via subprocess. If not present, run `cargo install --path crates/mc-cli` to put it at `~/.cargo/bin/mc`.
- **Plugin's example Acme:** byte-identical to `crates/mc-model/examples/acme.yaml` + `acme.inputs.csv`. `mosaic-plugin/examples/models/acme-marketing.yaml` is the YAML; `acme.inputs.csv` is the CSV (kept original filename so the YAML's `source:` field resolves at runtime).
- **`mosaic-plugin/examples/adapters/README.md`** is currently a Phase 4B placeholder pointing at "not yet shipped" — that's what Phase 4B replaces.
- **Toolchain:** Rust 1.78. Cargo.lock pins from Phase 1B + Phase 3A. **Do not bump.** ADR-0008 Decision 11 is explicit: Phase 4 does NOT trigger the toolchain bump. Phase 4B introduces Python deps but they are isolated to per-adapter `pyproject.toml` files; the Rust toolchain stays at 1.78.
- **Diagnostic-code registry through Phase 3D:** MC1001–MC1006 (parse), MC2001–MC2025 (validation; MC3008 retired and promoted to MC2011), MC3001–MC3007 + MC3009–MC3011 (lint; MC3008 permanently retired), MC4xxx (reserved). Stable JSON envelope: `{ "schema_version": "1.0", "diagnostics": [...] }`. Phase 4A added zero new codes. **Phase 4B adds zero new codes.**

For the full Phase 4A audit see [`../reports/phase-4a-completion-report.md`](../reports/phase-4a-completion-report.md). For the Phase 4 strategic context see [`../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md`](../decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md).

---

## Phase 4B prompt (verbatim — this is your contract)

> We are starting Mosaic Phase 4B: Python Reference Adapters.
>
> **Context.** Phase 4A shipped the Mosaic Claude Code plugin — institutional knowledge in agent-framework-agnostic form (markdown + JSON; no code; no provider-specific tags). Phase 4B is the portability proof: two Python adapters that consume the SAME plugin content and produce working Mosaic YAML via two providers (Claude/Anthropic + OpenAI). If both adapters can drive a working YAML from the same plugin, the "any AI agent can be embued" claim is closed.
>
> **Goal.** Ship two Python reference adapters at `mosaic-plugin/examples/adapters/anthropic-python/` and `mosaic-plugin/examples/adapters/openai-python/` such that:
>
> 1. Each adapter reads the plugin's `skills/`, `agents/`, `commands/`, and `examples/` content from `mosaic-plugin/` (resolved relative to the adapter's location).
> 2. Each adapter accepts a natural-language prompt as a CLI arg, calls its provider's API with the plugin content as system prompt, and produces a candidate Mosaic YAML.
> 3. Each adapter runs an iteration loop against `mc model {validate,lint,test} --format json` (subprocess to the existing `mc` binary, NOT the MCP server), parses the Phase 3B diagnostic JSON envelope, feeds structured diagnostics back to the LLM, and iterates up to N=5 times before declaring failure.
> 4. **Headline acceptance (best-of-3 per provider — read carefully):** the same prompt — *"marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"* — is run **3 times against EACH adapter** (6 runs total). For each adapter, **at least 2 of 3 runs must converge** to a YAML that passes `mc model validate / lint / test` (exit 0; zero documented warnings; goldens pass within 1e-9). All 6 run outcomes — successes AND failures — are captured in the proof transcripts (auditable record of real flake rates). The outputs across runs and across adapters are NOT required to be byte-identical (different LLMs produce different valid models, and even the same LLM produces different models across runs); the gate is "≥ 2/3 valid per adapter," not "deterministic output." **Why best-of-3:** LLM authoring success rates on novel structured tasks are stochastic. Single-shot gating would conflate flake with adapter bugs and tempt either silent `max_iterations` bumping or selection-bias re-runs. Best-of-3 acknowledges the stochastic reality, captures the actual flake rate as audit evidence, and stays meaningful: an adapter that fails ≥ 2/3 runs is not a flake — it's evidence of an LLM-specific limitation OR a real adapter bug, both of which are SPEC QUESTION territory.
> 5. The plugin's `skills/`, `agents/`, `commands/` content is unchanged — adapters are READ-only consumers. (Bug fixes to plugin content discovered during 4B implementation are surfaced as a SPEC QUESTION + a 4A.1 follow-up commit, NOT folded into the 4B implementation commit.) **Mechanical enforcement:** the locked-surfaces git-diff gate at validation time enforces this — `git diff phase-4a-mosaic-plugin -- mosaic-plugin/skills/ mosaic-plugin/agents/ mosaic-plugin/commands/` MUST return 0 lines or Phase 4B fails its acceptance gate.
>
> **Phase 4B scope** (binding contract):
>
> 1. **Create `mosaic-plugin/examples/adapters/anthropic-python/`** with:
>    - `pyproject.toml` — declares `anthropic` SDK dep; PEP 621 metadata; **MUST include `requires-python = ">=3.10"`** so `pip install -e .` fails fast on older Python (e.g., macOS-default 3.9) instead of letting `author.py` crash on modern type-hint syntax (`X | None`, `list[str]`)
>    - `README.md` — install instructions (`pip install -e .` or `uv pip install -e .`), env-var auth (`ANTHROPIC_API_KEY`), usage example, expected output
>    - `author.py` — the iteration loop (~150 lines target; loose budget — see SPEC QUESTION trigger #5)
>
> 2. **Create `mosaic-plugin/examples/adapters/openai-python/`** with:
>    - `pyproject.toml` — declares `openai` SDK dep; PEP 621 metadata; **MUST include `requires-python = ">=3.10"`** (same reason as anthropic-python)
>    - `README.md` — same shape with `OPENAI_API_KEY`
>    - `author.py` — same iteration loop with OpenAI API calls
>
> 3. **Replace `mosaic-plugin/examples/adapters/README.md`** (currently a Phase 4B placeholder) with a real adapter index: lists the two adapters, points users at each one's README for installation, documents the "default = Anthropic per ADR-0008 amendment D" convention, and notes that future adapters (TypeScript, Codex, Gemini, etc.) are demand-driven future phases.
>
> 4. **Iteration loop architecture (per `author.py`):**
>
>    ```python
>    def author(user_prompt: str, max_iterations: int = 5) -> str:
>        plugin_root = find_plugin_root()  # walk up from __file__
>        plugin_content = load_plugin_content(plugin_root)
>        system_prompt = build_system_prompt(plugin_content)
>
>        candidate_yaml = call_provider(system_prompt, user_prompt)
>
>        for attempt in range(max_iterations):
>            yaml_path = write_temp_yaml(candidate_yaml)
>            diagnostics = run_mc(["model", "validate", str(yaml_path), "--format", "json"])
>            if not has_blocking_errors(diagnostics):
>                break
>            candidate_yaml = call_provider_with_diagnostics(
>                system_prompt, user_prompt, candidate_yaml, diagnostics
>            )
>        else:
>            raise ConvergenceFailure(...)  # surfaces the last diagnostics
>
>        # After validate is clean, run lint + test (separate calls)
>        # Lint warnings: surface to user; only block on `--strict` flag (default off)
>        # Test failures: feed back like validate errors; iterate
>        return candidate_yaml
>    ```
>
>    **Before pasting the inline examples below into your adapter, verify the model strings are still current.** This handoff was drafted 2026-05-03; both Anthropic and OpenAI have a track record of shipping new flagship models on roughly quarterly cadence. Run `web_search` (or read the current Anthropic API docs + OpenAI Platform docs) for "current Anthropic Claude top model" and "current OpenAI GPT top model"; if either has shifted from the strings below, use the current name. SPEC QUESTION trigger #8 is the fallback if the SDK doesn't provide a current default.
>
>    Adapter-specific provider call (Anthropic example — verified current at 2026-05-03):
>    ```python
>    import anthropic
>    client = anthropic.Anthropic()  # reads ANTHROPIC_API_KEY from env
>    response = client.messages.create(
>        model="claude-opus-4-7",
>        max_tokens=8000,
>        system=system_prompt,
>        messages=[{"role": "user", "content": user_prompt}],
>    )
>    return extract_yaml_from_response(response.content[0].text)
>    ```
>
>    Adapter-specific provider call (OpenAI example — verified current at 2026-05-03 per OpenAI Platform docs):
>    ```python
>    from openai import OpenAI
>    client = OpenAI()  # reads OPENAI_API_KEY from env
>    response = client.responses.create(
>        model="gpt-5.5",  # OpenAI's current flagship reasoning + coding model; verify at execution time
>        input=[
>            {"role": "system", "content": system_prompt},
>            {"role": "user", "content": user_prompt},
>        ],
>    )
>    return extract_yaml_from_response(response.output_text)
>    ```
>
> 5. **Plugin content loading (shared concept; each adapter implements its own — no shared utilities crate in 4B):**
>    - Walk plugin root from `Path(__file__).resolve().parents[3]` (three levels up: `anthropic-python/` → `adapters/` → `examples/` → `mosaic-plugin/`).
>    - Read every `.md` under `skills/` (recursive; auto-discover, mirroring how Claude Code loads skills).
>    - Read every `.md` under `agents/` and `commands/`.
>    - Read the canonical Acme example at `examples/models/acme-marketing.yaml` for in-context few-shot.
>    - Concatenate into a single `system_prompt` string with clear section headers (`# Skill: <name>`, `# Agent: <name>`, etc.) so the LLM can navigate the content.
>    - **Include the Acme example as a verbatim block** so the LLM has a concrete reference for the schema shape it should emit.
>    - **The system prompt MUST end with an explicit response-format instruction:** *"Respond with the complete YAML model in a single fenced block (```yaml ... ```) with no surrounding prose, commentary, or explanation. The validate/lint/test pipeline runs against the YAML directly; any text outside the fence will be discarded."* This cuts YAML-extraction failure rate by roughly 5×; the fallback extraction logic in scope item 7 still handles edge cases, but the fence instruction prevents most of them.
>
> 6. **Diagnostic-feedback loop:**
>    - Adapter calls `mc model validate <path> --format json` → captures stdout (the JSON envelope) + exit code.
>    - Parses the envelope: `{"schema_version": "1.0", "diagnostics": [...]}`.
>    - Filters to severity=error (MC1xxx + MC2xxx). MC3xxx warnings do NOT block iteration unless adapter is run with `--strict` flag (default off).
>    - Constructs a feedback message: "The YAML you produced has these errors:" followed by structured diagnostic content (code, severity, path, message, suggestion).
>    - Calls provider again with: original system prompt + original user prompt + the FAILED YAML + the diagnostic feedback. The LLM then proposes a corrected YAML.
>
> 7. **YAML extraction from LLM response:**
>    - LLM responses often wrap YAML in markdown code fences (```yaml ... ```). Strip the fences before writing to disk.
>    - If the response contains prose around the YAML, take only the YAML block (first triple-backtick-delimited block; or if no backticks, treat the whole response as YAML).
>    - On extraction failure: feed back to the LLM with "your previous response was not parseable as YAML; please respond with ONLY a YAML block."
>
> 8. **Subprocess discipline:**
>    - Adapters call `mc` via `subprocess.run(["mc", "model", ...], capture_output=True, text=True, check=False)`.
>    - **`mc` MUST be on PATH** — adapter README documents this as an install precondition (`cargo install --path crates/mc-cli` from the workspace root).
>    - Do NOT add a `--mc-path` arg in Phase 4B (defer to a future phase if needed). Keep the adapter surface minimal.
>    - Subprocess errors (non-zero exit, stderr present) are caught and surfaced cleanly; never let a `subprocess.CalledProcessError` propagate as an unhandled exception in `author.py`.
>
> 9. **Auth handling:**
>    - `ANTHROPIC_API_KEY` for `anthropic-python/`; `OPENAI_API_KEY` for `openai-python/`. Read from env via the SDKs' default behavior.
>    - On missing key: clean error message pointing at the README. NEVER log the key. NEVER pass it as a CLI arg.
>    - No keychain integration, no `.env` file loading (defer to user's shell), no auth fallbacks.
>
> 10. **Output:**
>     - Default: write the converged YAML to `output.yaml` in the current working directory; print the path; exit 0.
>     - Optional `--output <path>` flag.
>     - On failure (max iterations exceeded): write the last failed YAML to `output.failed.yaml`, print the last diagnostic envelope, exit non-zero (e.g., exit 2).
>
> **Hard rules:**
>
> - **Rust workspace LOCKED.** No source change in any crate. `git diff phase-4a-mosaic-plugin -- crates/` returns 0 lines. `cargo test --workspace` still produces 416 / 0 at HEAD.
> - **Plugin's `skills/`, `agents/`, `commands/`, `.claude-plugin/`, `.mcp.json`, `examples/models/`, `hooks/` LOCKED.** `git diff phase-4a-mosaic-plugin -- mosaic-plugin/skills/ mosaic-plugin/agents/ mosaic-plugin/commands/ mosaic-plugin/.claude-plugin/ mosaic-plugin/.mcp.json mosaic-plugin/examples/models/ mosaic-plugin/hooks/` returns 0 lines.
> - **`mosaic-plugin/examples/adapters/README.md` is the ONLY existing file Phase 4B modifies.** Everything else under `examples/adapters/` is NEW per-adapter directory contents.
> - **Toolchain:** Rust stays at 1.78. Cargo.lock pins intact. Python: minimum 3.10 (modern syntax, type hints; nothing exotic). No Python version bump beyond what the SDK minimums require.
> - **No new Rust deps.** Period.
> - **Python deps per adapter:** `anthropic` SDK in `anthropic-python/pyproject.toml`; `openai` SDK in `openai-python/pyproject.toml`. NOTHING ELSE in `pyproject.toml` (no `pyyaml`, no `requests`, no `pydantic`, no `httpx`, no test framework). The SDKs bring their own transitive deps — that's accepted; declared first-party deps are 1 per adapter.
> - **No async, no concurrency.** Both adapters are sync, single-threaded. Provider SDKs both have sync clients; use those.
> - **No production polish.** No retries (provider SDKs may auto-retry; that's fine — don't add adapter-level retry logic). No exponential backoff. No streaming responses (single response per call). No rate limiting. No cost tracking. No telemetry. No partial-completion resumption. No graceful-degradation fallback to a different provider.
> - **No new diagnostic codes.** Phase 4B adds zero codes. The diagnostic envelope's `schema_version` stays `"1.0"`.
> - **Marketing-mix is the ONLY domain** (per ADR-0008 amendment F). Adapters demonstrate the marketing-mix authoring path; no other domain is exercised. The acceptance prompt is marketing-mix-shaped.
> - **No provider-specific content in the plugin.** If during 4B you discover the plugin's content needs a provider-specific tweak (e.g., "Anthropic responds better to numbered lists; OpenAI to bullets"), DO NOT add that tweak to the plugin's `skills/` or `agents/`. Surface as a SPEC QUESTION; the resolution likely belongs in the adapter's own prompt-construction logic, not in the shared plugin.
> - **MCP integration is OUT OF SCOPE.** Adapters call `mc model ...` via subprocess only. Native MCP-from-Python (using an MCP client library) is a future demand-driven phase if a real customer needs it.
> - **All 416 existing tests must still pass.** Phase 4B adds no Rust tests. Optional: add a single Python smoke test per adapter (e.g., `tests/test_load_plugin_content.py`) that verifies plugin-content loading works without making any API calls. If you add Python tests, document the runner; don't wire them into `cargo test`.
> - **You did NOT start Phase 5 (actuals), Phase 4A.1 (hooks), Phase 4A.2 (mc model trace), or any other phase.** Phase 4B's deliverable is the two adapters. Period.
> - **You did NOT add a third adapter** (no TypeScript, Codex, Gemini, Mistral, Ollama, Vertex AI, Bedrock, etc.). Per ADR-0008 amendment G, those are demand-driven future phases.
> - **You did NOT modify ADR-0008 or any earlier ADR.** Inherited contracts.
>
> **Acceptance gate (the headline + supporting):**
>
> Headline: **For each adapter, ≥ 2 of 3 runs of the canonical acceptance prompt converge to a YAML that passes `mc model validate / lint / test` with zero documented warnings.** Concretely, the canonical acceptance prompt is:
>
> > *"marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"*
>
> Run **3 times against each adapter** (6 runs total). Capture all 6 outcomes — successes AND failures — in the proof transcripts. Adapter passes iff ≥ 2/3 of its runs converge.
>
> Supporting:
>
> 1. `mosaic-plugin/examples/adapters/anthropic-python/` directory exists with `pyproject.toml`, `README.md`, `author.py`.
> 2. `mosaic-plugin/examples/adapters/openai-python/` directory exists with the same three files.
> 3. `mosaic-plugin/examples/adapters/README.md` is no longer the Phase 4B placeholder — it's the adapter index.
> 4. Each adapter installs cleanly: `cd <adapter-dir> && pip install -e .` (or `uv pip install -e .`) succeeds.
> 5. Each adapter's 3-run sample yields ≥ 2 runs that converge to YAML passing validate/lint/test.
> 6. Both adapters use the same plugin content (no provider-specific tweaks in `skills/` / `agents/` / `commands/`).
> 7. Plugin's `skills/`, `agents/`, `commands/`, `.claude-plugin/`, `.mcp.json`, `examples/models/`, `hooks/` unchanged (`git diff phase-4a-mosaic-plugin` for those paths returns 0 lines).
> 8. Rust workspace unchanged (`git diff phase-4a-mosaic-plugin -- crates/` returns 0 lines).
> 9. `cargo test --workspace` still produces 416 / 0 at HEAD.
> 10. End-to-end transcripts captured in `docs/reports/phase-4b-proof/`:
>     - `transcript-anthropic.md` (3 runs: commands + LLM responses + iteration counts per run + per-run pass/fail summary + observed flake rate)
>     - `transcript-openai.md` (same shape, 3 runs)
>     - `output-anthropic.yaml` (the **first passing** YAML the Anthropic adapter produced — the canonical output for inspection)
>     - `output-openai.yaml` (same — first passing run from OpenAI adapter)
>     - **If a run failed:** preserve the failure transcript inline in the per-adapter transcript (don't delete failed runs from the audit record). If a 3rd-run final-failure YAML is informative, save as `output-anthropic-run-N.failed.yaml` (or similar).
> 11. Documented divergences between the passing outputs: a short section in the completion report comparing structural choices each LLM made (e.g., "Anthropic chose channels X/Y/Z; OpenAI chose A/B/C; both pass all gates"). Plus the observed per-adapter flake rate (e.g., "Anthropic 3/3, OpenAI 2/3 — within best-of-3 gate"). This is for audit, not gate.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0 — should be unchanged from Phase 4A)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0 — unchanged)
> - `cargo build --release --workspace` (zero warnings — unchanged)
> - `cargo test --workspace` (416 / 0 — unchanged)
> - `git diff phase-4a-mosaic-plugin -- crates/ mosaic-plugin/skills/ mosaic-plugin/agents/ mosaic-plugin/commands/ mosaic-plugin/.claude-plugin/ mosaic-plugin/.mcp.json mosaic-plugin/examples/models/ mosaic-plugin/hooks/` (zero lines)
> - `cd mosaic-plugin/examples/adapters/anthropic-python && python -c "import anthropic; print('SDK loadable')"` (smoke check)
> - `cd mosaic-plugin/examples/adapters/openai-python && python -c "import openai; print('SDK loadable')"` (smoke check)
> - **3 acceptance runs against the Anthropic adapter** (best-of-3 gate): `for i in 1 2 3; do python mosaic-plugin/examples/adapters/anthropic-python/author.py "<acceptance prompt>" --output run-${i}.yaml; done` — then for each `run-${i}.yaml` that exists (a converged run), verify `mc model validate run-${i}.yaml && mc model lint run-${i}.yaml && mc model test run-${i}.yaml` (all exit 0). **At least 2 of the 3 must pass all three gates.**
> - **3 acceptance runs against the OpenAI adapter** (same pattern). At least 2/3 must pass.
> - Save the first passing YAML from each adapter as `docs/reports/phase-4b-proof/output-{anthropic,openai}.yaml`.
>
> **Documentation requirements:**
>
> - Append `docs/reports/phase-4b-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Capture both end-to-end transcripts at `docs/reports/phase-4b-proof/transcript-anthropic.md` + `transcript-openai.md` + the two output YAMLs.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 4B from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 4B status row.
> - **Do NOT modify ADR-0008** or any earlier ADR. If a strategic concern surfaces, write a SPEC QUESTION and pause.
> - **Do NOT modify the brief, engine-semantics doc, or any spec.** Locked.
> - **Do NOT modify CLAUDE.md.** Operating manual; not a Phase 4B deliverable.
> - **Do NOT modify Phase 4A artifacts** (the plugin, the completion report, the proof transcript). They are sealed at `phase-4a-mosaic-plugin`.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
>
> 1. **Plugin-content path resolution** — `Path(__file__).resolve().parents[3]` doesn't land on `mosaic-plugin/`. The expected layout is `mosaic-plugin/examples/adapters/anthropic-python/author.py` so `parents[3]` should be `mosaic-plugin/`. If the plugin's directory layout has changed (rare), surface before adapting.
>
> 2. **The plugin's content has a bug** that materially affects iteration convergence (e.g., a skill example references a YAML field that doesn't exist; an agent's system prompt is contradictory). Phase 4A flagged this exact pattern as a feature of the design (the iteration loop catches teaching-material bugs). For Phase 4B: surface ANY plugin-content bug as a SPEC QUESTION + a Phase 4A.1 follow-up commit (do NOT fold the plugin fix into the 4B implementation commit; the plugin is locked).
>
> 3. **`mc` not on PATH at runtime.** The adapter shells out via subprocess. If `mc` isn't installed (`which mc` returns nothing), document the failure mode + remediation (`cargo install --path crates/mc-cli`) in the adapter's README; don't try to auto-build. If a CI environment without `mc` is the use case, surface — that's a future demand-driven concern.
>
> 4. **Both adapters consistently fail to converge** within 5 iterations on the canonical acceptance prompt. This means either (a) the plugin's content is insufficient for LLM authoring (Phase 4A bug) or (b) the iteration-feedback shape is wrong. Surface BEFORE bumping `max_iterations` past 5.
>
> 5. **`author.py` exceeds ~250 lines.** The ~150-line target is loose (Phase 4A trigger #10 set the precedent: budgets are intent, not hard caps). At 250 lines, surface to confirm scope hasn't ballooned. Specifically, if you find yourself writing helper modules (multiple `.py` files per adapter), STOP — each adapter should be a single self-contained `author.py` for Phase 4B (reference quality), not a multi-module package.
>
> 6. **An adapter fails the best-of-3 gate (≤ 1/3 runs converge).** Both adapters must pass the 2/3 threshold for Phase 4B acceptance. If one provider consistently produces 0/3 or 1/3 (vs the expected 2/3 or 3/3), that's NOT garden-variety flake — it's evidence of an LLM-specific limitation OR a real adapter bug. Surface as a SPEC QUESTION before assuming either; the rollback plan #1 ("ship only the working adapter") is the LAST resort, not the first response. **Common false positives to rule out before opening the SPEC QUESTION:** check whether `mc` is on PATH on the gate-run machine; check whether the API key has sufficient credit/rate-limit headroom; check whether the model string in `author.py` matches a model the SDK actually supports (per trigger #8). If 2/3 passes once, runs are operating normally — stochastic 1/3 outcomes within a 3-run sample are expected.
>
> 7. **API streaming required for response-size reasons.** Default is non-streaming (single response). If the LLM consistently truncates output mid-YAML because the response cap is too small, surface — bumping `max_tokens` is fine; switching to streaming is a SPEC QUESTION.
>
> 8. **Provider model selection.** Default to current frontier models. The 2026-05-03 inline examples in scope item 4 use `claude-opus-4-7` (Anthropic) and `gpt-5.5` (OpenAI). **Before pasting either string, verify it's still current via `web_search` or by reading the Anthropic API docs + OpenAI Platform docs.** Both providers ship new flagship models on roughly quarterly cadence; the strings here may be ≥ 1 month stale by your execution date. If the SDK provides a current default (e.g., `model="auto"` or omitting the param), use it. Do NOT pin a deprecated/legacy model. If you can't determine the current top model with high confidence, surface as a SPEC QUESTION rather than guessing.
>
> 9. **Token budget exceeded** (provider returns "context window full" or similar). The plugin content is the dominant input; if it doesn't fit alongside the user prompt + iteration history, that's a real problem. Surface — possible mitigations include selectively loading only relevant skills, but that contradicts the "feed the full plugin" simplicity Phase 4B aims for.
>
> 10. **The acceptance YAML structurally diverges between the two adapters in a way that suggests one is "more correct"** (e.g., one adapter authors a clean 5-channel model, the other authors a 5-channel model that technically passes but is awkward). Decision: both pass = both correct. Document the divergence in the completion report; don't try to "fix" the worse one by tweaking the plugin (per trigger #2).
>
> **Rollback plan (in case complexity explodes):**
>
> **First, distinguish flake from real failure.** A 1/3 result on a single best-of-3 run is flake; a consistent 0/3 or 1/3 across multiple gate-run sessions on the same day is real failure. The best-of-3 gate is precisely designed to absorb flake without rolling back; do NOT take a rollback path on the first poor run.
>
> If real failure is established (≥ 2 separate gate-run sessions both producing < 2/3 on the same adapter), OR the SDK + plugin-content combo blows the context window, OR `author.py` balloons past ~400 lines, **stop and write a SPEC QUESTION**. Two recovery paths:
>
> 1. **Phase 4B.1 narrowing**: ship ONLY the working adapter (likely `anthropic-python/` per amendment D's default-provider preference). The other adapter becomes a Phase 4B.2 amendment, scoped against the specific failure mode observed. Reduces scope by half; still proves portability via at least one working consumer of the plugin. **This is the response to confirmed real failure on one provider, NOT to flake.**
>
> 2. **Phase 4B.0 prep work**: ship the plugin-loading library + a "dry run" mode that builds the system prompt + writes it to disk WITHOUT calling any provider API. Lets users inspect the prompt the adapter would send. Real provider integration becomes Phase 4B.1. Use this only if the system prompt itself is the suspected problem (e.g., context-window blow-up, garbled prompt structure).
>
> Either fallback is a Phase 4B.1 amendment, not a Phase 4B scope rewrite.
>
> **Completion report format:**
>
> ```
> DONE: Phase 4B Python Reference Adapters
>
> Build:    cargo build --release --workspace ✓ (unchanged)
> Format:   cargo fmt --check --all ✓ (unchanged)
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓ (unchanged)
> Tests:    cargo test --workspace 416 / 0 (unchanged from Phase 4A)
> Locked surfaces:
>   git diff phase-4a-mosaic-plugin -- crates/ ✓ 0 lines
>   git diff phase-4a-mosaic-plugin -- mosaic-plugin/skills/ mosaic-plugin/agents/ mosaic-plugin/commands/ \
>     mosaic-plugin/.claude-plugin/ mosaic-plugin/.mcp.json mosaic-plugin/examples/models/ mosaic-plugin/hooks/ ✓ 0 lines
> Anthropic adapter (best-of-3):
>   pip install -e mosaic-plugin/examples/adapters/anthropic-python ✓
>   Run 1: python author.py "<prompt>" → converged in N iter; validate/lint/test ✓ (or ✗ + reason)
>   Run 2: ... (same shape)
>   Run 3: ... (same shape)
>   PASSING RUNS: <count>/3 (gate: ≥ 2/3)
>   Canonical output: docs/reports/phase-4b-proof/output-anthropic.yaml (first passing run)
> OpenAI adapter (best-of-3):
>   (same shape; <count>/3 passing)
> Cross-adapter consistency:
>   Both adapters use identical plugin content ✓ (no provider-specific tags in skills/agents/commands)
>   Both adapters meet best-of-3 gate (≥ 2/3 passing per provider) ✓
>   Structural divergences between passing outputs documented in completion report §X
>   Observed flake rates documented in completion report §X (e.g., "Anthropic 3/3, OpenAI 2/3")
>
> Source manifest:
> - mosaic-plugin/examples/adapters/anthropic-python/pyproject.toml   (NEW)
> - mosaic-plugin/examples/adapters/anthropic-python/README.md        (NEW)
> - mosaic-plugin/examples/adapters/anthropic-python/author.py        (NEW — N lines)
> - mosaic-plugin/examples/adapters/openai-python/pyproject.toml      (NEW)
> - mosaic-plugin/examples/adapters/openai-python/README.md           (NEW)
> - mosaic-plugin/examples/adapters/openai-python/author.py           (NEW — N lines)
> - mosaic-plugin/examples/adapters/README.md                         (modified — Phase 4B placeholder → adapter index)
> - docs/reports/phase-4b-completion-report.md                        (NEW)
> - docs/reports/phase-4b-proof/transcript-anthropic.md               (NEW)
> - docs/reports/phase-4b-proof/transcript-openai.md                  (NEW)
> - docs/reports/phase-4b-proof/output-anthropic.yaml                 (NEW — proof artifact)
> - docs/reports/phase-4b-proof/output-openai.yaml                    (NEW — proof artifact)
> - docs/CURRENT_STATE.md                                             (modified — flip 4B proposed → complete)
> - docs/roadmap/MASTER_PHASE_PLAN.md                                 (modified — flip 4B complete; "what's next" now Phase 5)
>
> Implementation summary:
>   <one paragraph: plugin-content loading approach, system prompt construction, provider call shape, iteration loop tuning, divergence observations between Claude and OpenAI outputs>
>
> Deviations:
>   <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. Where to find Phase 4A's plugin content for inspection

Read these BEFORE writing any prompt-construction logic:

- `mosaic-plugin/.claude-plugin/plugin.json` — manifest shape (commands array, agents array, skills auto-discovered)
- `mosaic-plugin/skills/authoring/SKILL.md` — the canonical end-to-end YAML authoring skill; this is the most important content for adapters to surface
- `mosaic-plugin/skills/debugging/SKILL.md` — the diagnostic-code registry; the LLM consults this when iterating on validate/lint errors
- `mosaic-plugin/skills/formulas/SKILL.md` — Phase 3D formula syntax; the LLM needs this to author rule bodies correctly
- `mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md` — the only domain skill; defines the canonical Acme shape
- `mosaic-plugin/agents/mosaic-architect.md` — the agent system prompt for "natural language → schema design"
- `mosaic-plugin/agents/mosaic-author.md` — the agent system prompt for "schema design → YAML"
- `mosaic-plugin/examples/models/acme-marketing.yaml` — the canonical reference YAML for in-context few-shot

The adapter's `system_prompt` should include all of the above (or a concatenated representative subset). Phase 4A's design intent: one big system prompt = whole plugin loaded; no progressive-disclosure complexity in the adapters (that's Claude Code's native feature, not a Python concern).

### B. The Phase 4A in-session proof transcript (reference for what "good" looks like)

Read `docs/reports/phase-4a-proof/transcript.md` — Phase 4A's implementer ran exactly this kind of iteration loop in-session (manually) to prove the plugin worked. The Phase 4B adapters automate that loop. The transcript shows: how the iteration unfolded, what the diagnostic feedback looked like, where the LLM caught and fixed its own bugs (e.g., `canonical_inputs` shape).

The Phase 4B output should look structurally similar to `docs/reports/phase-4a-proof/myco_marketing_q1_2026.yaml` (the proof YAML), though the specifics will differ (different prompt, different LLM, possibly different channels/markets).

### C. The diagnostic JSON envelope the iteration loop consumes

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC2003",
      "severity": "error",
      "path": "/rules/0/declared_dependencies",
      "message": "rule 'clicks_rule' references unknown measure 'CPCC' (did you mean 'CPC'?)",
      "suggestion": "change declared_dependencies entry from 'CPCC' to 'CPC'"
    }
  ]
}
```

Sorted by `(severity desc, code asc, yaml_pointer asc, message asc)` — deterministic across runs. This is the contract from Phase 3B; Phase 4B doesn't modify it.

The adapter's diagnostic-feedback message to the LLM should preserve the structure (don't flatten to prose) so the LLM can reason over codes individually:

```
The YAML you produced has 2 errors:

[1] MC2003 (error) at /rules/0/declared_dependencies:
    rule 'clicks_rule' references unknown measure 'CPCC' (did you mean 'CPC'?)
    Suggested fix: change declared_dependencies entry from 'CPCC' to 'CPC'

[2] MC2010 (error) at /measures/3:
    measure 'CVR' has no aggregation rule
    Suggested fix: add 'aggregation: WeightedAverage(weight: Spend)' or similar

Please respond with a corrected YAML. Same format as before — full file, in a yaml code block.
```

### D. The canonical acceptance prompt and why it's specific

The acceptance prompt is *"marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"* — chosen because:

- **5-channel** forces the LLM to invent reasonable channel names (Paid_Search, Paid_Social, Display, Email, Organic — a common B2C SaaS marketing mix).
- **B2C SaaS** anchors the domain so the LLM picks reasonable markets (US/CA/EU; or by tier).
- **monthly seasonality** forces the Time dimension to have monthly granularity (12 elements) AND requires the LLM to think about hierarchies (months → quarters → year).
- **Q4 lift scenario** forces the Scenario dimension to have at least Plan + Q4_Lift; tests whether the LLM understands scenario semantics.

This prompt is intentionally DIFFERENT from Acme (3 channels × 3 markets × Q1) so the LLM can't just copy the example verbatim. Phase 4A's in-session proof used a similar prompt and produced `MyCo_Marketing_Q1_2026`; Phase 4B's outputs should be structurally similar (a valid marketing-mix model) but with the differences that match this prompt's specifics.

If the adapter produces a model that doesn't match the prompt's specifics (e.g., 3 channels instead of 5, or no scenario), that's a soft signal the system prompt isn't communicating intent well — surface as a divergence note in the completion report.

### E. Python conventions for this project (first-time)

Phase 4B is the project's first Python code. Conventions:

- **Python version:** ≥ 3.10. Type hints expected (`list[str]` not `List[str]`; `X | None` not `Optional[X]`). Match modern Python style.
- **Package management:** `pyproject.toml` per PEP 621. Users install via `pip install -e .` or `uv pip install -e .` (whichever they prefer; both work). Do NOT add `requirements.txt`, `setup.py`, or `setup.cfg`.
- **Test framework:** none required. If you add Python tests, use stdlib `unittest` or `pytest` (declare it in `pyproject.toml` `[project.optional-dependencies] test = ["pytest"]`). Tests are optional in Phase 4B.
- **Linting:** none required. If you want `ruff` or `black` — fine, add to optional dev deps; don't make them required.
- **Logging:** stdlib `logging` if needed. Default to `print()` for the iteration-loop progress (it's a CLI tool; users want to see what's happening).
- **No frameworks:** no Click, no Typer, no Rich, no Pydantic, no requests, no httpx, no asyncio. `argparse` from stdlib for CLI args; `subprocess.run` for `mc` calls; the SDK's built-in client for provider calls.
- **File reading:** `pathlib.Path`, not `os.path`.
- **Error handling:** specific exceptions, not bare `except:`. Surface diagnostic content; don't swallow.

### F. Why subprocess and not native MCP

The Phase 4A `mc mcp` subcommand surfaces 5 tools as MCP tool calls — but consuming MCP from Python requires an MCP client library (one would need `mcp` SDK or similar). That's:

1. A new Python dep beyond the SDKs.
2. A tighter coupling (Python adapter and Rust MCP server both need to agree on the wire format).
3. More moving parts to debug.

Subprocess via `mc model {validate,lint,test} --format json` is:

1. Just stdlib `subprocess`.
2. The same JSON envelope the MCP server returns (just over a different transport).
3. Trivially debuggable (`mc model validate <path>` is the same command users run by hand).

The MCP server is the right tool for Claude Code (which has native MCP support). Subprocess is the right tool for Python adapters (which don't, by default). If a future demand-driven phase needs Python+MCP (e.g., a customer wants Python adapters that can be loaded as MCP servers themselves), that's a separate phase.

### G. Where to capture the proof transcripts (best-of-3 layout)

Mirror Phase 4A's `docs/reports/phase-4a-proof/` pattern at `docs/reports/phase-4b-proof/`, expanded to capture the best-of-3 run set:

```
docs/reports/phase-4b-proof/
├── transcript-anthropic.md           # 3 runs documented inline; per-run pass/fail; final flake rate
├── transcript-openai.md              # same shape, 3 runs
├── output-anthropic.yaml             # first PASSING run's YAML (the canonical output for inspection)
├── output-openai.yaml                # same — first passing run from OpenAI adapter
└── output-{anthropic,openai}-run-N.failed.yaml   # OPTIONAL: failed-run YAMLs IF they're informative
                                                  # (e.g., near-converged but final-iteration failed)
                                                  # Do NOT save trivial failures (immediate parse errors)
```

Each transcript should include, **per run** (3 sections per file):

- The exact `python author.py "..."` invocation (env + args)
- API call summary (model used, system prompt size, max_tokens) — but NEVER the API key
- Each iteration's diagnostic envelope (compact form OK)
- Convergence outcome (iterations used, final state — pass/fail)
- Final `mc model validate / lint / test` output (only if converged)

And, **once per file at the end**:

- Per-adapter result tally (e.g., "Run 1: PASS (3 iter), Run 2: FAIL (5 iter, MC2003 unresolved), Run 3: PASS (2 iter) → 2/3 ✓")
- Best-of-3 gate verdict (✓ ≥ 2/3, OR ✗ < 2/3 → SPEC QUESTION)
- A short paragraph on what the LLM did well across the 3 runs + observed flake patterns + any surprises

Don't include raw API responses if they're too large; summarize. The point of the transcript is auditability, not full replay. The flake-rate data is genuinely valuable — it's the project's first measured signal of LLM-authoring reliability against the Phase 4A plugin.

### H. Output verification harness (recommended but not required)

If you want a small repeatable harness for the gate run, drop a shell script at `docs/reports/phase-4b-proof/verify.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Verifies the canonical (first-passing) YAML per adapter passes all three gates.
# The 3-run best-of-3 evidence is captured in transcript-{anthropic,openai}.md;
# this script just re-checks the persisted canonical outputs.
for adapter in anthropic openai; do
  yaml="output-${adapter}.yaml"
  echo "=== Verifying ${adapter} canonical output: ${yaml} ==="
  mc model validate "${yaml}"
  mc model lint "${yaml}"
  mc model test "${yaml}"
  echo "✓ ${adapter} canonical YAML passes all three gates"
done
echo "(Best-of-3 audit lives in transcript-{anthropic,openai}.md; this script verifies the persisted canonical outputs only.)"
```

The two passing-YAML proof artifacts + the per-adapter transcripts are the auditable record; this script makes re-checking the persisted YAMLs trivially repeatable. **It does NOT re-run the LLM gate** — the API costs of re-running a 3×2 best-of-3 against frontier models are non-trivial. The transcript IS the audit; this script confirms the persisted outputs haven't bit-rotted between commit and review.

---

## Pointers to existing files you will most likely touch

| Why | File | Phase 4B action |
|---|---|---|
| Anthropic adapter manifest | `mosaic-plugin/examples/adapters/anthropic-python/pyproject.toml` | new — `anthropic` SDK dep, Python ≥ 3.10 |
| Anthropic adapter README | `mosaic-plugin/examples/adapters/anthropic-python/README.md` | new — install + usage instructions |
| Anthropic adapter entry point | `mosaic-plugin/examples/adapters/anthropic-python/author.py` | new — ~150-line iteration loop (Anthropic SDK) |
| OpenAI adapter manifest | `mosaic-plugin/examples/adapters/openai-python/pyproject.toml` | new — `openai` SDK dep, Python ≥ 3.10 |
| OpenAI adapter README | `mosaic-plugin/examples/adapters/openai-python/README.md` | new — install + usage instructions |
| OpenAI adapter entry point | `mosaic-plugin/examples/adapters/openai-python/author.py` | new — ~150-line iteration loop (OpenAI SDK) |
| Adapter index | `mosaic-plugin/examples/adapters/README.md` | modify — replace Phase 4B placeholder with adapter list |
| Phase 4B completion report | `docs/reports/phase-4b-completion-report.md` | new (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Proof transcripts + outputs | `docs/reports/phase-4b-proof/` | new dir (4 files: 2 transcripts + 2 YAMLs; optional `verify.sh`) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 4B from `proposed` → `complete` |

**Do not touch:**

- **`crates/`** — entire workspace locked. Zero source changes.
- **`mosaic-plugin/skills/`, `agents/`, `commands/`, `.claude-plugin/`, `.mcp.json`, `examples/models/`, `hooks/`** — the entire Phase 4A plugin content is locked. Adapters READ this content; they do NOT modify it.
- **`mosaic-plugin/README.md`** — Phase 4A's plugin-level README; unchanged.
- **`docs/specs/`** — locked.
- **`docs/decisions/0001-*` through `0009-*`** — Accepted; amendments go in `<NNNN>-amendment-N.md`.
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock`** — pins all stay.
- **PERF.md** — Phase 4B doesn't touch performance. The kernel didn't change.
- **The `Diagnostic` struct shape** — adding codes is fine; struct fields are not. Phase 4B adds NO new codes.
- **MC3008** — permanently retired.
- **Phase 4A artifacts**: `docs/reports/phase-4a-completion-report.md`, `docs/reports/phase-4a-proof/`, `docs/handoffs/phase-4a-handoff.md` — sealed at `phase-4a-mosaic-plugin`.
- **CLAUDE.md** — operating manual; not a Phase 4B deliverable.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

source $HOME/.cargo/env

# Pre-4B gate — must remain green throughout
cargo build --release --workspace                                                      # zero warnings
cargo fmt --check --all                                                                # exit 0
cargo clippy --workspace --all-targets -- -D warnings                                  # exit 0
cargo test --workspace                                                                 # 416 / 0
cargo run --release --bin mc -- demo                                                   # matches brief §4.6
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml          # zero warnings
cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml          # 9/9 goldens

# `mc` install precondition (one-time; same as Phase 4A)
which mc || cargo install --path crates/mc-cli --locked
mc --version

# Verify locked surfaces — must remain empty throughout
git diff phase-4a-mosaic-plugin -- crates/ \
  mosaic-plugin/skills/ mosaic-plugin/agents/ mosaic-plugin/commands/ \
  mosaic-plugin/.claude-plugin/ mosaic-plugin/.mcp.json \
  mosaic-plugin/examples/models/ mosaic-plugin/hooks/
# expected: zero output

# Adapter dev (Anthropic)
cd mosaic-plugin/examples/adapters/anthropic-python
pip install -e .                                                # OR: uv pip install -e .
export ANTHROPIC_API_KEY=<your-key>
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"
# → produces output.yaml
mc model validate output.yaml
mc model lint output.yaml
mc model test output.yaml

# Adapter dev (OpenAI)
cd ../openai-python
pip install -e .
export OPENAI_API_KEY=<your-key>
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"
mc model validate output.yaml
mc model lint output.yaml
mc model test output.yaml

# Reference: Phase 4A's in-session proof
cat docs/reports/phase-4a-proof/transcript.md   # what manual iteration looked like
cat docs/reports/phase-4a-proof/myco_marketing_q1_2026.yaml   # what a "good" output looks like
```

---

## Final checklist before you call Phase 4B done

- [ ] `mosaic-plugin/examples/adapters/anthropic-python/` exists with `pyproject.toml`, `README.md`, `author.py`.
- [ ] `mosaic-plugin/examples/adapters/openai-python/` exists with the same three files.
- [ ] **Both `pyproject.toml` files include `requires-python = ">=3.10"`.**
- [ ] `mosaic-plugin/examples/adapters/README.md` is the adapter index, not the Phase 4B placeholder.
- [ ] Each adapter installs cleanly: `pip install -e .` succeeds.
- [ ] **Each adapter ran 3 times against the canonical acceptance prompt; ≥ 2/3 runs per adapter converged to YAML passing `mc model validate / lint / test`.** Transcripts capture all 6 runs (successes + failures).
- [ ] **The system prompt explicitly instructs the LLM to emit YAML in a single ```yaml fenced block with no surrounding prose.**
- [ ] **Model strings in `author.py` verified current via `web_search` or provider-docs check at execution time** (handoff-snapshot strings: `claude-opus-4-7` for Anthropic, `gpt-5.5` for OpenAI, both as of 2026-05-03).
- [ ] Plugin's `skills/`, `agents/`, `commands/`, `.claude-plugin/`, `.mcp.json`, `examples/models/`, `hooks/` unchanged (`git diff phase-4a-mosaic-plugin` for those paths returns 0 lines).
- [ ] Rust workspace unchanged (`git diff phase-4a-mosaic-plugin -- crates/` returns 0 lines).
- [ ] All 416 existing Rust tests still pass.
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release --workspace` all clean.
- [ ] No new Rust deps in any crate.
- [ ] Each adapter has exactly ONE first-party Python dep (`anthropic` or `openai`) — no `pyyaml`, `pydantic`, `httpx`, `requests`, `click`, `typer`, `rich`, etc.
- [ ] No async, no concurrency, no streaming, no retries (beyond SDK auto-retry), no rate limiting, no telemetry, no cost tracking.
- [ ] Both adapters use the same plugin content (no provider-specific tags in `skills/` / `agents/` / `commands/`).
- [ ] No new diagnostic codes; `schema_version` stays `"1.0"`.
- [ ] Marketing-mix is the ONLY domain exercised in the proof transcripts.
- [ ] `docs/reports/phase-4b-proof/` contains: `transcript-anthropic.md`, `transcript-openai.md`, `output-anthropic.yaml`, `output-openai.yaml`. Optionally `verify.sh`.
- [ ] Completion report at `docs/reports/phase-4b-completion-report.md` written from template; includes structural-divergence comparison between the two adapter outputs.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 4B from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 5 (actuals), Phase 4A.1 (hooks), Phase 4A.2 (mc model trace), or any other phase.**
- [ ] **You did NOT add a third adapter** (no TypeScript, Codex, Gemini, Mistral, Ollama, etc.).
- [ ] **You did NOT modify the plugin's content** (skills/agents/commands/etc.). If you discovered a plugin-content bug, it's a Phase 4A.1 SPEC QUESTION + separate commit, NOT folded in.
- [ ] **You did NOT modify ADR-0008 or any earlier ADR.**

If you are uncertain at any point, the resolution order is:

1. The Phase 4B prompt above.
2. **ADR-0008** — the strategic gate. Read amendments A, D, G specifically (the Phase 4B-relevant ones) plus the Strategic centerpiece section.
3. **Phase 4A's plugin content** at `mosaic-plugin/skills/`, `agents/`, `commands/` — the content the adapters consume.
4. **Phase 4A's in-session proof transcript** at `docs/reports/phase-4a-proof/transcript.md` — what manual iteration looked like.
5. ADR-0004 / ADR-0005 / ADR-0006 / ADR-0007 — the schema + diagnostic + formula contracts the LLM emits against.
6. The current Anthropic / OpenAI Python SDK docs (cross-referenced at execution time for current model names + API shapes).
7. `CLAUDE.md` (operating manual).
8. `docs/strategy/POSITIONING.md` (Mosaic-as-LNM framing).
9. `docs/process-notes.md` (carry-forward rules — including §1 self-test for handoff-first).
10. `docs/roadmap/MASTER_PHASE_PLAN.md`.
11. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 4A)

**Read this handoff fully + read ADR-0008 amendments A, D, G + browse the plugin content before writing any Python.** The strategic context is in ADR-0008; the implementation context is in the plugin content the adapters will consume. Both matter.

**The plugin is the deliverable. The adapters are the proof.** If you spend 80% of your time on the iteration-loop logic and 20% on understanding what the plugin teaches, you're doing it backwards. The adapter's job is to faithfully feed plugin content to the LLM and faithfully feed diagnostic feedback back. The smarter the iteration logic gets, the more it's hiding bugs in the plugin content.

**Source-bounded.** Phase 4B touches `mosaic-plugin/examples/adapters/anthropic-python/` (NEW), `mosaic-plugin/examples/adapters/openai-python/` (NEW), and `mosaic-plugin/examples/adapters/README.md` (modify). Nothing else under `mosaic-plugin/`. Nothing under `crates/`. The 0-line diff against `phase-4a-mosaic-plugin` for the locked paths is a hard gate.

**The acceptance gate is "both adapters produce passing YAML from the same prompt."** Same plugin, two providers, two valid Mosaic models, both pass validate/lint/test. That's the portability proof. Anything that requires provider-specific plugin content invalidates the proof.

**Diagnostic codes are forever.** MC1xxx–MC4xxx semantics are stable. Phase 4B adds zero codes. If the iteration loop surfaces a need for a new code (rare), that's a SPEC QUESTION (it would mean modifying `mc-model`, which is locked).

**Hand-rolled wins (Python edition).** No `pydantic`, `click`, `typer`, `rich`, `httpx`, `pytest`-required, `loguru`, etc. Use stdlib + the SDK. Each adapter is one `author.py` plus its `pyproject.toml` + README. If you reach for a Python framework, ask first.

**Subprocess, not MCP.** Adapters call `mc model ...` via `subprocess.run`. Native Python MCP integration is a future demand-driven phase if a real customer needs it. Phase 4B's MCP story is "Claude Code uses it natively (Phase 4A); Python uses subprocess (Phase 4B); they both consume the same diagnostic envelope shape."

**Marketing-mix only.** Per ADR-0008 amendment F. The acceptance prompt is marketing-mix-shaped; the adapters can ONLY produce a marketing-mix model with confidence; that's the scope. Future domain support is demand-driven.

**Do not pick the next phase.** Phase 4B's deliverable is the two adapters. If the work surfaces opportunities for Phase 4A.1 (hooks), Phase 4A.2 (trace verb), Phase 5 (actuals), or new domain schemas, note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 4B handoff drafted 2026-05-03 immediately after [Phase 4A](../reports/phase-4a-completion-report.md) shipped at `36af56c` (tag `phase-4a-mosaic-plugin`). Per [`../process-notes.md`](../process-notes.md) §1's 5-question self-test, Phase 4B is eligible for the handoff-first parallel flow (all 5 questions yes — see "Self-test result" §above). ADR-0010 (if any new strategic surface emerges during 4B) drafts in parallel; this handoff is the binding contract until that happens.*
