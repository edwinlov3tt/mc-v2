# `tools/bench/phase-2c/` — Phase 2C bench-harness scripts

Five helper scripts the Phase 2C implementing instance built to drive the workload-shaped bench gate. Kept under `tools/` so the repo root stays clean. Future phases (2D, 2E, …) should follow the same `tools/bench/phase-N/` pattern when they need their own helpers.

## Scripts

| Script | Purpose | When to use |
|---|---|---|
| [`run_phase_2c_targeted.sh`](./run_phase_2c_targeted.sh) | Targeted bench gate — 1× rows + a curated set of 10× rows at `--sample-size 10`, against `--baseline phase-2b`. The narrow gate Phase 2C actually shipped. | Re-run the Phase 2C numbers locally / on a different machine. |
| [`run_phase_2c_gate.sh`](./run_phase_2c_gate.sh) | Earlier gate variant. Kept as audit trail. | Reference only; prefer `run_phase_2c_targeted.sh`. |
| [`run_phase_2c_final_gate.sh`](./run_phase_2c_final_gate.sh) | Full final-validation gate (fmt / clippy / build / test / 10× determinism / demo / forbidden-pattern grep). The script the project-manager audit ran in this session. | Re-verify Phase 2C any time. Logs to `/tmp/phase2c-final/`. |
| [`exfil_phase_2c_baseline.sh`](./exfil_phase_2c_baseline.sh) | Copies `crates/mc-core/target/criterion/<bench>/<id>/phase-2c/` JSON into `docs/reports/bench-data/phase-2c/`. The committed baseline workflow per [`docs/reports/bench-data/README.md`](../../../docs/reports/bench-data/README.md). | After any local re-run that captures `--save-baseline phase-2c`. |
| [`extract_phase_2c_numbers.sh`](./extract_phase_2c_numbers.sh) | Pulls median + range out of the saved `estimates.json` files for human-readable summary. Used to populate PERF.md §6.12 / §6.13 / §6.14 from the JSON. | When updating PERF.md after a re-run. |

## Invocation pattern

All scripts assume the workspace root is the working directory. Examples:

```bash
cd /path/to/mc-v2
bash tools/bench/phase-2c/run_phase_2c_final_gate.sh   # full gate, ~10 min
bash tools/bench/phase-2c/run_phase_2c_targeted.sh     # targeted bench gate, ~30–45 min
```

The scripts source `$HOME/.cargo/env` so a fresh shell works without manual rustup setup.

## Notes for Phase 2D and later

If Phase 2D (or any later sub-phase) wants the same harness pattern, copy these scripts to `tools/bench/phase-2d/`, update the baseline names (e.g. `phase-2c` → `phase-2d`), and update PERF.md / `bench-data/` paths accordingly. Don't edit the Phase 2C scripts in place — they're the audit record of what produced `bench-data/phase-2c/`.
