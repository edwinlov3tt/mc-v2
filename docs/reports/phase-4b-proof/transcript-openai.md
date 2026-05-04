# Phase 4B end-to-end proof transcript — OpenAI adapter (best-of-3)

**Date:** 2026-05-03
**Operator:** Edwin Lovett III (driving) + Claude Opus 4.7 (1M context) (implementer)
**Adapter:** [`mosaic-plugin/examples/adapters/openai-python/`](../../../mosaic-plugin/examples/adapters/openai-python/)
**Model:** `gpt-5.5` (verified current 2026-05-03 via `web_search`)
**Prompt:** *"marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"*
**Iteration cap:** `--max-iterations 5` (default)
**Strict mode:** off (lint warnings advisory; only validate-errors + test-failures block)

Each run is a **fresh Python process** — fresh SDK client, fresh `messages = [{"role": "user", "content": prompt}]` list, fresh `max_iter=5` budget. No state shared across runs.

Per-run artifacts in this directory:

- `run-openai-1.log` / `run-openai-1.yaml` (converged in 1 iter)
- `run-openai-2.log` / `run-openai-2.yaml` (converged in 1 iter)
- `run-openai-3.log` / `run-openai-3.yaml` (converged in 1 iter)

The first passing run's YAML is copied to `output-openai.yaml` as the canonical artifact for inspection.

---

## Setup

The 3 OpenAI runs were the second half of the initial 6-call best-of-3 driven by [`run-gate.sh`](run-gate.sh). System prompt size at gate-run time: 138,162 chars (~34K tokens).

```bash
export ANTHROPIC_API_KEY=...   # set, never logged
export OPENAI_API_KEY=...
cd docs/reports/phase-4b-proof
./run-gate.sh
```

