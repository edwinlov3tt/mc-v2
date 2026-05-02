#!/usr/bin/env bash
# Extract per-row median + change% from the phase-2c gate logs into a
# TSV that drops into PERF.md §6.12 / §6.13 tables.
#
# Run this after the gate finishes (or against any subset of completed
# .run.log files). Output goes to /tmp/phase2c-numbers.tsv.

set -euo pipefail

LOGDIR=/tmp/phase2c-runs
OUT=/tmp/phase2c-numbers.tsv

echo -e "bench_id\tmedian\tlower\tupper\tchange_median" > "$OUT"

for log in "$LOGDIR"/*.run.log; do
    bench_file=$(basename "$log" .run.log)
    awk -v file="$bench_file" '
        /^Benchmarking [^:]+: Analyzing/ {
            # The line BEFORE "Analyzing" is the bench id (often).
            next
        }
        /^[^ ]+\s+time:/ || /^                        time:/ {
            # Two patterns: "<id>\ttime: [low mid high]" or "                        time: [...]"
            # When the id is on the same line, the previous Benchmarking line tells us.
            if ($1 == "time:") {
                id = last_id
                start = 2
            } else {
                id = $1
                start = 3
            }
            # Time array is [low <unit> mid <unit> high <unit>], e.g. [645.87 ns 698.67 ns 738.83 ns]
            low = $(start)" "$(start+1)
            mid = $(start+2)" "$(start+3)
            high = $(start+4)" "$(start+5)
            # gsub brackets
            gsub(/[\[\]]/, "", low)
            gsub(/[\[\]]/, "", mid)
            gsub(/[\[\]]/, "", high)
            print file "\t" id "\t" mid "\t" low "\t" high
        }
        /^Benchmarking / && !/Warming|Collecting|Analyzing/ {
            last_id = substr($0, 14)  # "Benchmarking " is 13 chars
        }
    ' "$log" >> "$OUT" 2>/dev/null || true
done

echo "===> Wrote $OUT"
echo "===> Bench rows captured:"
wc -l "$OUT"
echo
echo "===> Preview:"
head -30 "$OUT"
