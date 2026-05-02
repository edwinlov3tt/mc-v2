#!/usr/bin/env bash
# Copy `crates/mc-core/target/criterion/<id>/phase-2c/{json}` to
# `docs/reports/bench-data/phase-2c/<id>/{json}` (flat layout matching
# the existing phase-2a / phase-2b dirs). Run after the gate completes
# the run pass.

set -euo pipefail
cd "$(dirname "$0")"

SRC=crates/mc-core/target/criterion
DST=docs/reports/bench-data/phase-2c

mkdir -p "$DST"

count=0
for d in "$SRC"/*/; do
    id=$(basename "$d")
    if [ -d "$d/phase-2c" ]; then
        mkdir -p "$DST/$id"
        cp "$d/phase-2c/benchmark.json" "$DST/$id/" 2>/dev/null || true
        cp "$d/phase-2c/estimates.json" "$DST/$id/" 2>/dev/null || true
        cp "$d/phase-2c/sample.json"    "$DST/$id/" 2>/dev/null || true
        cp "$d/phase-2c/tukey.json"     "$DST/$id/" 2>/dev/null || true
        # Prune raw.csv if criterion accidentally produced one
        # (shouldn't because default-features=false at workspace level).
        rm -f "$DST/$id/raw.csv"
        count=$((count + 1))
    fi
done

echo "===> Exfilled $count rows to $DST"
ls -la "$DST" | head -20
