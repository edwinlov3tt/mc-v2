#!/usr/bin/env bash
# Phase 4B best-of-3 acceptance gate runner.
#
# Runs the canonical acceptance prompt 3 times against EACH adapter
# (Anthropic + OpenAI = 6 total invocations). Each invocation is a fresh
# Python process — fresh SDK client, fresh messages list, fresh
# max_iterations=5 budget. No state shared across runs.
#
# Per-run artifacts in this directory:
#   run-<adapter>-<N>.yaml   the YAML the run produced
#                            (output.yaml on success, output.failed.yaml on
#                            non-convergence — see author.py main())
#   run-<adapter>-<N>.log    captured stdout+stderr from author.py
#
# This script:
#   - Refuses to start if ANTHROPIC_API_KEY or OPENAI_API_KEY is unset.
#   - Never echoes, prints, or logs the keys at any point.
#   - Uses `set -u` (via -euo pipefail) so any unset variable fails fast.
#   - Prints a summary table at the end:
#       adapter | run | converged | iters | yaml passes validate+lint+test
#   - Prints a best-of-3 verdict per adapter (≥ 2/3 required).
#
# After this script finishes, the user reviews the per-run logs + YAMLs and
# pastes a summary back to the implementer to fill in the Phase 4B
# transcripts and completion report.

set -euo pipefail

# Refuse to start without keys. The :? form prints the message and exits 1.
# Neither the variable name nor its value is ever echoed elsewhere.
: "${ANTHROPIC_API_KEY:?ANTHROPIC_API_KEY is not set; aborting.}"
: "${OPENAI_API_KEY:?OPENAI_API_KEY is not set; aborting.}"

PROOF_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${PROOF_DIR}/../../.." && pwd)"
PROMPT="marketing-mix model for a 5-channel B2C SaaS with monthly seasonality and a Q4 lift scenario"

# Sanity checks (no key handling here — these only check presence of tools).
command -v mc >/dev/null 2>&1 || {
  echo "ERROR: 'mc' not on PATH. Install via:"
  echo "       cargo install --path ${REPO_ROOT}/crates/mc-cli --locked"
  exit 1
}
command -v python >/dev/null 2>&1 || {
  echo "ERROR: 'python' not on PATH. Activate a Python ≥3.10 venv with"
  echo "       both adapters installed, then re-run."
  exit 1
}

py_major=$(python -c 'import sys; print(sys.version_info.major)')
py_minor=$(python -c 'import sys; print(sys.version_info.minor)')
if [[ "${py_major}" -lt 3 || ( "${py_major}" -eq 3 && "${py_minor}" -lt 10 ) ]]; then
  echo "ERROR: Python ${py_major}.${py_minor} is too old. Adapters require >=3.10."
  exit 1
fi

cd "${PROOF_DIR}"

echo "[gate] mc:           $(command -v mc)"
echo "[gate] python:       $(command -v python) (${py_major}.${py_minor})"
echo "[gate] proof dir:    ${PROOF_DIR}"
echo "[gate] prompt:       ${PROMPT}"
echo "[gate] keys:         present (not echoed)"
echo

# Summary rows accumulator. Format: "adapter|run|converged|iters|validates"
SUMMARY_ROWS=()

run_one () {
  local adapter="$1"
  local run_idx="$2"
  local adapter_dir="${REPO_ROOT}/mosaic-plugin/examples/adapters/${adapter}-python"
  local yaml_path="${PROOF_DIR}/run-${adapter}-${run_idx}.yaml"
  local log_path="${PROOF_DIR}/run-${adapter}-${run_idx}.log"

  # Clear any prior artifacts so a re-run produces clean state.
  rm -f "${yaml_path}" "${yaml_path%.yaml}.failed.yaml" "${log_path}"

  echo "=== ${adapter} run ${run_idx} ==="
  set +e
  python "${adapter_dir}/author.py" "${PROMPT}" \
      --output "${yaml_path}" >"${log_path}" 2>&1
  local rc=$?
  set -e

  local converged="no"
  local iters="?"
  local final_validates="n/a"

  # author.py writes either output.yaml (rc=0) or output.failed.yaml (rc=2).
  local final_yaml=""
  if [[ -f "${yaml_path}" ]]; then
    final_yaml="${yaml_path}"
  elif [[ -f "${yaml_path%.yaml}.failed.yaml" ]]; then
    final_yaml="${yaml_path%.yaml}.failed.yaml"
  fi

  if [[ ${rc} -eq 0 ]]; then
    converged="yes"
    iters=$(grep -oE 'Converged in [0-9]+ iteration' "${log_path}" \
              | head -1 | grep -oE '[0-9]+' || echo '?')
    if [[ -n "${final_yaml}" ]] \
        && mc model validate "${final_yaml}" >/dev/null 2>&1 \
        && mc model lint     "${final_yaml}" >/dev/null 2>&1 \
        && mc model test     "${final_yaml}" >/dev/null 2>&1; then
      final_validates="yes"
    else
      final_validates="no"
    fi
  else
    iters=$(grep -oE 'Did NOT converge in [0-9]+ iteration' "${log_path}" \
              | head -1 | grep -oE '[0-9]+' || echo '?')
    final_validates="no"
  fi

  SUMMARY_ROWS+=("${adapter}|${run_idx}|${converged}|${iters}|${final_validates}")
  echo "[gate]  rc=${rc} converged=${converged} iters=${iters} validates=${final_validates}"
  echo "[gate]  log:  ${log_path}"
  echo "[gate]  yaml: ${final_yaml:-<none>}"
  echo
}

for adapter in anthropic openai; do
  for i in 1 2 3; do
    run_one "${adapter}" "${i}"
  done
done

echo "=== Summary ==="
printf "%-9s | %-3s | %-9s | %-5s | %-30s\n" \
    "adapter" "run" "converged" "iters" "yaml passes validate+lint+test"
printf "%-9s-+-%-3s-+-%-9s-+-%-5s-+-%-30s\n" \
    "---------" "---" "---------" "-----" "------------------------------"
for row in "${SUMMARY_ROWS[@]}"; do
  IFS='|' read -r ad rn cv it va <<<"${row}"
  printf "%-9s | %-3s | %-9s | %-5s | %-30s\n" "${ad}" "${rn}" "${cv}" "${it}" "${va}"
done

echo
echo "Best-of-3 verdict (need ≥ 2/3 with converged=yes AND validates=yes):"
gate_failed=0
for adapter in anthropic openai; do
  passes=0
  for row in "${SUMMARY_ROWS[@]}"; do
    IFS='|' read -r ad rn cv it va <<<"${row}"
    if [[ "${ad}" == "${adapter}" && "${cv}" == "yes" && "${va}" == "yes" ]]; then
      passes=$((passes + 1))
    fi
  done
  if [[ ${passes} -ge 2 ]]; then
    echo "  ${adapter}: ${passes}/3 ✓"
  else
    echo "  ${adapter}: ${passes}/3 ✗ (gate failure)"
    gate_failed=1
  fi
done

echo
echo "Per-run logs + YAMLs are in ${PROOF_DIR}/run-*-*.{log,yaml}."
echo "Forward to the Phase 4B implementer to fill the transcripts +"
echo "completion report. Do NOT paste API keys or full YAML bodies upstream;"
echo "the implementer reads them off-band from this directory."

exit ${gate_failed}
