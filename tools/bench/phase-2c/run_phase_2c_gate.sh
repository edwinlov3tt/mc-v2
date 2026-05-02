#!/usr/bin/env bash
# Phase 2C bench gate runner.
#
# Runs each bench file ONCE with `--sample-size 10 --save-baseline
# phase-2c`, which simultaneously runs the benches and saves the
# results under `target/criterion/<id>/phase-2c/`. Then a fast
# follow-up pass uses `--load-baseline phase-2c --baseline-lenient
# phase-2b` to produce the diff against phase-2b without re-running
# benches (criterion reads the saved phase-2c data and the saved
# phase-2b data and computes the comparison instantly).
#
# `--baseline-lenient` (not `--baseline`) is what lets the new scaled
# rows (`/10x`, `/50x`, `/100x`, `combined_workflow/*`) — which have
# no phase-2b baseline because the rows didn't exist at the phase-2b
# tag — skip comparison instead of erroring.
#
# Sample-size 10 is criterion's minimum; the heavy 50× / 100× scaled
# rows have multi-second per-iteration setup costs that exhaust the
# default sample-size-100 budget within minutes per row.

set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

LOGDIR=/tmp/phase2c-runs
mkdir -p "$LOGDIR"

BENCHES=(
    leaf_read_write
    derived_read
    consolidated_read
    dirty_propagation
    demo_path
    synthetic_no_deps
    snapshot_clone
    hierarchy_mark
)

echo "===> Pass 1 (run + save): --save-baseline phase-2c"
for b in "${BENCHES[@]}"; do
    echo "----- run $b -----"
    cargo bench -p mc-core --bench "$b" -- \
        --sample-size 10 --save-baseline phase-2c \
        > "$LOGDIR/$b.run.log" 2>&1
    grep -E "time:|change:|^Performance|Found|Benchmarking" "$LOGDIR/$b.run.log" | tail -25 || true
done

echo "===> combined_workflow 50x only (100x's bulk-load wall-clock is multi-hour)"
cargo bench -p mc-core --bench combined_workflow -- \
    --sample-size 10 --save-baseline phase-2c 'combined_workflow/50x' \
    > "$LOGDIR/combined_workflow.run.log" 2>&1
grep -E "combined_workflow x|time:" "$LOGDIR/combined_workflow.run.log" | tail -25 || true

echo "===> Pass 2 (compare-only): --load-baseline phase-2c --baseline-lenient phase-2b"
for b in "${BENCHES[@]}"; do
    echo "----- compare $b -----"
    cargo bench -p mc-core --bench "$b" -- \
        --sample-size 10 --load-baseline phase-2c --baseline-lenient phase-2b \
        > "$LOGDIR/$b.compare.log" 2>&1
    grep -E "time:|change:|^Performance" "$LOGDIR/$b.compare.log" | tail -25 || true
done

echo "===> Done. Logs in $LOGDIR; baselines saved under crates/mc-core/target/criterion/<id>/phase-2c/"
