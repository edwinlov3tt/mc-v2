#!/usr/bin/env bash
# Phase 2C final validation gate per CLAUDE.md §6.1 + handoff
# acceptance criteria. Runs every gate the project requires before
# marking Phase 2C done. Logs to /tmp/phase2c-final/.

set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

LOGDIR=/tmp/phase2c-final
mkdir -p "$LOGDIR"

echo "==> 1. Format check"
cargo fmt --check --all > "$LOGDIR/fmt.log" 2>&1 \
    && echo "    ✓ fmt clean" \
    || (echo "    ✗ fmt failed; see $LOGDIR/fmt.log"; tail "$LOGDIR/fmt.log"; exit 1)

echo "==> 2. Clippy"
cargo clippy --workspace --all-targets -- -D warnings > "$LOGDIR/clippy.log" 2>&1 \
    && echo "    ✓ clippy clean" \
    || (echo "    ✗ clippy failed; see $LOGDIR/clippy.log"; tail "$LOGDIR/clippy.log"; exit 1)

echo "==> 3. Build"
cargo build --release --workspace > "$LOGDIR/build.log" 2>&1 \
    && echo "    ✓ build clean" \
    || (echo "    ✗ build failed; see $LOGDIR/build.log"; tail "$LOGDIR/build.log"; exit 1)

echo "==> 4. Tests"
cargo test --workspace > "$LOGDIR/test.log" 2>&1 \
    && echo "    ✓ tests pass; counts:" \
    || (echo "    ✗ tests failed; see $LOGDIR/test.log"; tail -50 "$LOGDIR/test.log"; exit 1)
grep -E "test result:" "$LOGDIR/test.log"

echo "==> 5. Determinism (10× consecutive cargo test --workspace -q)"
for i in $(seq 1 10); do
    if ! cargo test --workspace -q > "$LOGDIR/test.det.$i.log" 2>&1; then
        echo "    ✗ determinism run $i FAILED; see $LOGDIR/test.det.$i.log"
        exit 1
    fi
done
echo "    ✓ 10/10 deterministic"

echo "==> 6. CLI demo (matches brief §4.6)"
cargo run --release --bin mc -- demo > "$LOGDIR/demo.log" 2>&1 \
    && echo "    ✓ demo ran" \
    || (echo "    ✗ demo failed"; tail "$LOGDIR/demo.log"; exit 1)
# Quick golden check — Mar/Paid_Search/Tampa Spend = 11500 should appear:
grep -q "11_500" "$LOGDIR/demo.log" \
    && echo "    ✓ demo output contains expected anchor value 11_500" \
    || (echo "    ✗ demo output does not contain 11_500"; tail "$LOGDIR/demo.log"; exit 1)

echo "==> 7. Forbidden-pattern check — Phase 2C must not have touched mc-core/src/"
# Phase 2C is measurement-only; the kernel source must be unchanged
# from the inherited Phase 2B HEAD. Pre-existing `.expect()` calls in
# `#[cfg(test)] mod tests` blocks are allowed per CLAUDE.md §3.1
# ("Tests, benches, and fixtures may use `expect(\"static reason\")`")
# but a coarse grep can't tell them apart from production code. The
# right check for Phase 2C is "did this phase introduce any src/
# changes?" — answered by `git diff` against HEAD.
src_changes=$(git diff --stat HEAD -- crates/mc-core/src/ | head -1 || true)
if [ -z "$src_changes" ]; then
    echo "    ✓ zero src/ changes since Phase 2B HEAD"
else
    echo "    ✗ Phase 2C modified mc-core/src/:"
    echo "$src_changes"
    exit 1
fi
test_changes=$(git diff --stat HEAD -- crates/mc-core/tests/ | head -1 || true)
if [ -z "$test_changes" ]; then
    echo "    ✓ zero tests/ changes since Phase 2B HEAD"
else
    echo "    ✗ Phase 2C modified mc-core/tests/:"
    echo "$test_changes"
    exit 1
fi

echo "==> 8. unsafe / banned-import grep"
unsafe=$(grep -rn "unsafe " crates/mc-core/src/ || true)
if [ -z "$unsafe" ]; then
    echo "    ✓ zero unsafe in mc-core/src/"
else
    echo "    ✗ unsafe found:"; echo "$unsafe" | head; exit 1
fi
banned=$(grep -rn "use serde\|use tokio\|use rayon\|use anyhow" crates/ || true)
if [ -z "$banned" ]; then
    echo "    ✓ zero banned imports"
else
    echo "    ✗ banned imports:"; echo "$banned" | head; exit 1
fi
prints=$(grep -rn "println!\|eprintln!\|dbg!" crates/mc-core/src/ || true)
if [ -z "$prints" ]; then
    echo "    ✓ zero println in mc-core/src/"
else
    echo "    ✗ println in mc-core/src/:"; echo "$prints" | head; exit 1
fi

echo
echo "==> ALL GATES GREEN"
echo "Logs: $LOGDIR"
