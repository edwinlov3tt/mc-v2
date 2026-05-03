# Adapters — Phase 4B placeholder

This directory is **reserved for Phase 4B**. Phase 4A does not ship adapter code.

Phase 4B (per [ADR-0008](../../../docs/decisions/0008-phase-4-llm-authoring-and-plugin-ecosystem.md) Decision 7, amendment A + G) will land two Python reference adapters here:

```
examples/adapters/
├── anthropic-python/      # ~150-line iteration loop using the Anthropic SDK
│   ├── README.md
│   ├── pyproject.toml
│   └── author.py
└── openai-python/         # ~150-line iteration loop using the OpenAI SDK
    ├── README.md
    ├── pyproject.toml
    └── author.py
```

Each adapter:

1. Reads this plugin's `skills/`, `agents/`, `commands/`, and `examples/` content as plain markdown — no provider-specific tags, no plugin-specific transport.
2. Translates the content into provider-specific API calls (system prompts, tool-use specs, function-calling specs).
3. Runs the iteration loop against `mc-cli`'s diagnostic JSON envelope: emit YAML → run `mc model validate/lint/test --format json` → feed `{schema_version, diagnostics}` back to the LLM → iterate to convergence (default 5 iterations).
4. Produces a working Mosaic YAML that passes `mc model validate / lint / test`.

**Default provider for Phase 4B is Anthropic** per ADR-0008 amendment D. OpenAI is the cross-provider proof.

**Out of scope for Phase 4B** (per amendment G): TypeScript adapters, Codex / Gemini / Mistral / Ollama adapters, cost tracking, prompt hardening, schema marketplace. Each is its own demand-driven future phase.

The plugin's `skills/` and `agents/` are designed to be portable across providers — same content, same diagnostic codes, same iteration shape. Phase 4B is the "any agent can author Mosaic" proof; Phase 4A is the institutional knowledge it consumes.
