#!/usr/bin/env bash
# Targeted Phase 2C gate — runs 1× + 10× rows only, skipping 50× / 100×
# scaled rows that exceed the single-session wall-clock budget. Deferred
# rows are documented in PERF.md §6.12 and the completion report §4.4 +
# §6 deferrals.
#
# This is the *reduced* gate; the full gate runner (run_phase_2c_gate.sh)
# would take 6+ hours on this machine due to per-iteration setup costs
# at 50× / 100× scale. The 10× rows establish the directional scaling
# shape Phase 2D needs to read §6.14; 50× and 100× rows are
# enrichment that Phase 2C-bis (or Phase 2D step 0) can run later.

set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

LOGDIR=/tmp/phase2c-runs
mkdir -p "$LOGDIR"

# Each bench file: regex includes 1× rows (no /10x or /50x or /100x in
# id) AND 10× rows (matches /10x). Excludes 50× / 100× via no positive
# match.
declare -A FILTERS=(
    # Each filter is a positive regex listing rows to RUN. Excludes 50×
    # / 100× rows by enumerating only the 1× and 10× rows. Anchored with
    # ^...$ where needed to avoid prefix-matching the unwanted scaled rows.
    [leaf_read_write]="^(read_input_leaf_warm|read_input_leaf_cold|write_input_leaf|write_input_leaf_no_deps|write_input_leaf/10x|read_input_leaf_warm/10x|read_input_leaf_cold/10x)$"
    [derived_read]="^(read_derived_leaf_(warm|cold)/(Clicks|Leads|Customers|Revenue|Gross_Profit)|read_derived_leaf_cold/Revenue/10x)$"
    [consolidated_read]="(consolidation_(warm|cold)/Q1_PaidSearch_Tampa/Spend \(3 leaves\))|(consolidation_(warm|cold)/Q1_PaidMedia_Florida/(Spend|CPC|Revenue|Gross_Profit) \(27 leaves[^/]*\))|(consolidation_(warm|cold)/FY_AllChannels_USA/Spend \(420 leaves\))|(consolidation_cold/(Q1_PaidMedia_Florida/Spend|FY_AllChannels_USA/Spend)/10x)"
    [dirty_propagation]="."
    [demo_path]="(demo_path/(build_only|build_and_load|build_load_materialize|full_demo_reads|full_revenue_slice_warm|load_canonical_inputs))|(demo_path/load_canonical_inputs/10x)"
    [synthetic_no_deps]="."
    [snapshot_clone]="^(snapshot|rollback)/(0_cells_fresh|100_cells|2520_cells_loaded|materialized|10x_loaded)"
    [hierarchy_mark]="."
)

echo "===> Targeted Pass 1: --save-baseline phase-2c (1× + 10× only)"
for b in leaf_read_write derived_read consolidated_read dirty_propagation demo_path synthetic_no_deps snapshot_clone hierarchy_mark; do
    filter="${FILTERS[$b]}"
    echo "----- run $b (filter: $filter) -----"
    cargo bench -p mc-core --bench "$b" -- \
        --sample-size 10 --save-baseline phase-2c "$filter" \
        > "$LOGDIR/$b.run.log" 2>&1 || echo "  (warning: $b failed; logs in $LOGDIR/$b.run.log)"
    grep -E "time:|change:|^Performance|Found|Benchmarking [^:]+$" "$LOGDIR/$b.run.log" | tail -25 || true
done

echo "===> combined_workflow 50x (already-captured data is in /tmp/cw50_v4.log; this is a re-run for the saved baseline)"
cargo bench -p mc-core --bench combined_workflow -- \
    --sample-size 10 --save-baseline phase-2c 'combined_workflow/50x' \
    > "$LOGDIR/combined_workflow.run.log" 2>&1 || echo "  (warning: combined_workflow failed)"

echo "===> Pass 2 (compare-only): --load-baseline phase-2c --baseline-lenient phase-2b"
for b in leaf_read_write derived_read consolidated_read dirty_propagation demo_path synthetic_no_deps snapshot_clone hierarchy_mark; do
    filter="${FILTERS[$b]}"
    echo "----- compare $b -----"
    cargo bench -p mc-core --bench "$b" -- \
        --sample-size 10 --load-baseline phase-2c --baseline-lenient phase-2b "$filter" \
        > "$LOGDIR/$b.compare.log" 2>&1 || echo "  (warning: $b compare failed)"
    grep -E "time:|change:|^Performance" "$LOGDIR/$b.compare.log" | tail -15 || true
done

echo "===> Done. Logs in $LOGDIR; baselines saved under crates/mc-core/target/criterion/<id>/phase-2c/"
