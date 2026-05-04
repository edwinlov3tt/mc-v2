#!/usr/bin/env bash
# Phase 4B canonical-output verification harness.
#
# Re-runs `mc model validate / lint / test` against the persisted canonical
# YAML for each adapter (the FIRST passing run from each adapter's
# best-of-3). Verifies the persisted artifacts haven't bit-rotted between
# Phase 4B commit and review time.
#
# This script does NOT re-run the LLM gate. The best-of-3 audit lives in
# transcript-{anthropic,openai}.md; that record is authoritative. Re-running
# the gate burns API credit; this script is the cheap re-check.

set -euo pipefail

cd "$(dirname "$0")"

# Sanity check that mc is on PATH before iterating.
command -v mc >/dev/null 2>&1 || {
  echo "ERROR: 'mc' not on PATH. Install via:"
  echo "  cargo install --path ../../../crates/mc-cli --locked"
  exit 1
}

failed=0

for adapter in anthropic openai; do
  yaml="output-${adapter}.yaml"
  echo "=== Verifying ${adapter} canonical output: ${yaml} ==="
  if [[ ! -f "${yaml}" ]]; then
    echo "  ✗ ${yaml} not found"
    failed=1
    continue
  fi
  if ! mc model validate "${yaml}"; then
    echo "  ✗ ${yaml} failed validate"; failed=1; continue
  fi
  if ! mc model lint "${yaml}"; then
    echo "  ✗ ${yaml} failed lint"; failed=1; continue
  fi
  if ! mc model test "${yaml}"; then
    echo "  ✗ ${yaml} failed test"; failed=1; continue
  fi
  echo "  ✓ ${adapter} canonical YAML passes all three gates"
  echo
done

if [[ ${failed} -ne 0 ]]; then
  echo "✗ One or more canonical outputs failed re-verification."
  exit 1
fi

echo "All canonical outputs verified."
echo "(Best-of-3 audit lives in transcript-{anthropic,openai}.md;"
echo "this script verifies the persisted canonical outputs only.)"