The OpenAI adapter uses the `responses.create` API with `input=[{"role": "system", ...}, {"role": "user", ...}]`. `output_text` is the consolidated assistant response (the YAML body, possibly with surrounding prose that the adapter strips via the ```yaml fence regex).

### Note on adapter bugs (post-hoc — see anthropic transcript for detail)

The initial gate-run revealed two adapter bugs (case-insensitive severity filter mismatch + brittle YAML extraction on truncated responses). Both bugs **were dormant in OpenAI's 3 runs** because GPT-5.5 happened to author error-free YAMLs on the first try in every run — the case-mismatch never had errors to filter, and the response cap was sufficient. The bug fixes were applied prospectively to `mosaic-plugin/examples/adapters/openai-python/author.py` (case-insensitive comparison + truncation-tolerant fence regex) so future re-runs will catch errors correctly if they arise. The 3 persisted YAMLs are valid Mosaic models verified by [`verify.sh`](verify.sh) — the dormant bugs do not invalidate them.

The 3 OpenAI logs and YAMLs were captured under the original adapter; we did not re-run them with the fixed adapter (would burn ~$1 of API credit for no behavioral change in the convergence outcome). If OpenAI runs are re-collected in a future session with the fixed adapter and a prompt that triggers errors, the iteration loop will work as designed.

---

## Run 1

### Invocation

```
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario" --output run-openai-1.yaml
```

### API call summary

- Model: `gpt-5.5`
- API: `client.responses.create(model=..., input=[...])`
- Calls made (initial draft only): 1
- Approximate input tokens: ~34K
- Approximate output tokens: ~5K

### Iteration history

| Iter | Stage | Result |
|---:|---|---|
| 1 | validate | clean |
| 1 | lint | clean |
| 1 | test | clean |

### Convergence outcome

- Status: `converged`
- Iterations used: 1 / 5
- Final state: validate clean, lint clean, test 10/10 goldens pass

### Final gate

```
mc model validate run-openai-1.yaml      # exit 0
mc model lint     run-openai-1.yaml      # exit 0
mc model test     run-openai-1.yaml      # exit 0; 10/10 goldens
```

### Notes / observations

Clean first-try author. The model named the cube `B2C_SaaS_Marketing_FY2027` and stuck very close to Acme's measure naming (CVR, Close_Rate, AOV, Leads, Customers) and structure. Authored a 3-city Market dim with a 1-region rollup (Acme-shaped), unlike Anthropic's single-element Market.

**Selected as the canonical output:** [`output-openai.yaml`](output-openai.yaml).

---

## Run 2

### Invocation

```
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario" --output run-openai-2.yaml
```

### API call summary

- Model: `gpt-5.5`
- API: `client.responses.create(model=..., input=[...])`
- Calls made (initial draft only): 1
- Approximate input tokens: ~34K
- Approximate output tokens: ~5K

### Iteration history

| Iter | Stage | Result |
|---:|---|---|
| 1 | validate | clean |
| 1 | lint | clean |
| 1 | test | clean |

### Convergence outcome

- Status: `converged`
- Iterations used: 1 / 5
- Final state: validate clean, lint clean, test goldens all pass

### Final gate

```
mc model validate run-openai-2.yaml      # exit 0
mc model lint     run-openai-2.yaml      # exit 0
mc model test     run-openai-2.yaml      # exit 0
```

### Notes / observations

Clean first-try author. Same structural choices as Run 1 (no major divergence within the same provider).

---

## Run 3

### Invocation

```
python author.py "marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario" --output run-openai-3.yaml
```

### API call summary

- Model: `gpt-5.5`
- API: `client.responses.create(model=..., input=[...])`
- Calls made (initial draft only): 1
- Approximate input tokens: ~34K
- Approximate output tokens: ~5K

### Iteration history

| Iter | Stage | Result |
|---:|---|---|
| 1 | validate | clean |
| 1 | lint | clean |
| 1 | test | clean |

### Convergence outcome

- Status: `converged`
- Iterations used: 1 / 5
- Final state: validate clean, lint clean, test goldens all pass

### Final gate

```
mc model validate run-openai-3.yaml      # exit 0
mc model lint     run-openai-3.yaml      # exit 0
mc model test     run-openai-3.yaml      # exit 0
```

### Notes / observations

Clean first-try author.

---

## Per-adapter result tally

| Run | Status | Iterations | Validate / lint / test |
|---:|---|---:|---|
| 1 | converged | 1 | ✓ / ✓ / ✓ (10/10 goldens) |
| 2 | converged | 1 | ✓ / ✓ / ✓ |
| 3 | converged | 1 | ✓ / ✓ / ✓ |

**Best-of-3 verdict:** **3/3 ✓** (gate: ≥ 2/3 required) — all three runs converged on the first try with no iteration needed.

**Canonical output (first passing run):** [`output-openai.yaml`](output-openai.yaml) (copied from `run-openai-1.yaml`).

**Observed iteration distribution:** 1 / 1 / 1 (mean 1.0). GPT-5.5 produced a clean YAML on the first try in every run.

---

## Observations

### What the LLM did well

- **First-try cleanliness on all 3 runs.** GPT-5.5 nailed the canonical Mosaic schema on the initial draft each time, no iteration needed. Lower observed flake rate than Anthropic on this prompt.
- **Acme-faithful naming.** Stuck with CVR / Close_Rate / AOV / Leads / Customers — the canonical marketing-mix vocabulary documented in [`skills/domain-schemas/marketing-mix/SKILL.md`](../../../mosaic-plugin/skills/domain-schemas/marketing-mix/SKILL.md). Compare Anthropic which reframed for SaaS terminology.
- **Multi-tier Market hierarchy.** Authored 3 cities (Tampa / Orlando / Miami or similar) → 1 North_America region. Closer to Acme's geographic depth than Anthropic's single-element placeholder.
- **Q4 lift via Scenario element.** Implemented the Q4 lift as a non-default scenario element with dedicated input cells, rather than as a test_fixture overlay (the Anthropic choice). Both are valid per the plugin; the scenario-element approach is more directly responsive to the prompt's "scenario" wording.

### Observed flake patterns

- **No flake observed in this 3-run sample.** All 3 runs converged on iteration 1.
- However, 3 runs is a small sample; a larger N would give more confidence in the observed 0% iteration rate. The handoff's best-of-3 design absorbs flake; this OpenAI adapter happened to pass with the maximum observable cleanliness in this sample.
- The dormant adapter bugs (case-mismatch in severity filter; truncation-fragile YAML extractor) were NOT triggered in any of the 3 runs because the YAMLs were error-free on first emission. If a future run produces an erroring YAML, the now-fixed adapter will iterate correctly.

### Surprises

- **Speed.** GPT-5.5 calls completed faster than Opus 4.7 calls (~15-30s vs ~30-90s for the initial draft). Cost asymmetry too — OpenAI runs are ~10× cheaper per token than Anthropic, so the OpenAI 3-run gate cost ~$0.30 vs Anthropic's ~$3-5.
- **Cardinality difference.** OpenAI authored a 53,856-cell cube (3 scenarios × 3 versions × 17 time × 8 channel × 4 market × 11 measure) vs Anthropic's 13,464 (Market=1). Both pass; the OpenAI shape is more "production-ish" but the prompt didn't require either.
- **Closer-to-Acme stance.** OpenAI hewed to the canonical reference more tightly than Anthropic, which interpreted the prompt's "B2C SaaS" framing as license to reframe terminology. Neither is wrong; the divergence is a feature of the test, not a bug.

---

## Reproducibility

The `output-openai.yaml` artifact is verified periodically via [`verify.sh`](verify.sh):

```bash
cd docs/reports/phase-4b-proof
./verify.sh    # re-runs validate + lint + test on the persisted output YAMLs
```

Re-running the full 3-call best-of-3 requires `OPENAI_API_KEY` + `./run-gate.sh`; the API costs (~$0.30 estimated for 3 OpenAI runs at 1 iter each) make it a deliberate operation, not part of CI.
