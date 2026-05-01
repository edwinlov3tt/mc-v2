## Section 1: Repo orientation

### Top-level directory tree (2 levels deep, excluding node_modules / .git / dist / __pycache__)

```
claw-core/
├── .claude/
│   ├── commands/
│   ├── docs/
│   ├── features/
│   └── tasks/
├── .superpowers/
│   └── brainstorm/
├── .wrangler/
│   ├── state/
│   └── tmp/
├── docs/
│   ├── architecture/
│   ├── archive/
│   ├── audits/
│   ├── business/
│   ├── concepts/
│   ├── dead-ends/
│   ├── experiments/
│   ├── external-research/
│   ├── hypotheses/
│   ├── issues/
│   ├── markets/
│   ├── models/
│   ├── superpowers/
│   └── templates/
├── pages/                       # Static pages (small)
├── schema/                      # SQL schema + 19 migrations
├── scripts/                     # KB sync etc.
├── src/                         # Cloudflare Worker (TypeScript)
│   ├── lib/
│   ├── middleware/
│   ├── routes/
│   ├── scheduled/
│   └── services/
├── training/                    # Python training pipeline + EV simulator
│   ├── artifacts/               # trained weights + parquet datasets
│   ├── data/                    # historical games_*.parquet, etc.
│   ├── models/                  # legacy linear_v1.py, xgboost_v1_1.py
│   └── simulator/               # the canonical EV simulator (Python)
├── CLAUDE.md
├── package.json
├── tsconfig.json
└── wrangler.toml
```

### Tech stack

Worker / runtime ([package.json](../package.json)):

```json
{
  "dependencies": {
    "hono": "4.7.5",
    "zod": "3.24.2"
  },
  "devDependencies": {
    "@cloudflare/workers-types": "4.20250327.0",
    "typescript": "5.8.2",
    "vitest": "4.1.2",
    "wrangler": "4.6.0"
  }
}
```

Worker config ([wrangler.toml](../wrangler.toml)):

```toml
name = "claw-edge"
main = "src/index.ts"
compatibility_date = "2026-03-25"

[[d1_databases]]
binding = "DB"
database_name = "claw-edge-db"
database_id = "8767ad73-28fc-4617-9ccd-788b9c26bbba"

[[kv_namespaces]]
binding = "KV"
id = "189995470c4d4d638bc4a41eaf91ff05"

[[r2_buckets]]
binding = "R2"
bucket_name = "claw-edge-data"
```

Training / Python ([training/requirements.txt](../training/requirements.txt)):

```
scikit-learn
pandas
numpy
nba_api
xgboost
requests
pyarrow
```

No `pyproject.toml` or `Cargo.toml` exist in the repo. The Python code is invoked directly with `python3` — there is no installable package and no virtualenv pinning.

### Deployment surface

| Artifact | Location | Where it runs | Notes |
|---|---|---|---|
| Cloudflare Worker | [src/index.ts](../src/index.ts) (entry) → built by Wrangler | Cloudflare global edge | All routes + crons |
| D1 database | `claw-edge-db` (UUID `8767ad73-28fc-4617-9ccd-788b9c26bbba`) | Cloudflare D1 | SQLite-on-edge |
| KV namespace | id `189995470c4d4d638bc4a41eaf91ff05` | Cloudflare KV | Hot config / weights / sync metadata |
| R2 bucket | `claw-edge-data` | Cloudflare R2 | Walk-forward dataset, large artifacts |
| XGBoost inference server | DigitalOcean droplet at `xgb.edwinlovett.com` (CF Tunnel → 174.138.71.106:8080) | Droplet | Python flask-style; called by [src/services/xgboost-client.ts:11](../src/services/xgboost-client.ts#L11) |
| Training pipeline | `training/*.py` | Edwin's Mac (local) | Reads `training/data/*.parquet`, writes `training/artifacts/` |
| EV simulator (Python) | [training/simulator/](../training/simulator/) | Edwin's Mac (local) | Used by experiment scripts; not in Worker |
| KellyBets agent | Mac Mini (`kellybets@100.86.185.128`) | Mac Mini (Tailscale) | Out of repo; mentioned in CLAUDE.md |

### Entry points

#### Crons (declared in [wrangler.toml:30-41](../wrangler.toml#L30-L41), dispatched in [src/index.ts:493-536](../src/index.ts#L493-L536))

| Trigger (UTC) | File | Function | Purpose |
|---|---|---|---|
| `0,30 * * * *` (every 30m) | [src/scheduled/daily-odds-sync.ts](../src/scheduled/daily-odds-sync.ts) | `dailyOddsSync` | Pull odds (totals + h2h + spreads) |
| `0 5 * * *` | [src/scheduled/daily-stats-sync.ts](../src/scheduled/daily-stats-sync.ts) | `dailyStatsSync` | Pull scores + team stats + player stats |
| `0 14 * * *` | [src/scheduled/daily-stats-sync.ts](../src/scheduled/daily-stats-sync.ts) | `dailyStatsSync` | Refresh team + player rolling averages |
| `0 16 * * *` | [src/scheduled/pre-game-analysis.ts](../src/scheduled/pre-game-analysis.ts) | `preGameAnalysis` | Run model, compute edge, line shop |
| `15 16 * * *` | [src/scheduled/player-prop-analysis.ts](../src/scheduled/player-prop-analysis.ts) | `playerPropAnalysis` | Player prop predictions |
| `30 16 * * *` | [src/services/prediction-adjuster.ts](../src/services/prediction-adjuster.ts) | `applyPredictionAdjustments` | Lineup-derived + bottom-up adjustments |
| `0 20-23 * * *` and `0 0-3 * * *` | [src/scheduled/confirmation-pass.ts](../src/scheduled/confirmation-pass.ts) | `confirmPredictions` | Hourly dynamic confirmation gate (20-100 min before tipoff) |
| `0 6 * * *` | [src/scheduled/post-game-settlement.ts](../src/scheduled/post-game-settlement.ts) | `postGameSettlement` | Settle predictions, log CLV + residuals |
| `30 23 * * *` | [src/index.ts:431-483](../src/index.ts#L431-L483) | `lineupCheck` | Post-tip lineup verification |

#### HTTP routes (Hono app, [src/index.ts:36-72](../src/index.ts#L36-L72))

| Method/Path | File | Function | Auth |
|---|---|---|---|
| `*` (CORS) | [src/middleware/cors.ts](../src/middleware/cors.ts) | `corsMiddleware` | n/a |
| `app.route("/api/lab", ...)` | [src/routes/lab.ts](../src/routes/lab.ts) | `lab` | public |
| `app.route("/api/schedule", ...)` | [src/routes/schedule.ts](../src/routes/schedule.ts) | `schedule` | public |
| `app.route("/api/record", ...)` | [src/routes/record.ts](../src/routes/record.ts) | `record` | public |
| `app.route("/api/kb", ...)` | [src/routes/kb.ts](../src/routes/kb.ts) | `kb` | public |
| `app.route("/api/player-props", ...)` | [src/routes/player-props.ts](../src/routes/player-props.ts) | `playerProps` | public |
| `app.route("/api/predictions", ...)` | [src/routes/predictions.ts](../src/routes/predictions.ts) | `predictions` | public |
| `app.route("/api/games", ...)` | [src/routes/games.ts](../src/routes/games.ts) | `games` | public |
| `app.route("/api/health", ...)` | [src/routes/health.ts](../src/routes/health.ts) | `health` | public |
| `app.route("/api/clv", ...)` | [src/routes/clv.ts](../src/routes/clv.ts) | `clv` | public |
| `app.route("/api/line-shop", ...)` | [src/routes/line-shop.ts](../src/routes/line-shop.ts) | `lineShop` | public |
| `app.route("/api/notifications", ...)` | [src/routes/notifications.ts](../src/routes/notifications.ts) | `notifications` | public |
| `app.use("/api/*", authMiddleware)` | [src/middleware/auth.ts](../src/middleware/auth.ts) | bearer auth | gate |
| `app.use("/api/*", rateLimitMiddleware)` | [src/middleware/rate-limit.ts](../src/middleware/rate-limit.ts) | KV-backed rate limit | gate |
| `app.route("/api/backtest", ...)` | [src/routes/backtest.ts](../src/routes/backtest.ts) | `backtest` | bearer |
| `app.route("/api/research", ...)` | [src/routes/research.ts](../src/routes/research.ts) | `research` | bearer |
| `app.route("/api/dashboard", ...)` | [src/routes/dashboard.ts](../src/routes/dashboard.ts) | `dashboard` | bearer |
| `app.route("/api/wagers", ...)` | [src/routes/wagers.ts](../src/routes/wagers.ts) | `wagers` | bearer |
| `app.route("/api/wagers/import", ...)` | [src/routes/wager-import.ts](../src/routes/wager-import.ts) | `wagerImport` | bearer |
| `app.route("/api/bettor", ...)` | [src/routes/bettor.ts](../src/routes/bettor.ts) | `bettor` | bearer |
| `app.route("/nba", ...)` | [src/routes/nba.ts](../src/routes/nba.ts) | `nba` | public pages |
| `POST /api/admin/trigger/:job` | [src/index.ts:74-264](../src/index.ts#L74-L264) | dispatches all crons by name | bearer |
| `POST /api/admin/backfill/players` | [src/index.ts:267-276](../src/index.ts#L267-L276) | `backfillPlayerStats` | bearer |
| `POST /api/admin/calibrate` | [src/index.ts:279-326](../src/index.ts#L279-L326) | inline: `fitCalibration` from settled predictions, write KV `calibration:map:current` | bearer |
| `POST /api/admin/backfill` | [src/index.ts:329-348](../src/index.ts#L329-L348) | `backfillGames` | bearer |
| `POST /api/admin/shadow` | [src/index.ts:351-359](../src/index.ts#L351-L359) | `registerShadowModel` | bearer |
| `GET /api/admin/shadow/compare` | [src/index.ts:362-382](../src/index.ts#L362-L382) | `compareShadowPerformance` | bearer |
| `GET /api/admin/shadow` | [src/index.ts:385-389](../src/index.ts#L385-L389) | list shadow models | bearer |
| `GET /api/admin/logs` | [src/index.ts:392-425](../src/index.ts#L392-L425) | `queryLogs` | bearer |

#### CLI scripts (Python, all under [training/](../training/))

| File | Purpose |
|---|---|
| `audit_model.py` | Surfaces dead features, unused signal, error patterns, suspicious weights |
| `backfill.py` | Pull historical games + team stats from BDL into `training/data/games_*.parquet` |
| `backfill_players.py` | Pull historical player stats |
| `backtest_head_to_head.py` / `backtest_v15_vs_v16.py` / `backtest_v16_production.py` | Pre-simulator one-off backtests |
| `build_player_features.py` / `build_player_features_v2.py` | V1 / V2 player feature builders (V2 is current) |
| `compare_models.py` | Side-by-side metrics across model versions |
| `enrich_backfill.py` | Merge BDL advanced stats into `games_*.parquet` |
| `evaluate.py` | Generic eval over a saved model |
| `exp011_walk_forward_real_lines.py` | EXP-011 — re-grade walk-forward against real closing lines |
| `exp012_v17_residual_model.py` | EXP-012 — V1.7 residual-target Lasso (audit-blocked) |
| `exp013_v16_production_replay.py` | EXP-013 — replay deployed V1.6 weights through simulator |
| `exp015_xgboost_vs_lasso.py` | EXP-015 — Lasso/XGB ensemble sweep with OOD breakout |
| `exp016_fit_calibration.py` | EXP-016 — fit + push calibration map to KV |
| `exp017_opening_vs_closing.py` | EXP-017 — opening vs closing edge |
| `features.py` | Feature matrix builder shared by all scripts |
| `final_lasso_test.py` | One-off Lasso α sweep |
| `historical_odds_backfill.py` | Pull `odds_totals_*.parquet` from The Odds API historical endpoints |
| `iss011_walkback.py` | Walkback: what would V1.6 playoff predictions have looked like with `missing_top_scorers` populated |
| `panel_backtest.py` | Retroactive panel-Ridge backtest (5 alphas) |
| `postmortem_analysis.py` | Per-prediction error attribution from walk-forward parquet |
| `ridge_player_fix_experiment.py` | Diagnostic for Ridge vs Lasso on player features |
| `test_exp013_artifact_invariant.py` | Asserts the exp013 parquet is the production-fidelity variant |
| `test_v2_features.py` | Unit checks for `build_player_features_v2.py` |
| `train.py` | Generic Lasso/Ridge trainer |
| `train_v16_final.py` | The exact training script that produced [training/artifacts/lasso_v16_20260408_020552](../training/artifacts/lasso_v16_20260408_020552/) |
| `two_stage_residual_test.py` | Diagnostic for residual-on-residual chained models |
| `upload_model.py` | `npx wrangler kv key put` weights to KV `model:weights:{version}` |
| `validate_model.py` | Validation gate (MAE regression, leakage, residual std sanity) |
| `variance_experiment_v2.py` | Stage-2 σ model attempt (failed, see EXP-008 v2 dead-end) |
| `walk_forward_backtest.py` | The walk-forward V1.6 evaluation that produced `walk_forward_predictions_20260408.parquet` |
| `walk_forward_to_json.py` | Converts walk-forward parquet → R2-uploadable JSON |
| `xgb_hyperparam_sweep.py` | XGBoost hyperparameter sweep (produced V1.6 droplet model) |
| `xgb_player_experiment.py` | XGBoost on player features experiment |

---

## Section 2: Model implementations

### Lasso V1.6 (production)

- **Inference code path**: [src/services/probability-model.ts:374-435](../src/services/probability-model.ts#L374-L435) — `predict(env, features, modelVersion)`. Loads weights from KV, applies linear `intercept + Σ coef · feature`, imputes nulls with `TRAINING_MEDIANS`, applies `PLAYOFF_OFFSET_POINTS = -18` if in playoff window.
- **Training code path**: [training/train_v16_final.py:73-160](../training/train_v16_final.py#L73-L160) — Lasso(α=0.7) with StandardScaler over 3 seasons (2022-23 → 2024-25), then standardized→raw coefficient conversion at lines 100-119 so the Worker's dot-product inference doesn't need scaler state.
- **Weights/artifact storage**:
  - [training/artifacts/lasso_v16_20260408_020552/weights.json](../training/artifacts/lasso_v16_20260408_020552/weights.json) — 1,805 bytes, intercept + 50 raw-space coefficients + `residual_std`.
  - [training/artifacts/lasso_v16_20260408_020552/metrics.json](../training/artifacts/lasso_v16_20260408_020552/metrics.json) — 9,288 bytes, full audit metadata: `feature_names`, `non_zero_features`, `holdout_metrics`, `train_metrics`, `scaler.{means,stds}` (used to verify the raw conversion), and `column_means` (the training-time imputation values).
  - Production KV key: `model:weights:v1.6-lasso-player`.
  - Embedded fallback weights mirrored at [src/services/probability-model.ts:89-148](../src/services/probability-model.ts#L89-L148).
- **Feature contract** (verbatim from [src/lib/types.ts:101-162](../src/lib/types.ts#L101-L162)):

```ts
export interface FeatureVector {
  avg_pace: number | null;
  pace_delta: number | null;
  combined_off_rating: number | null;
  combined_def_rating: number | null;
  home_net_rating: number | null;
  away_net_rating: number | null;
  rest_disparity: number | null;
  home_b2b: number | null;
  away_b2b: number | null;
  avg_recent_total_5: number | null;
  avg_recent_total_10: number | null;
  home_away_factor: number | null;
  // V1.1 features: shooting efficiency + win records
  combined_ts_pct: number | null;
  combined_efg_pct: number | null;
  home_win_pct: number | null;
  away_win_pct: number | null;
  home_home_win_pct: number | null;
  away_road_win_pct: number | null;
  // V1.2 features: scoring + possession metrics
  combined_pts_avg: number | null;
  combined_possessions: number | null;
  combined_turnovers: number | null;
  combined_fg3_rate: number | null;
  combined_ft_rate: number | null;
  pts_diff: number | null;
  // V1.4: assists
  combined_assists: number | null;
  // V1.3 features: trends, matchups, volatility
  scoring_trend: number | null;
  home_off_vs_away_def: number | null;
  away_off_vs_home_def: number | null;
  total_volatility: number | null;
  h2h_last_total: number | null;
  h2h_avg_total: number | null;
  h2h_games_played: number | null;
  home_recent_total_avg: number | null;
  away_recent_total_avg: number | null;
  margin_of_victory: number | null;
  // V1.5 features: BDL enrichment + Odds API
  combined_oreb_pct: number | null;
  combined_stl: number | null;
  combined_pfd: number | null;
  combined_tm_tov_pct: number | null;
  combined_ft_pct: number | null;
  home_home_win_pct_real: number | null;
  away_road_win_pct_real: number | null;
  odds_spread: number | null;
  odds_moneyline_implied: number | null;
  // V1.6: Player availability features (pre-game, no leakage)
  home_expected_starter_pts: number | null;
  home_missing_starter_pts: number | null;
  home_expected_starter_count: number | null;
  home_top_scorer_recent_played_pct: number | null;
  home_missing_top_scorers: number | null;
  away_expected_starter_pts: number | null;
  away_missing_starter_pts: number | null;
  away_expected_starter_count: number | null;
  away_top_scorer_recent_played_pct: number | null;
  away_missing_top_scorers: number | null;
}
```

- **Output contract** ([src/services/probability-model.ts:72-77](../src/services/probability-model.ts#L72-L77)):

```ts
export interface ModelPrediction {
  predictedTotal: number;
  predictedStd: number;
  confidence: number;
  modelVersion: string;
}
```

- **Calibration metadata stored alongside the model**:
  - `residualStd: 17.251` — the std of holdout residuals at training time. Used as the σ in the normal-CDF distributional output.
  - `scaler.means` / `scaler.stds` (in metrics.json) — sanity-check that the raw conversion is reversible.
  - `column_means` (in metrics.json, NOT `TRAINING_MEDIANS`) — the values used to impute NaN at *training* time. **Production uses `TRAINING_MEDIANS` instead** — see Section 10 for the mismatch.
- **Imputation at inference** — [src/services/probability-model.ts:152-212](../src/services/probability-model.ts#L152-L212) defines `TRAINING_MEDIANS`. The actual imputation step:

```ts
// src/services/probability-model.ts:404-418
for (const key of featureKeys) {
  featureCount++;
  const value = features[key];
  if (value !== null && value !== undefined) {
    total += value * weights.coefficients[key];
  } else {
    // Impute with training median so the model behaves consistently
    // with how it was trained (sklearn imputes before fitting)
    const median = TRAINING_MEDIANS[key];
    if (median !== undefined) {
      total += median * weights.coefficients[key];
    }
    nullCount++;
  }
}
```

### XGBoost V1.6 (droplet, deployed but only 10% weight in production ensemble)

- **Inference code path**: [src/services/xgboost-client.ts:23-45](../src/services/xgboost-client.ts#L23-L45) — `predictXGB(features, medians)` POSTs to `https://xgb.edwinlovett.com/predict`. Batch variant at lines 51-75.
- **Training code path**: [training/xgb_hyperparam_sweep.py](../training/xgb_hyperparam_sweep.py) (sweep) and [training/xgb_player_experiment.py](../training/xgb_player_experiment.py) (V1.6 player-aware fit). The trained model lives on the droplet at `/opt/kelly-edge/models/xgboost_v1.6_player_aware.json` per CLAUDE.md.
- **Weights/artifact storage**: stored on the droplet, NOT in this repo. The Worker only knows the URL.
- **Feature contract**: same `FeatureVector` plus a `medians` dict — [src/services/xgboost-client.ts:51-75](../src/services/xgboost-client.ts#L51-L75):

```ts
export async function predictXGBBatch(
  games: Array<{ features: FeatureVector; medians: Record<string, number> }>
): Promise<XGBPrediction[] | null> {
  ...
  body: JSON.stringify({
    games: games.map((g) => ({ features: g.features, medians: g.medians })),
  }),
```

- **Output contract** ([src/services/xgboost-client.ts:14-17](../src/services/xgboost-client.ts#L14-L17)):

```ts
interface XGBPrediction {
  predicted_total: number;
  model_version: string;
}
```

  Point prediction only — no σ, no distributional output.
- **Calibration metadata**: none in the response. The Worker reuses the Lasso `residualStd` as the post-ensemble σ.
- **Imputation at inference**: client passes `TRAINING_MEDIANS` (from the Lasso constants) as the `medians` body field; the droplet handles imputation server-side.
- **Ensemble**: [src/services/xgboost-client.ts:101-119](../src/services/xgboost-client.ts#L101-L119) `ensemblePrediction(linearTotal, xgbTotal, linearWeight)` blends. Production uses `linearWeight=0.9` per [src/scheduled/pre-game-analysis.ts:99](../src/scheduled/pre-game-analysis.ts#L99) — but EXP-015 showed this is statistically equivalent to `linearWeight=1.0` on OOD data.

### Cold-start model

- **Inference code path**: [src/services/probability-model.ts:316-370](../src/services/probability-model.ts#L316-L370) — `coldStartPredict()`. Used when the DB has fewer than `MIN_COMPLETED_GAMES_FOR_FULL_MODEL = 50` ([src/services/probability-model.ts:216](../src/services/probability-model.ts#L216)) settled games. Fallback formula:

```ts
// src/services/probability-model.ts:339-362
const avgPace = features.avg_pace ?? leagueAvgPace;
const combinedOff = features.combined_off_rating ?? 224.0;
const combinedDef = features.combined_def_rating ?? 224.0;
const scoringEnv = (combinedOff + combinedDef) / 2;
let predictedTotal = scoringEnv * (avgPace / 100);
predictedTotal += 1.5;                 // home court
if (features.home_b2b === 1) predictedTotal -= 1.5;
if (features.away_b2b === 1) predictedTotal -= 1.5;
predictedTotal += (homeNet + awayNet) * 0.15;
return {
  predictedTotal: Math.round(predictedTotal * 10) / 10,
  predictedStd: 18.0,
  confidence: 0.35,
  modelVersion: `${modelVersion}-coldstart`,
};
```

- **Training code path**: none — closed-form heuristic.
- **Weights/artifact storage**: none — embedded in TS.
- **Feature contract**: same `FeatureVector`.
- **Output contract**: same `ModelPrediction` (with `-coldstart` suffix on `modelVersion`).
- **Calibration metadata**: hard-coded `predictedStd: 18.0`, `confidence: 0.35`.

### Panel Ridge (5 alphas — diversity panel for confidence boost only, not standalone)

- **Inference code path**: [src/services/shadow-runner.ts:154-164](../src/services/shadow-runner.ts#L154-L164) — `shadowPredict(features, weights)` runs each panel model. The panel agreement is consumed by [src/services/shadow-runner.ts:193-247](../src/services/shadow-runner.ts#L193-L247) `computePanelAgreement()` → [src/scheduled/pre-game-analysis.ts:404-421](../src/scheduled/pre-game-analysis.ts#L404-L421) where `agreementBoost = 0.8 + (panel.agreementRatio * 0.4)` multiplies the prediction's confidence.
- **Training code path**: [training/panel_backtest.py](../training/panel_backtest.py) and the offline Ridge sweep that produced [training/artifacts/panel-ridge-a1.0.json](../training/artifacts/panel-ridge-a1.0.json) … `panel-ridge-a3.0.json` (5 files, alphas 1.0/1.5/2.0/2.5/3.0).
- **Weights/artifact storage**: 5 JSON files in `training/artifacts/panel-ridge-a{1.0,1.5,2.0,2.5,3.0}.json`. Format mirrors V1.6 weights.json. Stored in production KV under shadow-model keys (registered via `POST /api/admin/shadow`).
- **Feature contract**: same `FeatureVector` (24 columns from `panel_backtest.py:FEATURE_KEYS` only — pre-V1.5 subset).
- **Output contract**: scalar predicted total per model.
- **Calibration metadata**: each panel JSON has its own `residualStd`.
- **Imputation**: [src/services/shadow-runner.ts:154-164](../src/services/shadow-runner.ts#L154-L164):

```ts
function shadowPredict(features: FeatureVector, weights: ShadowWeights): number {
  let total = weights.intercept;
  for (const [feat, coef] of Object.entries(weights.coefficients)) {
    const value = (features as unknown as Record<string, unknown>)[feat];
    if (value !== null && value !== undefined && typeof value === "number") {
      total += value * coef;
    }
    // null features use 0 contribution (same as training median * 0 for new features)
  }
  return Math.round(total * 10) / 10;
}
```

  **Note** the panel uses 0-imputation, not median-imputation. This is a conscious shortcut for shadows — it's not equivalent to the production Lasso path.

### V1.7 residual-target Lasso (attempted, audit-blocked, NOT live)

- **Inference code path**: none — never deployed.
- **Training code path**: [training/exp012_v17_residual_model.py](../training/exp012_v17_residual_model.py).
- **Weights/artifact storage**: [training/artifacts/exp012_v17_predictions.parquet](../training/artifacts/exp012_v17_predictions.parquet) (328 KB). The audit found the feature matrix was crippled; the artifact is kept only as evidence for the dead-end document.
- **Feature contract / output contract / calibration**: not load-bearing — superseded by EXP-013/EXP-015.

### BayesianRidge

**No examples found.** No file in [src/](../src/) or [training/](../training/) imports `BayesianRidge` or fits a Bayesian linear regression. `train_v16_final.py` uses `Lasso`; `walk_forward_backtest.py` uses `Lasso`; `panel_backtest.py` uses pre-fit Ridge JSON files but never re-fits. If MarketingCubes V2 wants Bayesian-style uncertainty bounds out of the box, it has no precedent in claw-edge — only the global frozen-σ approximation used by V1.6.

---

## Section 3: The experimental loop

### Experiment template

[docs/experiments/2026-04-29-v16-production-replay.md](experiments/2026-04-29-v16-production-replay.md) (EXP-013) section structure:

```
# Title (EXP-013: Production V1.6 weights × real closing lines × EV simulator)

**Date:** 2026-04-29
**Status:** complete
**Market:** NBA
**Concepts:** calibration, market-efficiency, ev-simulator, walk-forward
**Related hypotheses:** none
**Related issues:** ISS-014 (proposed)
**Related experiments:** EXP-010 (odds backfill), EXP-011 (...), EXP-012 (...)

## Hypothesis
## Method
  - Model
  - Inference
  - Evaluation
  - Data
  - Variants tested
  - Metrics

## Results
  ### Headline (with prominent audit corrections inline)
  ### Per-season (Variant A)
  ### Per-season (Variant B)
  ### Per-edge-bucket (Variant C)

## Interpretation

## What's Novel

## Follow-ups
  (each tagged as Hypothesis / Issue / Concept / Experiment / Doctrine)

## Reproducibility
  - Script
  - Inputs (with paths)
  - Output (with path + size)
  - Simulator commit
  - Run command
  - Walltime
  - Commit hash when run
```

Common section headers across the 19 experiment files (sampled across `2026-04-08-*`, `2026-04-29-*`, `2026-04-30-*`):

| Header | Present in |
|---|---|
| `# Title` | all |
| `**Date:**` / `**Status:**` / `**Market:**` / `**Concepts:**` | all |
| `**Related hypotheses/issues/experiments**` | most |
| `## Hypothesis` | most (some use `## Motivation` instead, e.g. walk-forward-backtest.md) |
| `## Method` | all |
| `## Results` (with subheads `### Headline`, `### Per-season`, `### Per-edge-bucket`) | all post-2026-04-08 |
| `## Interpretation` | most |
| `## What's Novel` | EXP-007, EXP-011, EXP-013, EXP-015 |
| `## Follow-ups` | all post-2026-04-08 (with sub-tags `Hypothesis (NEW: H###)` / `Issue (NEW: ISS-###)` / `Concept (to write)` / `Experiment to try`) |
| `## Open questions` | walk-forward-backtest.md |
| `## Decisions` | walk-forward-backtest.md, lasso-v16-breakthrough.md |
| `## Answers to the user's original questions` | walk-forward-backtest.md |
| `## Reproducibility` | all (with `**Script**`, `**Inputs**`, `**Output**`, `**Run command**`, `**Walltime**`, `**Commit hash when run**`) |

### Walk-forward implementation

File: [training/walk_forward_backtest.py](../training/walk_forward_backtest.py).

Fold-generation logic (expanding-window per-season, [training/walk_forward_backtest.py:76-89](../training/walk_forward_backtest.py#L76-L89)):

```python
ALL_SEASONS = [
    "2017-18", "2018-19", "2019-20", "2020-21",
    "2021-22", "2022-23", "2023-24", "2024-25",
]
EVAL_SEASONS = ALL_SEASONS[1:]
V16_ALPHA = 0.7  # matches production V1.6 Lasso
```

For each evaluation season S, train on all seasons strictly before S — see EXP-008 [docs/experiments/2026-04-08-walk-forward-backtest.md:62-73](experiments/2026-04-08-walk-forward-backtest.md):

> For each evaluation season S ∈ {2018-19, 2019-20, …, 2024-25}:
> 1. **Train** V1.6-style Lasso (α=0.7, StandardScaler, max_iter=20000) on all games from seasons strictly before S. Expanding window — the 2024-25 model trains on ~8,300 games from 7 prior seasons.
> 2. **Predict** every game in season S. The model has never seen any of these.
> 3. **Grade** against a proxy line = `avg_recent_total_10` (the rolling 10-game combined recent-total feature).
> 4. **Bet** when the model's `P(Over|proxy_line)` is ≥ 60% or ≤ 40%, mimicking the production 10% edge threshold against an uninformed 50% prior.

`stats_for_bets()` signature ([training/walk_forward_backtest.py:175-181](../training/walk_forward_backtest.py#L175-L181)):

```python
def stats_for_bets(
    preds: np.ndarray,
    y: np.ndarray,
    residual_std: float,
    proxy_lines: np.ndarray,
    edge_threshold: float = 0.10,
) -> dict[str, Any]:
```

Bet selection ([training/walk_forward_backtest.py:194-198](../training/walk_forward_backtest.py#L194-L198)):

```python
z = (proxy_lines - preds) / residual_std
prob_over = 1 - norm.cdf(z)

over_mask = prob_over >= (0.5 + edge_threshold)
under_mask = prob_over <= (0.5 - edge_threshold)
```

### PIT histogram implementation

Endpoint at [src/index.ts:139-234](../src/index.ts#L139-L234) (`POST /api/admin/trigger/pit-histogram`).

Bucketing logic ([src/index.ts:180-187](../src/index.ts#L180-L187)):

```ts
const buckets = {
  "within_0.5_sigma": zScores.filter((z) => Math.abs(z.z) <= 0.5).length,
  "within_1_sigma":   zScores.filter((z) => Math.abs(z.z) <= 1).length,
  "within_1.5_sigma": zScores.filter((z) => Math.abs(z.z) <= 1.5).length,
  "within_2_sigma":   zScores.filter((z) => Math.abs(z.z) <= 2).length,
  "within_3_sigma":   zScores.filter((z) => Math.abs(z.z) <= 3).length,
  "beyond_3_sigma":   zScores.filter((z) => Math.abs(z.z) > 3).length,
};
```

Z-score computation ([src/index.ts:168-177](../src/index.ts#L168-L177)):

```ts
const zScores = predictions.map((p) => ({
  z: (p.actual_total - p.predicted_total) / p.predicted_std,
  residual: p.actual_total - p.predicted_total,
  squared_residual: Math.pow(p.actual_total - p.predicted_total, 2),
  ...
}));
```

Reported expected percentages per bucket ([src/index.ts:223-230](../src/index.ts#L223-L230)):

```ts
within_0_5_sigma: { ..., expected_pct: 38.29 },
within_1_sigma:   { ..., expected_pct: 68.27 },
within_1_5_sigma: { ..., expected_pct: 86.64 },
within_2_sigma:   { ..., expected_pct: 95.45 },
within_3_sigma:   { ..., expected_pct: 99.73 },
beyond_3_sigma:   { ..., expected_pct: 0.27 },
```

### OOD-vs-IS breakout

Implemented in EXP-015 — [training/exp015_xgboost_vs_lasso.py:50](../training/exp015_xgboost_vs_lasso.py#L50) defines `SEASONS = ["2020-21", "2021-22", "2022-23", "2023-24", "2024-25"]`. The split is **implicit** — anything outside the model's training seasons (`["2022-23", "2023-24", "2024-25"]`) is OOD. The OOD subset is reconstructed in EXP-016 ([training/exp016_fit_calibration.py:44](../training/exp016_fit_calibration.py#L44)):

```python
OOD_SEASONS = ["2020-21", "2021-22"]
```

And used to filter the EXP-015 output ([training/exp016_fit_calibration.py:177-179](../training/exp016_fit_calibration.py#L177-L179)):

```python
preds = pd.read_parquet(EXP015_PARQUET)
ood = preds[preds["season"].isin(OOD_SEASONS)].copy()
```

The breakout in EXP-015's report (see [docs/experiments/2026-04-30-xgboost-vs-lasso-ood.md](experiments/2026-04-30-xgboost-vs-lasso-ood.md)) is generated by running the simulator twice — once on `season ∈ all` and once on `season ∈ OOD_SEASONS` — and reporting both headlines. The "in-sample" half is just the complement.

### Calibration ratio computation

[src/index.ts:196-222](../src/index.ts#L196-L222):

```ts
const sqResiduals = zScores.map((z) => z.squared_residual);
const empiricalVariance = sqResiduals.reduce((a, b) => a + b, 0) / n;
const empiricalStd = Math.sqrt(empiricalVariance);

const meanPredictedStd = predictions.reduce((s, p) => s + p.predicted_std, 0) / n;

return c.json({
  ...
  calibration_ratio: Math.round((empiricalStd / meanPredictedStd) * 100) / 100,
  ...
});
```

Formula: `calibration_ratio = empirical_residual_std / mean_predicted_std`. A value < 1.0 means the model is slightly *too* uncertain (predicted σ wider than actual residual spread).

### CRPS, log-likelihood, Brier score implementations

| Metric | File:line | Form |
|---|---|---|
| **Brier score** (model fitting) | [src/lib/calibrator.ts:94-103](../src/lib/calibrator.ts#L94-L103) | `rawBrier += (pred.rawProb - outcome) ** 2` (binary outcome, summed/n) |
| **Brier score** (Python mirror) | [training/exp016_fit_calibration.py:113-121](../training/exp016_fit_calibration.py#L113-L121) | Same formula |
| **Brier score** (walk-forward) | [training/walk_forward_backtest.py](../training/walk_forward_backtest.py) — `stats_for_bets` returns `brier` per-season; the per-season Brier scores are reported in [docs/experiments/2026-04-08-walk-forward-backtest.md:108-115](experiments/2026-04-08-walk-forward-backtest.md) |
| **Brier score** (settled-prediction backtest aggregate) | [src/lib/types.ts:341](../src/lib/types.ts#L341) — `BacktestRun.brier_score: number \| null` (column, populated by backtest jobs) |
| **CRPS** | **No examples found.** No `crps`, `continuous_ranked_probability_score`, or equivalent in [src/](../src/) or [training/](../training/). |
| **Log-likelihood** | **No examples found.** No `log_likelihood` or `nll` function in either codebase. The closest analog is the per-bucket calibration check in EXP-008 ([docs/experiments/2026-04-08-walk-forward-backtest.md:154-167](experiments/2026-04-08-walk-forward-backtest.md)). |

### Edge-bucket monotonicity check

Implemented inline in experiment scripts, not as a reusable function. Pattern from [training/exp013_v16_production_replay.py:252-271](../training/exp013_v16_production_replay.py#L252-L271):

```python
def per_edge_bucket(result):
    bets = result.bets[result.bets["recommended_side"].isin(["OVER", "UNDER"])].copy()
    if bets.empty:
        return
    bets["edge_bucket"] = pd.cut(
        bets["edge"].abs(),
        bins=[-1, 0, 0.03, 0.10, 0.20, 1],
        labels=["neg", "0-3%", "3-10%", "10-20%", ">20%"],
    )
    print(f"  {'bucket':<10} {'n':>5} {'win%':>6} {'EV/bet':>9} {'ROI':>7}")
    for bucket, sub in bets.groupby("edge_bucket", observed=True):
        if len(sub) == 0:
            continue
        w = (sub["result"] == "WIN").sum()
        l = (sub["result"] == "LOSS").sum()
        wp = 100 * w / max(1, w + l)
        ev = sub["ev_per_dollar"].mean() * 100
        roi = sub["profit_per_dollar"].mean() * 100
        print(f"  {str(bucket):<10} {len(sub):>5} {wp:>5.1f}% {ev:>+7.2f}% {roi:>+6.2f}%")
```

Same pattern in EXP-011 ([training/exp011_walk_forward_real_lines.py:179-194](../training/exp011_walk_forward_real_lines.py#L179-L194)) and EXP-015 ([training/exp015_xgboost_vs_lasso.py:286-295](../training/exp015_xgboost_vs_lasso.py#L286-L295)). The "monotonicity check" is visual only — there is no automated assertion that `WR(>20%) > WR(10-20%) > WR(3-10%)`. Findings are written into the experiment markdown's `## Interpretation` section.

### The audit pattern (experiments auditing earlier experiments)

This is one of the strongest organizational patterns in the repo. At least three explicit audit chains exist:

#### Audit 1 — EXP-013 audited EXP-011

**Found:** EXP-011's "57.5% / 54.0% real-line headline" was a regrade of *fold-local walk-forward Lasso predictions*, NOT the deployed V1.6 weights. The audit reviewer reproduced the gap: `predicted_total` from the walk-forward parquet differed from production V1.6 weights by an average of -3.05 points.

**Structure** (from [docs/experiments/2026-04-29-v16-production-replay.md:11-25](experiments/2026-04-29-v16-production-replay.md)):

> ## Hypothesis
> The audit of EXP-011 found that the "57.5% win rate against real lines" was a regrade of fold-local walk-forward Lasso models, NOT the production V1.6 weights deployed today. Audit reviewer reproduced the gap …
>
> **Specific claim:** when we replay actual production V1.6 weights through the (now-validated) EV simulator on the same 5 seasons of real sharp closing-line data, the win rate and EV/bet will exceed the fold-local Lasso's 53.5% …

The result table in EXP-013 confirmed the production model ran 56.49% / +8.33% on the same 5-season window — meaningfully above EXP-011's 53.5% fold-local number.

#### Audit 2 — EXP-015 audited EXP-013 (in-sample contamination)

**Found:** EXP-013's 56.49% all-seasons headline was inflated by ~2.4pp because 3 of 5 seasons were in-sample. OOD-only (2020-21 + 2021-22) was 54.10% — meaningfully closer to EXP-011's number.

**Structure** ([docs/experiments/2026-04-30-xgboost-vs-lasso-ood.md:52-77](experiments/2026-04-30-xgboost-vs-lasso-ood.md)):

> ### Headline — all seasons (illustrates the contamination)
> | Lasso 100/0 | 56.49% | +8.33% |  ← prior EXP-013 headline
> ### Headline — OOD only (true forward-looking)
> | Lasso 100/0 | 54.10% | +4.19% |
>
> OOD-only: **Lasso-only is the best variant**. XGBoost-only is the worst. Adding XGBoost weight monotonically *hurts* OOD performance.

A correction banner was inserted retroactively into [docs/experiments/2026-04-29-v16-production-replay.md:65-78](experiments/2026-04-29-v16-production-replay.md):

> 2. **EXP-015 audit (2026-04-30):** the headline below is the *all-seasons* number, which includes 3 seasons the model was trained on. **OOD-only** (2020-21 + 2021-22) is the honest forward-looking estimate: **Lasso 100/0 wins 54.10% / +4.19% ROI on 1,331 bets.** The 56.49% number here is inflated by ~2.4pp of in-sample contamination.

#### Audit 3 — Codex audit caught EXP-013 saving the wrong variant

**Found:** EXP-013's saved parquet artifact was Variant A (no playoff offset) when production runs Variant B (-18 offset). Saving the counterfactual misled downstream consumers (`CURRENT_STATE.md`, KB knowledge sync, Kelly sizing).

**Structure** (banner at [docs/experiments/2026-04-29-v16-production-replay.md:65-71](experiments/2026-04-29-v16-production-replay.md)):

> 1. **Codex audit (2026-04-29):** the saved artifact was Variant A (no offset) when production has the -18 offset active. Variant B is current production reality. (Patched in commit `3a172c7`.)

The fix was applied at [training/exp013_v16_production_replay.py:332-344](../training/exp013_v16_production_replay.py#L332-L344):

```python
# 4. Save CURRENT-PRODUCTION variant (B) as the default artifact.
# Variant A is a counterfactual ("what if we revert the offset") and
# must NOT be the headline. Production currently runs with the
# PLAYOFF_OFFSET_POINTS = -18 in src/services/probability-model.ts.
# Saving variant A here misleads downstream consumers (CURRENT_STATE.md,
# KB knowledge sync, Kelly sizing decisions). Audit blocker — see
# codex review 2026-04-29 for the original finding.
out = ART / "exp013_v16_production_replay.parquet"
res_b.bets.to_parquet(out, compression="snappy")
```

A guard test was added: [training/test_exp013_artifact_invariant.py](../training/test_exp013_artifact_invariant.py) — asserts the saved parquet is the production-fidelity variant.

#### Audit 4 — EXP-009 audited EXP-008 (calibration)

**Found:** EXP-008's "model is overconfident above 65% confidence" finding was almost entirely driven by 2018-19 (the year V1.6 had to predict the 3PT-era scoring boom with one season of training data). Excluding 2018-19, the calibration is fine through 75% confidence.

**Structure** (banner at [docs/experiments/2026-04-08-walk-forward-backtest.md:6-15](experiments/2026-04-08-walk-forward-backtest.md)):

> > **⚠️ READ FIRST: [EXP-009](./2026-04-08-postmortem-error-attribution.md) partially supersedes the calibration findings below.** The "model is overconfident above 65%" finding turned out to be **almost entirely driven by 2018-19** … Excluding 2018-19, the calibration is fine through 75% confidence and the pooled win rate jumps from 61.6% → **66.0% (CI 63.7-68.3%)** on 1,635 bets. The "discount Kelly by 8-13pp on high-confidence bets" recommendation in the Decisions section below has been **rescinded**.

#### The audit pattern itself

Every audit follows the same four moves:
1. **Quote the prior headline** verbatim from the experiment being audited.
2. **State the specific claim** — what the audit hypothesizes the prior experiment got wrong.
3. **Run the corrected version** (same data when possible, different methodology).
4. **Insert a banner** at the top of the audited experiment's file pointing to the new finding, with a link.

The repo treats experiment files as living documents — they are amended (with banners) but never deleted, so the audit chain is preserved.

---

## Section 4: The simulator

### Location

[training/simulator/](../training/simulator/) — Python package with `__init__.py`, `core.py`, `oddsmath.py`, `books.py`, `config.py`, and three test files (`test_core.py`, `test_oddsmath.py`, `test_books.py`). It is **not** part of the Worker — it runs locally during experiments.

Public top-level docstring ([training/simulator/__init__.py:1-4](../training/simulator/__init__.py#L1-L4)):

```python
"""EV simulator — canonical evaluation harness for NBA totals models.

See docs/concepts/ev-simulator.md for the full spec.
"""
```

### Public API (every exported function with verbatim signature)

#### `core.py`

```python
@dataclass
class EvalResult:
    bets: pd.DataFrame      # one row per game evaluated
    headline: dict          # aggregate metrics
    config: SimConfig
    run_id: str

    @property
    def placed(self) -> pd.DataFrame:
        """Subset of `bets` where a bet was actually placed (not SKIP)."""
        return self.bets[self.bets["recommended_side"].isin(["OVER", "UNDER"])]


def simulate(
    predictions: pd.DataFrame,
    odds: pd.DataFrame,
    actuals: pd.DataFrame,
    config: SimConfig,
    run_id: str = "default",
) -> EvalResult:
```

#### `config.py`

```python
@dataclass(frozen=True)
class SimConfig:
    bet_selection: str = "ev"
    bet_threshold: float = 0.02
    vig_method: str = "multiplicative"
    bet_sizing: str = "quarter_kelly"
    flat_unit_size: float = 100.0
    consensus_tier: str = "sharp_or_mid"
    global_residual_std: float = 17.278
    require_min_books: int = 2

def kelly_multiplier(bet_sizing: str) -> float:
```

#### `oddsmath.py`

```python
def american_to_implied(odds: int | float) -> float:
def american_to_decimal(odds: int | float) -> float:
def implied_to_american(prob: float) -> int:
def remove_vig_multiplicative(over_implied: float, under_implied: float) -> Tuple[float, float]:
def remove_vig_power(over_implied: float, under_implied: float, *, tol: float = 1e-9, max_iter: int = 100) -> Tuple[float, float]:
def standard_normal_cdf(z: float) -> float:
def prob_over_line(predicted_total: float, line: float, std: float) -> float:
def ev_per_dollar(p_win: float, american_price: int | float) -> float:
def kelly_fraction(p_win: float, american_price: int | float, fraction: float = 1.0) -> float:
def profit_per_dollar(side: str, american_price: int | float, actual_total: float, line: float) -> float:
```

#### `books.py`

```python
SHARP_BOOKS: frozenset[str] = frozenset({"pinnacle", "circa", "bookmaker"})
MID_BOOKS: frozenset[str] = frozenset({"fanduel", "betrivers", "williamhill_us", "unibet", "superbook"})
SOFT_BOOKS: frozenset[str] = frozenset({...11 books})

ALIASES: dict[str, str] = {
    "circasports": "circa",
    "unibet_us":   "unibet",
    "sugarhouse":  "betrivers",
}

def normalize(key: str) -> str:
def tier_of(key: str) -> str:
def is_sharp(key: str) -> bool:
def is_mid(key: str) -> bool:
def is_soft(key: str) -> bool:
def filter_by_tier(keys: Iterable[str], tier: str) -> list[str]:
def consensus_books(keys: Iterable[str]) -> list[str]:
```

### The bet-selection logic

[training/simulator/core.py:170-238](../training/simulator/core.py#L170-L238):

```python
# Generate candidate bets across (book, side)
candidates = []
for _, row in group.iterrows():
    book = str(row["bookmaker"])
    line = float(row["total_line"])
    over_price = float(row["over_price"])
    under_price = float(row["under_price"])

    # Model probability at this book's specific line — KEY POINT: we
    # do NOT use a single consensus line, we use each book's own.
    p_over = prob_over_line(pred, line, std)

    # Vig removal per book (also key — production was averaging
    # American odds across books, which is mathematically wrong).
    try:
        over_imp = american_to_implied(over_price)
        under_imp = american_to_implied(under_price)
    except ValueError:
        continue  # skip rows with invalid prices
    if config.vig_method == "multiplicative":
        over_fair, under_fair = remove_vig_multiplicative(over_imp, under_imp)
    else:
        over_fair, under_fair = remove_vig_power(over_imp, under_imp)

    # Per-side metrics
    over_edge = p_over - over_fair
    under_edge = (1 - p_over) - under_fair
    over_ev = ev_per_dollar(p_over, over_price)
    under_ev = ev_per_dollar(1 - p_over, under_price)
    over_kelly_full = kelly_fraction(p_over, over_price, fraction=1.0)
    under_kelly_full = kelly_fraction(1 - p_over, under_price, fraction=1.0)

    candidates.append({
        "book": book, "side": "OVER", "line": line, "price": over_price,
        "model_p": p_over, "fair_p": over_fair,
        "edge": over_edge, "ev": over_ev, "kelly_full": over_kelly_full,
    })
    candidates.append({
        "book": book, "side": "UNDER", "line": line, "price": under_price,
        "model_p": 1 - p_over, "fair_p": under_fair,
        "edge": under_edge, "ev": under_ev, "kelly_full": under_kelly_full,
    })

if not candidates:
    return _skip_row(...)

# Rank by selection metric.
metric_key = {
    "ev": "ev",
    "edge_pp": "edge",
    "kelly_stake": "kelly_full",
}[config.bet_selection]

best = max(candidates, key=lambda c: c[metric_key])
gate_passed = best[metric_key] >= config.bet_threshold
```

### The vig-removal logic

Multiplicative ([training/simulator/oddsmath.py:85-109](../training/simulator/oddsmath.py#L85-L109)):

```python
def remove_vig_multiplicative(
    over_implied: float,
    under_implied: float,
) -> Tuple[float, float]:
    _assert_implied(over_implied, "over_implied")
    _assert_implied(under_implied, "under_implied")
    total = over_implied + under_implied
    if total <= 0:
        raise ValueError(f"Implied probabilities sum to {total}; cannot remove vig")
    return over_implied / total, under_implied / total
```

Power ([training/simulator/oddsmath.py:112-148](../training/simulator/oddsmath.py#L112-L148)):

```python
def remove_vig_power(
    over_implied: float,
    under_implied: float,
    *,
    tol: float = 1e-9,
    max_iter: int = 100,
) -> Tuple[float, float]:
    _assert_implied(over_implied, "over_implied")
    _assert_implied(under_implied, "under_implied")
    lo, hi = 1.0, 10.0
    if over_implied ** hi + under_implied ** hi - 1 >= 0:
        hi = 50.0
    for _ in range(max_iter):
        mid = (lo + hi) / 2
        f = over_implied ** mid + under_implied ** mid - 1
        if abs(f) < tol:
            break
        if f > 0:
            lo = mid
        else:
            hi = mid
    k = (lo + hi) / 2
    return over_implied ** k, under_implied ** k
```

### The per-book line-shopping logic

The simulator does NOT use the production "best price" line shop. It generates every (book, side) candidate, computes per-book p_over at that book's specific line, and picks the **best metric** (EV, edge, or Kelly fraction depending on `config.bet_selection`).

This is deliberately different from the Worker — see the test that pins it down ([training/simulator/test_core.py:132-144](../training/simulator/test_core.py#L132-L144)):

```python
def test_g4_picks_best_price_book(self):
    """G4: pred=232, both books at line 220. Fanduel offers -105 over
    (better price than DK's -120). Simulator MUST pick fanduel.
    This is the line-shopping correctness check that production
    edge-calculator misses (averages consensus, then line-shops by
    raw price after the fact)."""
    result = simulate(self.predictions, self.odds, self.actuals, self.cfg)
    g4 = result.bets[result.bets["game_id"] == "G4"].iloc[0]

    self.assertEqual(g4["recommended_side"], "OVER")
    self.assertEqual(g4["chosen_bookmaker"], "fanduel")
    self.assertAlmostEqual(g4["chosen_price"], -105, places=4)
    self.assertEqual(g4["result"], "WIN")
```

### Test files

#### [training/simulator/test_oddsmath.py](../training/simulator/test_oddsmath.py)

| Test class | Asserts | Math pinned |
|---|---|---|
| `AmericanConversionTests` (~10 tests) | -110 → 0.5238, +120 → 0.4545, decimal +120 → 2.20, roundtrip 0.6→-150, 0.4→+150, 0.5→-100, 0 raises, NaN raises, invalid roundtrip raises | `american_to_implied`, `american_to_decimal`, `implied_to_american` |
| `VigRemovalTests` (~6) | balanced -110/-110 → 0.5/0.5, unbalanced -120/+100 sums to 1, zero raises, power method 0.5/0.5 symmetric, power vs multiplicative within 0.005 on balanced | `remove_vig_multiplicative`, `remove_vig_power` |
| `NormalCDFTests` (~5) | Φ(0)=0.5, Φ(1)=0.8413, Φ(-1)=0.1587, Φ(2)=0.9772, Φ(3)=0.9987 | `standard_normal_cdf` |
| `ProbOverLineTests` (~5) | At-line=0.5, above-line increases, below-line decreases, σ≤0 raises | `prob_over_line` |
| `EVTests` (~6) | 0.6 at -110 = +0.1455, breakeven at 110/210, P=0.5 at -110 < 0, P=1 at +100 → +1.0, P=0 → -1.0, invalid raises | `ev_per_dollar` |
| `KellyTests` (~5) | Full Kelly at 0.55/-110 = (b·0.55-0.45)/b, quarter = 1/4 of full, breakeven=0, negative-edge=0, large edge 0.99 at +100 = 0.98 | `kelly_fraction` |
| `ProfitPerDollarTests` (~7) | OVER win at -110 = +100/110, OVER loss = -1, push = 0, +120 win = +1.20, invalid side raises | `profit_per_dollar` |
| `RoundTripIntegrationTests` (~2) | -110 round-trip → -100 (0 vig), Kelly aligns with EV sign across (p,price) grid | composition of all primitives |

#### [training/simulator/test_books.py](../training/simulator/test_books.py)

| Test class | Asserts |
|---|---|
| `NormalizeTests` (~7) | circasports→circa, unibet_us→unibet, sugarhouse→betrivers, lowercase, None raises |
| `TierTests` (~7) | Sharp/mid/soft mappings, circasports resolves to sharp via alias, unknown defaults to soft, alias inherits canonical tier |
| `FilterByTierTests` (~5) | Filter by tier, sharp_or_mid, invalid raises |
| `ConsensusBooksTests` (~5) | Sharp preferred, falls back to mid, empty when only soft, circasports alias works |
| `CoverageTests` (~3) | All 25 distinct books observed in `odds_totals_*.parquet` are classified, no overlap between tier sets, every alias resolves to a tier |

#### [training/simulator/test_core.py](../training/simulator/test_core.py)

| Test class | Asserts | Math pinned |
|---|---|---|
| `SmokeFixtureTests` (6 tests, hand-graded fixture: 4 games × 2 books) | G1 strong OVER at -110 wins, G2 mirror UNDER, G3 fair market skipped, G4 picks fanduel over DK at better price, headline aggregates 3W-0L-1SKIP, consensus uses mid-tier when sharp absent | End-to-end: feature → p_over → edge → EV → Kelly → grade |
| `CircaSportsAliasingTest` | Game with only `circasports` + `draftkings` → `consensus_line` = circa's 219.5 (sharp) | book alias resolution in `consensus_books` |
| `ReplayEXP011Tests` | Loads real EXP-011 parquet, runs simulator, asserts win_rate ∈ [0.51, 0.56] (audit-validated band of 53.5%) | full real-data integration |

### Invocation from an experiment

[training/exp013_v16_production_replay.py:212-227](../training/exp013_v16_production_replay.py#L212-L227):

```python
def run_variant(name: str, predictions, odds, actuals, cfg: SimConfig):
    print(f"\n{'='*72}")
    print(f"VARIANT: {name}")
    print(f"  bet_selection={cfg.bet_selection}  threshold={cfg.bet_threshold}")
    print(f"  vig_method={cfg.vig_method}")
    print(f"{'='*72}")
    result = core.simulate(predictions, odds, actuals, cfg, run_id=name)
    h = result.headline
    print(f"  Games:       {h['n_games_evaluated']:,}")
    print(f"  Bets placed: {h['n_bets_placed']:,}  ({h['n_skipped']:,} skipped)")
    print(f"  W-L-P:       {h['wins']}-{h['losses']}-{h['pushes']}")
    print(f"  Win rate:    {h['win_rate']*100:.2f}%")
    print(f"  EV/bet:      {h['ev_per_bet_pct']:+.2f}%")
    print(f"  Realized ROI: {h['realized_roi_pct']:+.2f}%  per dollar staked, flat unit")
    print(f"  Avg CLV:     {h['avg_clv_points']}")
    return result
```

The same script invokes 3 variants ([training/exp013_v16_production_replay.py:307-326](../training/exp013_v16_production_replay.py#L307-L326)):

```python
cfg_a = SimConfig(bet_selection="edge_pp", bet_threshold=0.10,
                  vig_method="multiplicative", require_min_books=1)
res_a = run_variant("A. base (edge_pp 10%, no offset)",
                    preds_no_offset, odds, actuals, cfg_a)

res_b = run_variant("B. with_offset (-18 in playoff window, edge_pp 10%)",
                    preds_with_offset, odds, actuals, cfg_a)

cfg_c = SimConfig(bet_selection="ev", bet_threshold=0.02,
                  vig_method="multiplicative", require_min_books=1)
res_c = run_variant("C. EV gate (>= 2% per bet, no offset)",
                    preds_no_offset, odds, actuals, cfg_c)
```

EXP-015 ([training/exp015_xgboost_vs_lasso.py:188-197](../training/exp015_xgboost_vs_lasso.py#L188-L197)) uses an even simpler call pattern:

```python
def run_simulator(predictions_df, odds, actuals, std, label):
    preds = predictions_df[["game_id"]].copy()
    preds["predicted_total"] = predictions_df["pred"]
    preds["predicted_std"] = std
    preds["model_version"] = label
    cfg = SimConfig(
        bet_selection="edge_pp", bet_threshold=0.10,
        vig_method="multiplicative", require_min_books=1,
    )
    return core.simulate(preds, odds, actuals, cfg, run_id=label)
```

---

## Section 5: Data and persistence

### D1 schema — full CREATE TABLE statements

From [schema/schema.sql](../schema/schema.sql) plus 19 migrations applied in order. The combined effective schema (with all `ALTER TABLE`s applied):

#### `bookmakers` ([schema/schema.sql:9-33](../schema/schema.sql#L9-L33))

```sql
CREATE TABLE IF NOT EXISTS bookmakers (
  key TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  tier TEXT NOT NULL DEFAULT 'soft',
  region TEXT NOT NULL DEFAULT 'us',
  is_active BOOLEAN DEFAULT TRUE,
  notes TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);
```

Seeds 13 books: pinnacle/circa/bookmaker (sharp), fanduel/betrivers/williamhill_us/unibet (mid), draftkings/betmgm/caesars/pointsbetus/bovada/barstool (soft). Migration 001 adds `hardrock` and `fliff` (both soft).

#### `games` ([schema/schema.sql:36-58](../schema/schema.sql#L36-L58))

```sql
CREATE TABLE IF NOT EXISTS games (
  id TEXT PRIMARY KEY,
  bdl_game_id INTEGER,
  sport_key TEXT NOT NULL DEFAULT 'basketball_nba',
  home_team TEXT NOT NULL,
  away_team TEXT NOT NULL,
  commence_time TEXT NOT NULL,
  home_score INTEGER,
  away_score INTEGER,
  total_score INTEGER,
  completed BOOLEAN DEFAULT FALSE,
  season INTEGER,
  is_home_b2b BOOLEAN DEFAULT FALSE,
  is_away_b2b BOOLEAN DEFAULT FALSE,
  home_rest_days INTEGER,
  away_rest_days INTEGER,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_games_commence ON games(commence_time);
CREATE INDEX idx_games_completed ON games(completed);
CREATE INDEX idx_games_season ON games(season);
```

#### `odds_snapshots` ([schema/schema.sql:61-84](../schema/schema.sql#L61-L84))

```sql
CREATE TABLE IF NOT EXISTS odds_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  game_id TEXT NOT NULL REFERENCES games(id),
  bookmaker_key TEXT NOT NULL REFERENCES bookmakers(key),
  bookmaker_title TEXT NOT NULL,
  market_key TEXT NOT NULL,
  snapshot_time TEXT NOT NULL,
  over_price INTEGER,
  under_price INTEGER,
  total_line REAL,
  over_implied_prob REAL,
  under_implied_prob REAL,
  vig REAL,
  fair_over_prob REAL,
  fair_under_prob REAL,
  is_opening BOOLEAN DEFAULT FALSE,
  is_closing BOOLEAN DEFAULT FALSE,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_odds_game ON odds_snapshots(game_id);
CREATE INDEX idx_odds_game_book ON odds_snapshots(game_id, bookmaker_key);
CREATE INDEX idx_odds_snapshot_time ON odds_snapshots(snapshot_time);
CREATE INDEX idx_odds_closing ON odds_snapshots(is_closing);
-- migration-002:
CREATE INDEX idx_odds_game_time_book ON odds_snapshots(game_id, snapshot_time, bookmaker_key);
-- migration-005 → migration-009 superseded:
CREATE UNIQUE INDEX idx_odds_dedup ON odds_snapshots(game_id, bookmaker_key, market_key, snapshot_time);
```

#### `team_stats` ([schema/schema.sql:87-108](../schema/schema.sql#L87-L108)) + migrations 003/006/008/016

```sql
CREATE TABLE IF NOT EXISTS team_stats (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  team_name TEXT NOT NULL,
  season INTEGER NOT NULL,
  as_of_date TEXT NOT NULL,
  pace REAL,
  offensive_rating REAL,
  defensive_rating REAL,
  net_rating REAL,
  avg_total_last_5 REAL,
  avg_total_last_10 REAL,
  avg_pace_last_5 REAL,
  wins INTEGER,
  losses INTEGER,
  home_wins INTEGER,
  home_losses INTEGER,
  away_wins INTEGER,
  away_losses INTEGER,
  -- migration-003 (V1.1 shooting):
  ts_pct REAL,
  efg_pct REAL,
  pts_avg REAL,
  fg3_pct REAL,
  -- migration-006 (V1.2 scoring + possession):
  possessions REAL,
  turnovers REAL,
  fg3_attempted REAL,
  ft_attempted REAL,
  plus_minus REAL,
  -- migration-008 (V1.5 BDL):
  oreb_pct REAL,
  stl REAL,
  pfd REAL,
  tm_tov_pct REAL,
  ast_to REAL,
  -- migration-016:
  ft_pct REAL,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_team_stats_lookup ON team_stats(team_name, season, as_of_date);
```

#### `features` ([schema/schema.sql:111-140](../schema/schema.sql#L111-L140)) + migrations 002/004/006/019

After migration 019 (the ISS-005 fix), the table has 67 columns: `id`, `game_id`, `model_version`, `computed_at`, all 50 features (V1.0–V1.6), 8 metadata columns (`sharp_consensus_line`, `market_consensus_line`, `sharp_over_implied`, `predicted_total`, `model_confidence`, `edge_points`, `edge_direction`, `edge_threshold_met`), and `created_at`.

```sql
CREATE TABLE IF NOT EXISTS features (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  game_id TEXT NOT NULL REFERENCES games(id),
  model_version TEXT NOT NULL,
  computed_at TEXT NOT NULL,
  -- V1.0 (12)
  avg_pace REAL, pace_delta REAL,
  combined_off_rating REAL, combined_def_rating REAL,
  home_net_rating REAL, away_net_rating REAL,
  rest_disparity INTEGER,
  home_b2b INTEGER, away_b2b INTEGER,
  avg_recent_total_5 REAL, avg_recent_total_10 REAL,
  home_away_factor REAL,
  -- migration-004 (V1.1, 6)
  combined_ts_pct REAL, combined_efg_pct REAL,
  home_win_pct REAL, away_win_pct REAL,
  home_home_win_pct REAL, away_road_win_pct REAL,
  -- migration-006 (V1.2, 6)
  combined_pts_avg REAL, combined_possessions REAL,
  combined_turnovers REAL, combined_fg3_rate REAL,
  combined_ft_rate REAL, pts_diff REAL,
  -- migration-019 (V1.3, 10):
  scoring_trend REAL, home_off_vs_away_def REAL, away_off_vs_home_def REAL,
  total_volatility REAL, h2h_last_total REAL, h2h_avg_total REAL,
  h2h_games_played REAL, home_recent_total_avg REAL,
  away_recent_total_avg REAL, margin_of_victory REAL,
  -- migration-019 (V1.4, 1):
  combined_assists REAL,
  -- migration-019 (V1.5, 9):
  combined_oreb_pct REAL, combined_stl REAL, combined_pfd REAL,
  combined_tm_tov_pct REAL, combined_ft_pct REAL,
  home_home_win_pct_real REAL, away_road_win_pct_real REAL,
  odds_spread REAL, odds_moneyline_implied REAL,
  -- migration-019 (V1.6, 10):
  home_expected_starter_pts REAL, home_missing_starter_pts REAL,
  home_expected_starter_count REAL, home_top_scorer_recent_played_pct REAL,
  home_missing_top_scorers REAL,
  away_expected_starter_pts REAL, away_missing_starter_pts REAL,
  away_expected_starter_count REAL, away_top_scorer_recent_played_pct REAL,
  away_missing_top_scorers REAL,
  -- Metadata
  sharp_consensus_line REAL, market_consensus_line REAL,
  sharp_over_implied REAL,
  predicted_total REAL, model_confidence REAL,
  edge_points REAL, edge_direction TEXT, edge_threshold_met BOOLEAN,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_features_game ON features(game_id);
CREATE INDEX idx_features_edge ON features(edge_threshold_met);
CREATE UNIQUE INDEX idx_features_game_model ON features(game_id, model_version);  -- migration-002
```

#### `predictions` ([schema/schema.sql:143-178](../schema/schema.sql#L143-L178)) + migrations 002/010/014/015/018

```sql
CREATE TABLE IF NOT EXISTS predictions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  game_id TEXT NOT NULL REFERENCES games(id),
  model_version TEXT NOT NULL,
  predicted_at TEXT NOT NULL,
  predicted_total REAL NOT NULL,
  predicted_std REAL,
  confidence REAL NOT NULL,
  sharp_total_line REAL NOT NULL,
  market_total_line REAL NOT NULL,
  market_over_implied REAL NOT NULL,
  market_under_implied REAL NOT NULL,
  best_over_book TEXT, best_over_price INTEGER,
  best_under_book TEXT, best_under_price INTEGER,
  best_over_line REAL, best_under_line REAL,
  recommended_side TEXT, recommended_book TEXT,
  edge_points REAL,
  model_over_prob REAL, market_implied_prob REAL,
  edge_probability REAL,
  kelly_fraction REAL, quarter_kelly_fraction REAL, suggested_units REAL,
  actual_total INTEGER, result TEXT,
  closing_line REAL, closing_over_implied REAL,
  clv_points REAL, clv_positive BOOLEAN,
  -- migration-010:
  clv_probability REAL,
  -- migration-014:
  unadjusted_total REAL, lineup_adjustment REAL,
  bottom_up_total REAL, divergence REAL,
  adjustment_source TEXT, adjusted_at TEXT,
  -- migration-015:
  residual REAL, squared_residual REAL, z_score REAL,
  -- migration-018:
  confirmation_status TEXT DEFAULT 'preliminary',
  cancel_reason TEXT, confirmed_at TEXT,
  missing_scorers_at_predict INTEGER,
  missing_scorers_at_confirm INTEGER,
  created_at TEXT DEFAULT (datetime('now')),
  settled_at TEXT
);

CREATE INDEX idx_predictions_game ON predictions(game_id);
CREATE INDEX idx_predictions_model ON predictions(model_version);
CREATE INDEX idx_predictions_result ON predictions(result);
CREATE INDEX idx_predictions_clv ON predictions(clv_positive);
CREATE INDEX idx_predictions_game_model_result ON predictions(game_id, model_version, result);  -- migration-002
CREATE UNIQUE INDEX idx_predictions_game_model_unique ON predictions(game_id, model_version);   -- migration-002
CREATE INDEX idx_predictions_confirmation ON predictions(confirmation_status);                   -- migration-018
```

#### `backtest_runs` ([schema/schema.sql:186-214](../schema/schema.sql#L186-L214))

```sql
CREATE TABLE IF NOT EXISTS backtest_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_name TEXT NOT NULL, model_version TEXT NOT NULL,
  start_date TEXT NOT NULL, end_date TEXT NOT NULL,
  edge_threshold REAL NOT NULL, bet_sizing TEXT NOT NULL,
  initial_bankroll REAL NOT NULL, unit_size REAL,
  total_bets INTEGER, wins INTEGER, losses INTEGER, pushes INTEGER,
  win_rate REAL, roi REAL, total_profit_loss REAL, max_drawdown REAL,
  avg_edge_points REAL, avg_clv REAL, clv_positive_rate REAL,
  final_bankroll REAL, brier_score REAL,
  flat_roi REAL, quarter_kelly_roi REAL, half_kelly_roi REAL, full_kelly_roi REAL,
  created_at TEXT DEFAULT (datetime('now'))
);
```

#### `model_versions` ([schema/schema.sql:217-227](../schema/schema.sql#L217-L227))

```sql
CREATE TABLE IF NOT EXISTS model_versions (
  id TEXT PRIMARY KEY,
  description TEXT,
  feature_set TEXT NOT NULL,
  algorithm TEXT NOT NULL,
  hyperparameters TEXT, training_seasons TEXT, training_env TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  is_active BOOLEAN DEFAULT FALSE
);
```

#### Bettor intelligence tables — `bettors` / `market_types` / `wagers` / `bankroll_accounts` / `bankroll_transactions` / `wager_tags`

[schema/schema.sql:234-345](../schema/schema.sql#L234-L345). The `wagers` table is the most heavily indexed (10 indexes for query patterns by bettor / game / market / book / result / placed_at / clv_positive / model_agreed / source / season / bet_type / external).

#### `system_log` (migration-007)

```sql
CREATE TABLE IF NOT EXISTS system_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL DEFAULT (datetime('now')),
  level TEXT NOT NULL,          -- 'info' | 'warn' | 'error' | 'debug'
  category TEXT NOT NULL,       -- 'cron_run' | 'cron_error' | 'api_call' | ...
  message TEXT NOT NULL,
  context TEXT,                 -- JSON blob: game_id, model_version, ...
  resolved BOOLEAN DEFAULT FALSE
);
```

#### `players` / `player_game_stats` / `player_season_averages` (migration-011)

```sql
CREATE TABLE IF NOT EXISTS players (
  id TEXT PRIMARY KEY,                -- "bdl:{bdl_player_id}"
  bdl_player_id INTEGER UNIQUE,
  first_name TEXT NOT NULL, last_name TEXT NOT NULL, full_name TEXT NOT NULL,
  position TEXT, team_name TEXT,
  sport_key TEXT NOT NULL DEFAULT 'basketball_nba',
  is_active BOOLEAN DEFAULT TRUE,
  created_at TEXT, updated_at TEXT
);

CREATE TABLE IF NOT EXISTS player_game_stats (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  player_id TEXT NOT NULL REFERENCES players(id),
  game_id TEXT REFERENCES games(id),
  bdl_game_id INTEGER,
  game_date TEXT NOT NULL,
  season INTEGER NOT NULL,
  team_name TEXT NOT NULL, opponent_name TEXT,
  is_home BOOLEAN, is_starter BOOLEAN,
  minutes TEXT, minutes_decimal REAL,
  pts INTEGER, reb INTEGER, ast INTEGER, stl INTEGER, blk INTEGER, tov INTEGER,
  fgm INTEGER, fga INTEGER, fg_pct REAL,
  fg3m INTEGER, fg3a INTEGER, fg3_pct REAL,
  ftm INTEGER, fta INTEGER, ft_pct REAL,
  oreb INTEGER, dreb INTEGER, pf INTEGER, plus_minus INTEGER,
  sport_key TEXT NOT NULL DEFAULT 'basketball_nba',
  created_at TEXT
);

CREATE TABLE IF NOT EXISTS player_season_averages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  player_id TEXT NOT NULL REFERENCES players(id),
  season INTEGER NOT NULL,
  as_of_date TEXT NOT NULL,
  games_played INTEGER, minutes_avg REAL,
  pts_avg REAL, reb_avg REAL, ast_avg REAL, stl_avg REAL, blk_avg REAL, tov_avg REAL,
  fg_pct REAL, fg3_pct REAL, ft_pct REAL,
  fg3a_avg REAL, fta_avg REAL, plus_minus_avg REAL,
  pts_last_5 REAL, pts_last_10 REAL, reb_last_5 REAL, reb_last_10 REAL,
  ast_last_5 REAL, ast_last_10 REAL, minutes_last_5 REAL,
  sport_key TEXT, created_at TEXT
);
-- migration-017 changed unique constraint to (player_id, season) only
CREATE UNIQUE INDEX idx_psa_dedup ON player_season_averages(player_id, season);
```

#### `player_availability` (migration-012)

```sql
CREATE TABLE IF NOT EXISTS player_availability (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  player_id TEXT NOT NULL REFERENCES players(id),
  game_id TEXT NOT NULL REFERENCES games(id),
  status TEXT NOT NULL,           -- 'in', 'out', 'gtd', 'probable', 'questionable'
  source TEXT NOT NULL DEFAULT 'bdl_lineup',
  detected_at TEXT NOT NULL,
  notes TEXT, created_at TEXT
);
```

#### `player_prop_odds` / `player_predictions` (migration-013)

Schemas verbatim in [schema/migration-013-player-props.sql](../schema/migration-013-player-props.sql).

#### Views ([schema/schema.sql:352-433](../schema/schema.sql#L352-L433))

`v_bettor_market_performance`, `v_bettor_sportsbook_performance`, `v_bettor_monthly_performance`, `v_bettor_vs_model`, `v_bankroll_equity_curve`. All read-only aggregates over `wagers` and `bankroll_transactions`.

### R2 objects in use

```ts
// src/services/walk-forward-loader.ts:28
const R2_KEY = "walk-forward/v1.6-lasso-player.json";

// src/services/walk-forward-loader.ts:44
const obj = await env.R2.get(R2_KEY);
```

R2 is referenced *only* for the walk-forward dataset — a JSON dump of `walk_forward_predictions_20260408.parquet` for the public Lab page. No other R2 keys are written from the Worker. The bucket is `claw-edge-data`.

### KV keys in use

| Key | Writers | Readers | Purpose |
|---|---|---|---|
| `model:weights:{version}` | `npx wrangler kv key put` (manual) | [src/services/probability-model.ts:271](../src/services/probability-model.ts#L271) `loadWeights` | Hot-swappable model weights |
| `calibration:map:current` | [src/index.ts:313](../src/index.ts#L313) `/api/admin/calibrate` | [src/scheduled/pre-game-analysis.ts:69](../src/scheduled/pre-game-analysis.ts#L69) | Isotonic calibration map |
| `shadow:models` | [src/services/shadow-runner.ts:182](../src/services/shadow-runner.ts#L182) | [src/services/shadow-runner.ts:43](../src/services/shadow-runner.ts#L43) | Shadow model registry |
| `panel:agreement:latest` | [src/scheduled/pre-game-analysis.ts:427](../src/scheduled/pre-game-analysis.ts#L427) | dashboard/notifications | Panel agreement summary |
| `lineup:adjustments:latest` | [src/index.ts:441](../src/index.ts#L441) `lineupCheck` | notifications | Lineup adjustments JSON (TTL 86400) |
| `confirmation:latest` | [src/scheduled/confirmation-pass.ts:208](../src/scheduled/confirmation-pass.ts#L208) | notifications | Confirmation pass summary (TTL 86400) |
| `sync:odds:last` | [src/services/odds-ingestion.ts:207](../src/services/odds-ingestion.ts#L207) | [src/routes/health.ts:32](../src/routes/health.ts#L32) | Last odds sync metadata |
| `sync:stats:last` | [src/services/stats-ingestion.ts:457](../src/services/stats-ingestion.ts#L457) [:701](../src/services/stats-ingestion.ts#L701) | health | Last stats sync metadata |
| `sync:settlement:last` | [src/scheduled/post-game-settlement.ts:67](../src/scheduled/post-game-settlement.ts#L67) | dashboards | Last settlement metadata |
| `sync:prop_settlement:last` | [src/scheduled/post-game-settlement.ts:125](../src/scheduled/post-game-settlement.ts#L125) | dashboards | Last prop settlement metadata (TTL 86400 × 7) |
| `sync:analysis:last` | [src/scheduled/pre-game-analysis.ts:442](../src/scheduled/pre-game-analysis.ts#L442) | dashboards | Last pre-game analysis metadata |
| `walk-forward:v1.6-lasso-player` | [src/services/walk-forward-loader.ts:57](../src/services/walk-forward-loader.ts#L57) | [src/services/walk-forward-loader.ts:34](../src/services/walk-forward-loader.ts#L34) | Cached R2 walk-forward JSON (TTL 24h) |
| `alert:odds:quota_exhausted` | odds-ingestion.ts (alarm setter) | [src/routes/health.ts:93](../src/routes/health.ts#L93) | Odds API quota exhaustion flag |
| `ratelimit:{ip}:{window}` | [src/middleware/rate-limit.ts:54](../src/middleware/rate-limit.ts#L54) | [src/middleware/rate-limit.ts:40](../src/middleware/rate-limit.ts#L40) | Per-IP rate-limit counter |
| various `sync:player_*:last` | [src/services/player-stats-backfill.ts:131](../src/services/player-stats-backfill.ts#L131), [src/services/player-stats-ingestion.ts:261](../src/services/player-stats-ingestion.ts#L261), [src/services/player-prop-ingestion.ts:179](../src/services/player-prop-ingestion.ts#L179), [src/services/injury-sync.ts:313](../src/services/injury-sync.ts#L313) | dashboards | Per-domain sync metadata |

### Parquet artifacts

All under [training/artifacts/](../training/artifacts/) and [training/data/](../training/data/) (the latter not listed earlier — those are raw input parquets from BDL backfill).

| Path | Size | Contents |
|---|---|---|
| `training/artifacts/walk_forward_predictions_20260408.parquet` | 915 KB | 8,284 rows × ~12 cols — V1.6 walk-forward predictions across 7 seasons w/ proxy line + `bet_result` |
| `training/artifacts/exp011_walk_forward_real_lines.parquet` | 917 KB | 5,582 rows × 76 cols — walk-forward joined with real consensus + naive/full regrade outcomes |
| `training/artifacts/exp012_v17_predictions.parquet` | 329 KB | V1.7 residual-target Lasso predictions (audit-blocked, kept as evidence) |
| `training/artifacts/exp013_v16_production_replay.parquet` | 303 KB | 5,582 rows × 22 cols — production V1.6 + real lines, "Variant B" with -18 offset |
| `training/artifacts/exp015_xgboost_vs_lasso.parquet` | 117 KB | Lasso + XGBoost predictions side-by-side w/ season label |
| `training/artifacts/exp017_opening_vs_closing.parquet` | 9 KB | Opening vs closing edge analysis |
| `training/artifacts/odds_events_{2020_21..2024_25}.parquet` | ~60 KB each | Odds events listing from EXP-010 backfill |
| `training/artifacts/odds_totals_{2020_21..2024_25}.parquet` | 112-136 KB each | Per-book closing-line totals (the 84,244-row dataset used by the simulator) |
| `training/artifacts/odds_props_assists_points_rebounds_{2023_24,2024_25}.parquet` | 1.7 / 3.2 MB | Player prop historical odds |
| `training/artifacts/odds_movement_2025-01-15.parquet` | 24 KB | One-day movement sample |
| `training/artifacts/calibration_map_v1.6_ood.json` | n/a | EXP-016's fitted calibration map (JSON, not parquet) |
| `training/artifacts/lasso_v16_20260408_020552/{weights,metrics}.json` | 1.8 KB / 9.3 KB | Production V1.6 weights |
| `training/artifacts/panel-ridge-a{1.0,1.5,2.0,2.5,3.0}.json` | n/a | 5-alpha Ridge panel |
| `training/artifacts/postmortem_20260408.json` | n/a | EXP-009 error attribution |
| `training/artifacts/walk_forward_20260408.json` | n/a | EXP-008 walk-forward run summary |
| `training/artifacts/retrotest_v16_production_20260408.json` | n/a | EXP-007 retrotest output |
| `training/artifacts/odds_backfill_state.json` | n/a | EXP-010 resume state |

### Migrations

| File | One-line summary |
|---|---|
| `migration-001-bettor-import.sql` | Add `wagers.bet_type` / `parlay_legs` / `external_bet_id`; seed Hard Rock + Fliff books; add player_prop / game_exclusive / other market types |
| `migration-002-indexes-and-constraints.sql` | Composite indexes on hot query paths; UNIQUE `(game_id, model_version)` on `features` and `predictions` for idempotency |
| `migration-003-shooting-stats.sql` | `team_stats` += `ts_pct`, `efg_pct`, `pts_avg`, `fg3_pct` (V1.1) |
| `migration-004-v11-features.sql` | `features` += V1.1 columns (combined_ts_pct, combined_efg_pct, win pcts) |
| `migration-005-odds-dedup.sql` | UNIQUE index `(game_id, bookmaker_key, snapshot_time)` (later upgraded by migration-009) |
| `migration-006-v12-scoring-features.sql` | `team_stats` += V1.2 columns; `features` += V1.2 columns |
| `migration-007-system-log.sql` | New `system_log` table for structured ops logging |
| `migration-008-v15-live-features.sql` | `team_stats` += V1.5 BDL enrichment (oreb_pct, stl, pfd, tm_tov_pct, ast_to) |
| `migration-009-multi-market-dedup.sql` | Replace odds_dedup index with `(game_id, bookmaker_key, market_key, snapshot_time)` to support multi-market storage |
| `migration-010-clv-probability.sql` | `predictions` += `clv_probability` |
| `migration-011-player-stats.sql` | New `players`, `player_game_stats`, `player_season_averages` |
| `migration-012-player-availability.sql` | New `player_availability` |
| `migration-013-player-props.sql` | New `player_prop_odds`, `player_predictions` |
| `migration-014-prediction-adjustments.sql` | `predictions` += `unadjusted_total`, `lineup_adjustment`, `bottom_up_total`, `divergence`, `adjustment_source`, `adjusted_at` |
| `migration-015-squared-residual.sql` | `predictions` += `residual`, `squared_residual`, `z_score` (variance-model substrate) |
| `migration-016-team-stats-ft-pct.sql` | `team_stats` += `ft_pct` (out-of-band fix for prior schema drift) |
| `migration-017-dedup-player-season-averages.sql` | Delete duplicate rows; replace UNIQUE index `(player_id, season, as_of_date)` → `(player_id, season)` (root-cause fix for ISS-009) |
| `migration-018-confirmation-status.sql` | `predictions` += `confirmation_status`, `cancel_reason`, `confirmed_at`, `missing_scorers_at_predict`, `missing_scorers_at_confirm` |
| `migration-019-complete-features.sql` | `features` += V1.3 + V1.4 + V1.5 + V1.6 columns (30 ALTERs); fixes ISS-005 |

### The `features` table specifically

After migration-019 the table has all 50 features defined in `FeatureVector`. Per [docs/issues/ISS-005-features-table-insert-incomplete.md](issues/ISS-005-features-table-insert-incomplete.md), prior to migration-019 the table had only 24 of 50 feature columns and the INSERT in [src/scheduled/pre-game-analysis.ts](../src/scheduled/pre-game-analysis.ts) silently dropped V1.3+ columns. **In production right now, all 50 columns are present and the INSERT at [src/scheduled/pre-game-analysis.ts:135-180](../src/scheduled/pre-game-analysis.ts#L135-L180) writes all of them.**

The INSERT explicitly enumerates all 65 columns (50 features + 15 metadata) — verify by counting `?` placeholders against `.bind()` arguments. The fix added a comment explicitly tracking the count groups by version: V1.0(12)/V1.1(6)/V1.2(6)/V1.3(10)/V1.4(1)/V1.5(9)/V1.6(10).

The historical impact (per ISS-005 "Impact on Results"): every prediction made between V1.3 deployment and 2026-04-11 has an incomplete features-table row. The in-memory feature vector flowed through the model correctly (predictions are unaffected), but **any backtest reading from the features table will use NULL for missing columns and median-impute** — producing different results than what was actually predicted.

---

## Section 6: Feature engineering

### V2 player feature builder

[training/build_player_features_v2.py](../training/build_player_features_v2.py).

Public function ([training/build_player_features_v2.py:47-143](../training/build_player_features_v2.py#L47-L143)):

```python
def build_features_for_team_game(
    player_df: pd.DataFrame,
    team: str,
    game_date,
    season: int,
) -> dict:
    """V2: Identify starters by season PPG + minutes, use 30-game lookback."""
    team_games = player_df[
        (player_df["team_name"] == team) & (player_df["season"] == season)
    ]

    nan_result = {
        "expected_starter_pts": np.nan,
        "missing_starter_pts": np.nan,
        "expected_starter_count": np.nan,
        "top_scorer_recent_played_pct": np.nan,
        "missing_top_scorers": np.nan,
    }
    ...
```

The "starters" definition ([training/build_player_features_v2.py:90-103](../training/build_player_features_v2.py#L90-L103)):

```python
total_team_games = len(unique_dates)
min_games_threshold = max(10, int(total_team_games * 0.3))

eligible = player_summary[
    (player_summary["total_games"] >= min_games_threshold) &
    (player_summary["avg_min"] >= 20)
]
...
expected = eligible.nlargest(8, "ppg")
```

### Team feature builder

Python: [training/features.py:109-460](../training/features.py#L109-L460) — `build_feature_matrix(games_df: pd.DataFrame) -> pd.DataFrame`. The function header doc lists all features in order ([training/features.py:1-33](../training/features.py#L1-L33)).

TypeScript inference twin: [src/services/feature-engine.ts:53-106](../src/services/feature-engine.ts#L53-L106) — `computeFeatures(env, modelVersion)` and [src/services/feature-engine.ts:129-309](../src/services/feature-engine.ts#L129-L309) — `buildFeatureVector(env, game, homeStats, awayStats, season)`.

Python builder fingerprint:

```python
def build_feature_matrix(games_df: pd.DataFrame) -> pd.DataFrame:
    """Compute model features from raw game/team stats.
    Mirrors the logic in src/services/feature-engine.ts exactly.
    ...
    """
    df = games_df.copy()
    features = pd.DataFrame(index=df.index)

    # 1. avg_pace = (home_pace + away_pace) / 2
    if "home_pace" in df.columns and "away_pace" in df.columns:
        features["avg_pace"] = (df["home_pace"] + df["away_pace"]) / 2
    else:
        features["avg_pace"] = np.nan
    ...
```

Each Python feature checks for column presence; missing columns produce all-NaN columns that get imputed in `impute_features()` ([training/features.py:485-492](../training/features.py#L485-L492)):

```python
def impute_features(X: pd.DataFrame) -> pd.DataFrame:
    """Fill missing feature values with column medians.
    Used before training/prediction to handle incomplete data.
    The Worker handles nulls by skipping them in the dot product,
    but for sklearn we need complete feature matrices.
    """
    return X.fillna(X.median())
```

### Time-cutoff logic for avoiding lookahead

The repo treats `commence_time <` (strict less-than) as the canonical anti-leakage gate. Examples:

- [src/services/feature-engine.ts:631-637](../src/services/feature-engine.ts#L631-L637) — total volatility (last 10 games):

```ts
env.DB.prepare(
  `SELECT total_score FROM games
   WHERE completed = TRUE AND season = ?
     AND (home_team = ? OR away_team = ?)
     AND commence_time < ?
   ORDER BY commence_time DESC LIMIT 10`
).bind(season, game.home_team, game.home_team, game.commence_time)
```

- [src/services/feature-engine.ts:642-647](../src/services/feature-engine.ts#L642-L647) — home team's recent home games:

```ts
`SELECT total_score FROM games
 WHERE completed = TRUE AND season = ? AND home_team = ?
   AND commence_time < ?
 ORDER BY commence_time DESC LIMIT 5`
```

- [src/services/feature-engine.ts:660-665](../src/services/feature-engine.ts#L660-L665) — H2H matchups this season:

```ts
`SELECT total_score FROM games
 WHERE completed = TRUE AND season = ?
   AND ((home_team = ? AND away_team = ?) OR (home_team = ? AND away_team = ?))
   AND commence_time < ?
 ORDER BY commence_time DESC LIMIT 4`
```

- [src/services/feature-engine.ts:393-409](../src/services/feature-engine.ts#L393-L409) — V2 player feature query (game_date < cutoff):

```ts
`WITH team_games AS (
   SELECT COUNT(DISTINCT game_date) as total_games
   FROM player_game_stats
   WHERE team_name = ? AND season = ? AND game_date < ?
 )
 SELECT
   pgs.player_id,
   AVG(pgs.pts) as ppg,
   ...
 FROM player_game_stats pgs
 WHERE pgs.team_name = ? AND pgs.season = ? AND pgs.game_date < ?
   AND pgs.minutes_decimal > 0
 GROUP BY pgs.player_id
 ORDER BY ppg DESC`
).bind(teamName, bdlSeason, gameDate, teamName, bdlSeason, gameDate)
```

- [src/services/clv-tracker.ts:217-236](../src/services/clv-tracker.ts#L217-L236) — closing line snapshot must be strictly before `commence_time`:

```ts
`SELECT os.bookmaker_key, os.total_line, os.over_price, os.under_price, b.tier
 FROM odds_snapshots os
 JOIN bookmakers b ON os.bookmaker_key = b.key
 JOIN games g ON os.game_id = g.id
 WHERE os.game_id = ?
   AND os.snapshot_time < g.commence_time
   AND os.snapshot_time = (
     SELECT MAX(o2.snapshot_time) FROM odds_snapshots o2
     WHERE o2.game_id = os.game_id
       AND o2.bookmaker_key = os.bookmaker_key
       AND o2.snapshot_time < g.commence_time
   )
 ORDER BY b.tier ASC`
```

The CLAUDE.md "Closing line snapshots must use `snapshot_time < commence_time`" rule (strict `<` not `<=`) is reflected here — using `<=` could include in-play odds posted exactly at tipoff.

### Imputation strategies

**Three different imputation maps coexist:**

1. **`TRAINING_MEDIANS`** (production, Worker) — [src/services/probability-model.ts:152-212](../src/services/probability-model.ts#L152-L212). Hand-curated median estimates per feature. Used at inference time ([src/services/probability-model.ts:411-415](../src/services/probability-model.ts#L411-L415)).
2. **`column_means`** (training) — [training/artifacts/lasso_v16_20260408_020552/metrics.json](../training/artifacts/lasso_v16_20260408_020552/metrics.json) `column_means` field. Used by training-time mean imputation ([training/train_v16_final.py:46-49](../training/train_v16_final.py#L46-L49)). Loaded by experiment scripts via `load_weights()` to faithfully replay training-time inference (e.g., [training/exp013_v16_production_replay.py:67](../training/exp013_v16_production_replay.py#L67)).
3. **`X.fillna(X.median())`** (Python eval) — [training/features.py:492](../training/features.py#L492). Per-dataset median, computed ad-hoc per script. Used by walk-forward when there is no saved artifact to reference.

The mismatch between #1 and #2 is **codex finding #6 — the model was trained on `column_means` but production imputes with `TRAINING_MEDIANS`**, which are different numbers. See Section 10.

### The "9 non-zero features" pattern

Production code does **not** filter at runtime — the inference loop iterates over all keys in `weights.coefficients` and multiplies. Lasso's zero coefficients produce zero contributions, and that's how "only 9 features matter."

[src/services/probability-model.ts:399-418](../src/services/probability-model.ts#L399-L418):

```ts
let total = weights.intercept;
let nullCount = 0;
let featureCount = 0;

const featureKeys = Object.keys(weights.coefficients) as (keyof typeof weights.coefficients)[];
for (const key of featureKeys) {
  featureCount++;
  const value = features[key];
  if (value !== null && value !== undefined) {
    total += value * weights.coefficients[key];
  } else {
    const median = TRAINING_MEDIANS[key];
    if (median !== undefined) {
      total += median * weights.coefficients[key];
    }
    nullCount++;
  }
}
```

The "9 non-zero" identity is documented in [training/artifacts/lasso_v16_20260408_020552/metrics.json:64](../training/artifacts/lasso_v16_20260408_020552/metrics.json#L64): `"non_zero_features": 9` (out of 50 in the `feature_count` field). The list of 9 lives only as an interpretation of the embedded weights — not as a runtime structure. CLAUDE.md enumerates them under "Non-zero coefficients (raw space)".

---

## Section 7: Dependency-tracking patterns

### Change-propagation chain (lineup change → prediction → recommendation)

Claw-edge does NOT have a declarative dependency graph. It has an **implicit cron-driven recalc chain** with explicit coupling:

```
12:00 PM ET cron (preGameAnalysis)
  → fetches latest player_availability (written by post-tip lineup-adjuster only — broken; see ISS-011)
  → computeFeatures()                                              // src/services/feature-engine.ts:53
    → for each game: query latest team_stats, compute V1.0-V1.6 features
      → features.home_missing_top_scorers reads player_availability  // src/services/feature-engine.ts:472-491
  → predict(env, features, modelVersion)                           // src/services/probability-model.ts:374
  → ensemblePrediction(linearTotal, xgbTotal, 0.9)                 // src/scheduled/pre-game-analysis.ts:96
  → calculateEdge(predicted, std, 0.10)                            // src/services/edge-calculator.ts:35
  → shopLines(gameId)                                              // src/services/line-shopper.ts:35
  → calibrate(rawProb, calibrationMap)                             // src/scheduled/pre-game-analysis.ts:264
  → kellyFraction(modelWinProb, decOdds, 0.25)                     // src/scheduled/pre-game-analysis.ts:267
  → INSERT INTO predictions (immutable thereafter, except outcome)
  → runShadowModels(env, computed, sharpLines, threshold)          // src/services/shadow-runner.ts:36
  → computePanelAgreement(env, gameIds)                            // src/services/shadow-runner.ts:193
    → UPDATE predictions SET confidence = MIN(1.0, confidence * boost)

20:00-03:00 hourly (confirmPredictions)
  → query unsettled predictions w/ commence_time in [+20m, +100m] window
  → fetchEspnInjuries() + fetchBdlInjuries() (NEW data, post-prediction)
  → countMissingFromInjuries(home, away, injuries, season)         // confirmation-pass.ts:247
  → if delta >= cancelThreshold (1 in April, 2 otherwise): UPDATE confirmation_status='cancelled'
  → else: UPDATE confirmation_status='confirmed'

23:30 UTC (lineupCheck)
  → getLineupAdjustments(env)                                      // src/services/lineup-adjuster.ts:49
    → BDL /lineups for actual starting lineups (post-tip)
    → findMissingImpact(home, away, lineup) — flags 15+ PPG players missing
    → INSERT INTO player_availability (status='out', source='bdl_lineup')
  → if highImpactMissing >= 2 for 'confirmed' predictions: UPDATE → 'cancelled'

06:00 UTC (postGameSettlement)
  → syncScoresOnly()
  → captureClosingLines()
  → settlePredictions() — fills actual_total, result, CLV, residual, z_score
  → playerStatsBackfill() — load missing player_game_stats
  → settlePlayerPredictions()
```

### Cron-driven recalc patterns

The system is **fully cron-driven, not event-driven**. The "dependency graph" is implicit in the cron schedule and the SQL `WHERE` clauses:

- `preGameAnalysis` skips games that already have a non-stale features row for the active model version ([src/services/feature-engine.ts:73-80](../src/services/feature-engine.ts#L73-L80)):

```ts
AND (
  g.id NOT IN (
    SELECT game_id FROM features WHERE model_version = ?
  )
  OR g.id IN (
    SELECT game_id FROM features
    WHERE model_version = ? AND computed_at < ?
  )
)
```

  where `staleThreshold = 12 hours ago`. So feature rows older than 12h get recomputed on the next cron tick. **There is no fine-grained invalidation** — recalculation is purely time-based.

- `force=true` in `POST /api/admin/trigger/analysis` ([src/index.ts:84-93](../src/index.ts#L84-L93)) explicitly deletes existing features and unsettled predictions, then runs analysis from scratch. This is the only "reset and recompute" lever.

- `confirmPredictions` only checks predictions in a tight window (`commence_time ∈ [+20m, +100m]`) — so each prediction is examined exactly once during the hour or two before tipoff ([src/scheduled/confirmation-pass.ts:79-82](../src/scheduled/confirmation-pass.ts#L79-L82)).

- `captureClosingLines` retroactively marks the latest pre-tip snapshot per bookmaker as the closing line, with a **30-day lookback window** ([src/services/clv-tracker.ts:281-283](../src/services/clv-tracker.ts#L281-L283)). Older games without closing lines are unrecoverable.

### Cache-invalidation patterns

There is **no cache-invalidation layer**. KV values either:
1. Have a TTL (`expirationTtl`) — see `lineup:adjustments:latest` (86400s), `confirmation:latest` (86400s), `walk-forward:v1.6-lasso-player` (24h), `sync:prop_settlement:last` (7×86400s).
2. Are overwritten by the next writer. There is no read-modify-write coordination.
3. Are persistent (no TTL) — `model:weights:*`, `calibration:map:current`, `shadow:models`. These are mutated only by deliberate admin actions (`wrangler kv key put` or `/api/admin/calibrate`).

The walk-forward dataset cache uses TTL-based invalidation only ([src/services/walk-forward-loader.ts:34-58](../src/services/walk-forward-loader.ts#L34-L58)):

```ts
const KV_TTL_SECONDS = 24 * 60 * 60;
const cached = await env.KV.get(KV_KEY);
if (cached) return JSON.parse(cached);

const obj = await env.R2.get(R2_KEY);
...
await env.KV.put(KV_KEY, text, { expirationTtl: KV_TTL_SECONDS });
```

If the R2 object is updated, the KV cache will serve the stale version for up to 24h.

### Live-updated vs computed-once

**Predictions are computed once and frozen** ([src/scheduled/pre-game-analysis.ts:273-288](../src/scheduled/pre-game-analysis.ts#L273-L288)):

```ts
const existing = await env.DB.prepare(
  "SELECT id, result FROM predictions WHERE game_id = ? AND model_version = ?"
).bind(gameId, modelVersion).first<{ id: number; result: string | null }>();

if (existing && existing.result !== null) {
  // Settled prediction — immutable, skip
  continue;
}

if (existing) {
  await env.DB.prepare(
    "DELETE FROM predictions WHERE id = ?"
  ).bind(existing.id).run();
}
```

A settled prediction is fully immutable. An unsettled prediction can be replaced if `pre-game-analysis` runs again before settlement (e.g. with `force=true`).

**Live updates** are limited to:
1. `confirmation_status` (preliminary → confirmed/cancelled) — [src/scheduled/confirmation-pass.ts:217-237](../src/scheduled/confirmation-pass.ts#L217-L237)
2. `confidence` after panel-agreement boost — [src/scheduled/pre-game-analysis.ts:414-418](../src/scheduled/pre-game-analysis.ts#L414-L418)
3. `unadjusted_total`, `lineup_adjustment`, `bottom_up_total`, `divergence`, `adjustment_source`, `adjusted_at` after the prediction-adjuster cron — [src/services/prediction-adjuster.ts](../src/services/prediction-adjuster.ts)
4. Outcome fields at settlement — `actual_total`, `result`, `closing_line`, `clv_*`, `residual`, `squared_residual`, `z_score`, `settled_at`

All other prediction columns are immutable.

---

## Section 8: Calibration and uncertainty

### Where is `residual_std` used?

| Site | Purpose |
|---|---|
| [src/services/probability-model.ts:69](../src/services/probability-model.ts#L69) | `LinearModelWeights.residualStd` field on the weights interface |
| [src/services/probability-model.ts:147](../src/services/probability-model.ts#L147) | Embedded fallback value `17.251` |
| [src/services/probability-model.ts:277-278](../src/services/probability-model.ts#L277-L278) | Snake_case → camelCase translation when loading from KV |
| [src/services/probability-model.ts:430](../src/services/probability-model.ts#L430) | Returned as `predictedStd` on every `ModelPrediction` |
| [src/services/edge-calculator.ts:118](../src/services/edge-calculator.ts#L118) | `probOverLine(predictedTotal, predictedStd, sharpLine)` — feeds the normal CDF for edge calc |
| [src/services/clv-tracker.ts:130-148](../src/services/clv-tracker.ts#L130-L148) | Probability-space CLV — uses `pred.predicted_std` to compute `probAtBetLine` and `probAtCloseLine` |
| [src/services/clv-tracker.ts:153](../src/services/clv-tracker.ts#L153) | `zScore = residual / pred.predicted_std` (logged at settlement) |
| [src/services/shadow-runner.ts:25-30](../src/services/shadow-runner.ts#L25-L30) / [:67-69](../src/services/shadow-runner.ts#L67-L69) / [:93](../src/services/shadow-runner.ts#L93) | Shadow weights' `residualStd` (with snake_case fallback), passed to `probOverLine` |
| [src/lib/normal-cdf.ts:31-32](../src/lib/normal-cdf.ts#L31-L32) | `probOverLine(mean, std, line)` — throws if `std <= 0` |
| [src/index.ts:151](../src/index.ts#L151) | PIT histogram query filters `predicted_std > 0` |
| [src/index.ts:200](../src/index.ts#L200) | PIT mean predicted std calculation |
| [training/artifacts/lasso_v16_20260408_020552/metrics.json:71](../training/artifacts/lasso_v16_20260408_020552/metrics.json#L71) | Holdout `residual_std` recorded as `17.251174677778263` |
| [training/exp011_walk_forward_real_lines.py:32](../training/exp011_walk_forward_real_lines.py#L32) | `RESIDUAL_STD = 17.278` (constant; note the discrepancy with the production 17.251) |
| [training/iss011_walkback.py:58](../training/iss011_walkback.py#L58) | `RESIDUAL_STD = 17.251` |
| [training/simulator/config.py:51](../training/simulator/config.py#L51) | `global_residual_std: float = 17.278` (sim default — matches walk-forward, NOT production) |

### Per-game σ (even if disabled)

**Implemented but not deployed.** The substrate exists:

- Migration-015 added `predictions.residual`, `predictions.squared_residual`, `predictions.z_score` ([schema/migration-015-squared-residual.sql](../schema/migration-015-squared-residual.sql)) — populated at settlement ([src/services/clv-tracker.ts:151-156](../src/services/clv-tracker.ts#L151-L156)).
- [training/variance_experiment_v2.py](../training/variance_experiment_v2.py) — Stage 2 σ model attempt (predicts `squared_residual` from features). The conclusion (per [docs/experiments/2026-04-08-variance-experiment-v2.md](experiments/2026-04-08-variance-experiment-v2.md)) was that σ is already well-calibrated globally, so the Stage 2 model had nowhere to improve.
- The PIT histogram endpoint ([src/index.ts:139-234](../src/index.ts#L139-L234)) confirms calibration ratio ≈ 0.98 — close enough to 1.0 that the project decided per-game σ wasn't worth deploying.

The Worker NEVER reads `predictions.squared_residual` or `predictions.z_score` for inference — they exist purely for the planned (and abandoned) variance model.

### BayesianRidge

**No examples found.** No `BayesianRidge`, `bayesian_ridge`, or `posterior_std` references in the codebase. The closest analog is the panel of 5 Ridge alphas, but those are point estimators, not posterior samples.

### The alpha panel (5 Ridge models)

**Yes, exists in code:**

- Trained: see panel JSON files at [training/artifacts/panel-ridge-a{1.0,1.5,2.0,2.5,3.0}.json](../training/artifacts/) (5 files). Each has `intercept`, `coefficients` over the V1.2-era 24-feature subset, and `residual_std`.
- Backtest: [training/panel_backtest.py:14-32](../training/panel_backtest.py#L14-L32). Loads all 5 weights, runs each on settled predictions, computes a "consensus side" and counts agreement.
- Production usage: [src/services/shadow-runner.ts:201-208](../src/services/shadow-runner.ts#L201-L208) queries shadow predictions with `model_version LIKE 'shadow:panel-ridge-%'`, then [src/services/shadow-runner.ts:211-243](../src/services/shadow-runner.ts#L211-L243) computes `agreementRatio = majorityCount / total`. The boost flows back into `confidence` at [src/scheduled/pre-game-analysis.ts:412-418](../src/scheduled/pre-game-analysis.ts#L412-L418):

```ts
const agreementBoost = 0.8 + (panel.agreementRatio * 0.4);

await env.DB.prepare(
  `UPDATE predictions
   SET confidence = MIN(1.0, confidence * ?)
   WHERE game_id = ? AND model_version LIKE ? AND result IS NULL`
).bind(agreementBoost, gameId, modelVersion + "%").run();
```

The 5-model panel is what backs the "84% win rate with high agreement" memory in CLAUDE.md, but it's only used as a confidence multiplier — it does NOT produce ensemble predictions.

### Uncertainty bounds reported alongside point predictions

| Where | What is reported |
|---|---|
| [src/services/probability-model.ts:429-434](../src/services/probability-model.ts#L429-L434) | `{ predictedTotal, predictedStd, confidence, modelVersion }` — every internal call |
| [predictions table](#predictions) | `predicted_total` + `predicted_std` + `confidence` + `model_over_prob` + `edge_probability` + `kelly_fraction` |
| [src/routes/predictions.ts](../src/routes/predictions.ts) | API response includes the same |
| `model_over_prob` ([src/services/edge-calculator.ts:118](../src/services/edge-calculator.ts#L118)) | This IS the distributional output — the integral of the predicted Gaussian above the line |

There is no upper/lower percentile reporting (e.g., 95% CI bounds). The uncertainty is reported as σ + a derived probability — never as `[low, high]` bounds.

---

## Section 9: The "model-backed cell" pattern, latent

### Site 1 — `predict()` in probability-model.ts

[src/services/probability-model.ts:374-435](../src/services/probability-model.ts#L374-L435):

```ts
export async function predict(
  env: Env,
  features: FeatureVector,
  modelVersion: string
): Promise<ModelPrediction> {
  const hasRollingData = features.avg_recent_total_5 !== null
    && features.avg_recent_total_10 !== null;

  if (!hasRollingData) {
    const countResult = await env.DB.prepare(
      "SELECT COUNT(*) as cnt FROM games WHERE completed = TRUE"
    ).first<{ cnt: number }>();
    const completedGames = countResult?.cnt ?? 0;
    if (completedGames < MIN_COMPLETED_GAMES_FOR_FULL_MODEL) {
      return coldStartPredict(env, features, modelVersion);
    }
  }

  const weights = await loadWeights(env, modelVersion);
  let total = weights.intercept;
  ...
  return {
    predictedTotal: Math.round(total * 10) / 10,
    predictedStd: weights.residualStd,
    confidence,
    modelVersion,
  };
}
```

**Caller pattern** ([src/scheduled/pre-game-analysis.ts:88](../src/scheduled/pre-game-analysis.ts#L88)):

```ts
const prediction = await predict(env, features, modelVersion);
```

### Site 2 — `predictXGB()` and `predictXGBBatch()`

[src/services/xgboost-client.ts:23-75](../src/services/xgboost-client.ts#L23-L75) — already quoted in Section 2. Returns `XGBPrediction | null` (point only).

**Caller pattern** ([src/scheduled/pre-game-analysis.ts:93-100](../src/scheduled/pre-game-analysis.ts#L93-L100)):

```ts
const xgbResult = await predictXGB(features, constants.trainingMedians);
const ensemble = ensemblePrediction(
  prediction.predictedTotal,
  xgbResult?.predicted_total ?? null,
  0.9
);
const finalPredictedTotal = ensemble.total;
```

### Site 3 — `simulate()` (the walk-forward replay primitive)

[training/simulator/core.py:65-133](../training/simulator/core.py#L65-L133) — already quoted. Takes (predictions, odds, actuals, config) and returns an `EvalResult`.

**Caller pattern** ([training/exp013_v16_production_replay.py:218](../training/exp013_v16_production_replay.py#L218)):

```python
result = core.simulate(predictions, odds, actuals, cfg, run_id=name)
```

### Site 4 — `shadowPredict()` (cell with bring-your-own-weights)

[src/services/shadow-runner.ts:154-164](../src/services/shadow-runner.ts#L154-L164) — already quoted.

### Site 5 — `calibrate()` (post-hoc cell)

[src/lib/calibrator.ts:29-52](../src/lib/calibrator.ts#L29-L52):

```ts
export function calibrate(
  rawProb: number,
  map: CalibrationMap | null
): number {
  if (!map || map.points.length < 2) return rawProb;
  const points = map.points;
  if (rawProb <= points[0]!.raw) return points[0]!.calibrated;
  if (rawProb >= points[points.length - 1]!.raw) return points[points.length - 1]!.calibrated;
  for (let i = 0; i < points.length - 1; i++) {
    const lo = points[i]!;
    const hi = points[i + 1]!;
    if (rawProb >= lo.raw && rawProb <= hi.raw) {
      const t = (rawProb - lo.raw) / (hi.raw - lo.raw);
      return lo.calibrated + t * (hi.calibrated - lo.calibrated);
    }
  }
  return rawProb;
}
```

### Ideal `ModelCell` abstraction

Drawing from the actual usage patterns above, the cell that V1.6 / XGBoost / panel-Ridge / cold-start / calibration *all want to be* is:

```ts
// Sketch — does NOT exist in the codebase yet.
interface ModelCell<TInput, TOutput> {
  // Identity & versioning
  readonly id: string;                          // e.g., "v1.6-lasso-player"
  readonly version: string;                     // monotonic; baked into outputs
  readonly featureContract: ReadonlyArray<string>; // canonical input keys

  // Inference (per single observation)
  predict(input: TInput): Promise<TOutput>;

  // Inference (vectorized; lets implementations batch over the wire — needed for XGBoost droplet)
  predictBatch(inputs: TInput[]): Promise<TOutput[]>;

  // Imputation contract
  imputeMissing(input: Partial<TInput>): TInput;

  // Calibration metadata for downstream Kelly / CLV / EV math
  readonly residualStd: number;
  readonly featureMedians: Readonly<Record<string, number>>;
  readonly featureMeans?: Readonly<Record<string, number>>;  // training-time means (audit only)

  // Diagnostics for the experiment loop
  describe(): {
    nonZeroFeatures: string[];                  // for L1/sparse models
    intercept: number;
    holdoutMetrics?: { mae: number; direction: number; rmse: number; residualStd: number };
    trainingSeasons?: string[];
  };

  // Optional: a closed-form distributional form.
  // If absent, callers rely on residualStd + Gaussian assumption.
  predictDistribution?(input: TInput): {
    mean: number;
    std: number;
    quantiles?: { p5: number; p50: number; p95: number };
  };
}
```

The Python side wants the same shape:

```python
class ModelCell(Protocol):
    id: str
    version: str
    feature_contract: list[str]
    residual_std: float
    feature_medians: dict[str, float]
    feature_means: dict[str, float] | None  # for audit reproducibility

    def predict(self, input: dict | pd.Series) -> ModelPrediction: ...
    def predict_batch(self, inputs: pd.DataFrame) -> pd.DataFrame: ...
    def impute_missing(self, input: dict) -> dict: ...
    def describe(self) -> dict: ...
    def predict_distribution(self, input) -> Distribution | None: ...  # optional
```

**Key requirements drawn from the codebase:**

1. **Hot-swap from a key-value store** — the V1.6 KV-loaded weights pattern must be a first-class lifecycle event. The cell needs an explicit `reload()` or be constructed lazily.
2. **Imputation is part of the contract.** Production has had three imputation strategies disagreeing for weeks (codex finding #6). The cell must own its imputation map and expose it.
3. **Feature contract drift detection.** Two times in the project's life, the FeatureVector grew, the Worker shipped, but the SQL INSERT lagged (ISS-005). The cell should validate input keys against `featureContract` at runtime.
4. **Cold-start fallback** is itself a cell — callers shouldn't branch on "is enough data available." The dispatcher should pick the best cell based on environmental conditions.
5. **Distributional output is optional.** XGBoost only returns point estimates; Lasso V1.6 returns mean+σ; the cold-start returns a fixed σ=18. Calling code must handle both.
6. **Ensembling is a cell composition primitive.** `ensemblePrediction(linear, xgb, 0.9)` is a special case of "linear combination of two cells" and should be expressible without writing a new function each time.
7. **Calibration is a post-cell.** The V1.6 calibration map at [src/lib/calibrator.ts](../src/lib/calibrator.ts) is itself a tiny cell (`raw_probability → calibrated_probability`). MarketingCubes V2 should treat it the same way as the model — a deterministic function with a versioned artifact.

---

## Section 10: Anti-patterns and gotchas

### ISS-009: V1.6 prediction inflation

[docs/issues/ISS-009-v16-prediction-inflation.md](issues/ISS-009-v16-prediction-inflation.md).

**Bug:** V1.6 predictions ran 267-364 (vs NBA reality of 200-260) for 11 of 13 games on 2026-04-10. Two compounding bugs:

1. `player_season_averages` had **exactly 4 duplicate rows per player** because the daily sync used `ON CONFLICT(player_id, season, as_of_date)` (allowing one row per date), and `as_of_date` advanced daily.
2. `computeBottomUpTotal()` ([src/services/prediction-adjuster.ts](../src/services/prediction-adjuster.ts)) ran `SELECT SUM(psa.pts_avg) ... FROM player_season_averages` with no date filter, so duplicates summed → ~400/team instead of ~100.
3. Bottom-up "game total" of 518-666 → adjuster applied a 30% nudge → predictions inflated by 60-130 points.

**Lesson:**
- **Materialized aggregations need uniqueness invariants enforced at the schema level**, not at the query level. Migration-017 dropped the 3-column unique index and replaced it with `(player_id, season)`.
- **Range-guard every derived value before it influences anything downstream.** The emergency fix added `[150, 320]` bounds and a `±15` nudge cap before the unique-constraint fix.
- **Defense-in-depth even after schema fix.** The ROW_NUMBER() OVER (PARTITION BY player_id ORDER BY as_of_date DESC) subquery pattern was added in [prediction-adjuster.ts](../src/services/prediction-adjuster.ts) and adjustment-backtest.ts as a second layer.

### ISS-005: Features table INSERT incomplete

[docs/issues/ISS-005-features-table-insert-incomplete.md](issues/ISS-005-features-table-insert-incomplete.md).

**Bug:** The `INSERT INTO features` in pre-game-analysis.ts wrote 24 of 50 feature columns (V1.0+V1.1+V1.2 only, missing V1.3-V1.6). Predictions were unaffected (the in-memory FeatureVector flowed through `predict()` correctly), but **the audit trail was broken** — backtests reading from the features table would median-impute the missing columns and produce different results than what was actually predicted.

**Lesson:**
- **CLAUDE.md "Recurring Bug Pattern #3" was written for this case** — INSERT column count must match VALUES placeholders must match `.bind()` arg count. The bug occurred *because* the type-level FeatureVector grew but the persistence layer drift wasn't caught.
- **Build-time invariant needed:** CI check that `features.sql` columns ⊇ FeatureVector keys.
- **Process invariant:** adding a feature must trigger a checklist that includes the table INSERT.

### Proxy-line inflation discovered in EXP-011

[docs/experiments/2026-04-29-walk-forward-real-lines.md](experiments/2026-04-29-walk-forward-real-lines.md).

**Bug:** The walk-forward backtest reported a 67% win rate, but graded against a `proxy_line = avg_recent_total_10` (rolling 10-game total average), not real sharp closing lines. When re-graded against real sharp closes (5,582 games of EXP-010 backfill), the win rate dropped to 57.5% (naive regrade) / 54.0% (full regrade with real-line-aware bet selection). The proxy systematically lagged real sharps by ~1.5 points.

**Lesson:**
- **Proxy lines used for backtest grading inflate apparent edge by a roughly fixed amount** because they share blind spots with the proxy side of the model. A rolling 10-game average doesn't react to lineup news, in-season trades, late-season tanking, or recent stylistic shifts — but the model partially captures these AND so does the real market. The model "beats" the proxy because both it and the market see signals the proxy doesn't.
- **The implication for bet sizing was the most important takeaway.** Quarter Kelly with the 67% number sized bets at ~8.5% of bankroll. Quarter Kelly with the calibrated 57.5% number recommends ~3.0%. The system was over-betting by ~2.8x.
- **Rule:** Any backtest that grades against anything other than real per-book closing lines must caveat the headline as "vs proxy."

### IS-vs-OOD contamination discovered in EXP-015

[docs/experiments/2026-04-30-xgboost-vs-lasso-ood.md](experiments/2026-04-30-xgboost-vs-lasso-ood.md).

**Bug:** EXP-013's 56.49% all-seasons headline was inflated by ~2.4pp because 3 of 5 evaluation seasons (2022-23, 2023-24, 2024-25) were *in the model's training set*. EXP-015 ran the same data with an OOD split (2020-21 + 2021-22 only) and got 54.10% — meaningfully closer to the EXP-011 number.

**The XGBoost case is starker.** All-seasons WR was 60.46%; OOD-only WR was 53.42%. XGBoost showed a 17-point IS-vs-OOD gap because boosted trees memorize training noise that doesn't generalize.

**Lesson:**
- **Every multi-season backtest must report an OOD breakout** — the all-seasons average systematically inflates because it averages over seasons the model trained on.
- **Per-season-status tagging is mandatory.** EXP-015 tags each season as "IS" or "OOD" in the per-season table.
- **`claw-audit` invariant proposed (ISS-015):** any backtest that doesn't separate IS from OOD seasons fails the audit.

### TRAINING_MEDIANS vs column_means mismatch (codex finding #6)

This is referenced in [training/exp013_v16_production_replay.py:113-116](../training/exp013_v16_production_replay.py#L113-L116):

```python
# Mean imputation (training-time strategy from train_v16_final.py:44).
# Production Worker uses TRAINING_MEDIANS instead — that mismatch is
# codex finding #6. We use means here to reproduce what the model
# was actually fit on; a "production-fidelity" variant would swap to
# the medians from probability-model.ts.
```

The conflicting code:

**Training side** ([training/train_v16_final.py:42-49](../training/train_v16_final.py#L42-L49)):

```python
X = X.replace([np.inf, -np.inf], np.nan)

# Column means for imputation and later for the Worker medians dict
column_means = {}
for col in X.columns:
    m = X[col].mean()
    column_means[col] = 0.0 if pd.isna(m) else float(m)
    X[col] = X[col].fillna(column_means[col])
```

**Inference side** ([src/services/probability-model.ts:152-160](../src/services/probability-model.ts#L152-L160), abridged):

```ts
const TRAINING_MEDIANS: Record<string, number> = {
  avg_pace: 101.25,
  pace_delta: 0.0,
  combined_off_rating: 224.5,
  combined_def_rating: 224.0,
  ...
};
```

Compare to the actual training means in [training/artifacts/lasso_v16_20260408_020552/metrics.json:184-238](../training/artifacts/lasso_v16_20260408_020552/metrics.json#L184-L238):

```json
"column_means": {
  "avg_pace": 101.29675712347354,
  "pace_delta": -0.00046132971506105525,
  "combined_off_rating": 224.17343283582088,
  "combined_def_rating": 224.13717774762551,
  ...
}
```

For most features the gap is small (< 1%), but for some it's meaningful (e.g., `combined_pfd: 38.0 median vs 38.21 mean`, `home_missing_top_scorers: 0.7 median vs 0.575 mean`, `combined_assists: 52.0 vs 52.31`). The cumulative effect is hard to quantify without a side-by-side run — but **production has been imputing with a different distribution than training assumed** for V1.6's entire production lifetime.

**Lesson:**
- **The training pipeline must export the imputation values it actually used, and the inference pipeline must consume them.** Hand-curated medians at the Worker level are guaranteed to drift.
- **Automate the conversion** — `train.py` should write a `TRAINING_MEDIANS` TS snippet straight into `probability-model.ts`, or the Worker should read `column_means` from a KV-stored JSON.

### CLAUDE.md edge-tier doctrine empirically wrong

CLAUDE.md "Edge threshold tiers" memory:

> 10-20% edge = sweet spot (88% win rate), 3-10% edge = noise (50% coin flip), >20% edge = data quality issue (0% win rate)

These numbers came from a few weeks of live data in early 2026. EXP-013 disproved them on 5 seasons / 5,582 games of historical data ([docs/experiments/2026-04-29-v16-production-replay.md:117-133](experiments/2026-04-29-v16-production-replay.md)):

| Edge bucket | n bets | Win % | EV/bet | Realized ROI |
|---|---|---|---|---|
| 0-3% | 90 | 55.1% | +2.64% | +7.69% |
| **3-10%** | **2,302** | **53.5%** | **+8.66%** | **+3.47%** |
| **10-20%** | **2,019** | **56.1%** | **+23.78%** | **+8.62%** |
| **>20%** | **465** | **60.2%** | **+44.17%** | **+16.59%** |

**Edge ranking is monotonically informative** — higher predicted edge → higher win rate AND higher realized ROI. The "3-10% = noise" claim is wrong (53.5% is not coin-flip). The ">20% = data quality issue" claim is wrong (60.2% wins, the highest tier).

**Lesson:**
- **Doctrine derived from small live samples is unreliable.** The original tier numbers were observational, not statistical. Always replicate against historical-scale data before codifying.
- **CLAUDE.md memories that contradict EXP-013's numbers need an update banner.** EXP-013's `## Follow-ups` section explicitly logs this as "Doctrine update needed." The doctrine has not yet been retired in CLAUDE.md.

### Fix landed but audit still flags as CRITICAL

[docs/issues/ISS-011-missing-top-scorers-always-zero.md](issues/ISS-011-missing-top-scorers-always-zero.md) — status `active` (not yet fixed at the time the inventory was assembled). Even though pre-game analysis now calls `syncPlayerAvailability()` ([src/scheduled/pre-game-analysis.ts:36-47](../src/scheduled/pre-game-analysis.ts#L36-L47)), the issue post-mortem hasn't been retroactively closed and is still flagged active.

**Lesson:**
- **Issue files are the source of truth, not the code.** A fix landing without closing the issue document leaves the audit graph saying "active CRITICAL" indefinitely.
- **The `claw-audit` skill should fail if any `active` `critical` issue's referenced patch commit is in `git log`** — i.e., if the fix already landed but the doc didn't update, that's its own bug.

### EXP-012 silent feature drop

Referenced in [training/exp013_v16_production_replay.py:122-128](../training/exp013_v16_production_replay.py#L122-L128):

```python
# All-NaN guard (defense against the EXP-012 silent-feature-drop bug)
all_nan_after = [c for c in feat_names if X[c].isna().all()]
if all_nan_after:
    raise RuntimeError(
        f"All-NaN columns after imputation: {all_nan_after}. "
        "Means dict missing entries or upstream feature pipeline broken."
    )
```

EXP-012 (the V1.7 residual-target Lasso experiment) silently produced a feature matrix where some columns were 100% NaN, which then got mean-imputed to 0 → those features contributed nothing to the linear sum, and the experiment "worked" but with a crippled model. The audit caught it and EXP-013 added the explicit guard.

**Lesson:**
- **Always check for all-NaN columns after imputation.** Silent feature dropping looks like a healthy run.
- **Guard rails belong in the inference primitive,** not in the calling experiment script.

---

## Section 11: What's NOT here that the new engine will need

### Multidimensional cube structure

claw-edge has a **flat per-game record** — every prediction, edge, feature is keyed on `(game_id, model_version)`. There is no concept of a "cell at (campaign × geography × week × metric)."

**Closest existing pattern:** the `predictions` table is essentially a 2-D matrix `(game_id × model_version)`, which lets multiple models coexist on the same game. To get to a real cube you'd need:
- Arbitrary-rank dimensions, not just two
- A coordinate system that's compositional (e.g., a hierarchical key)
- An efficient sparse storage layer (not 1 row per cell, but only-populated cells)

**What would need to change:** drop the per-table per-domain schema (predictions, features, wagers) in favor of a generic `(cube_id, dimension_key, value, metadata)` substrate. SQLite/D1 is fine as a backing store but the access layer needs to be coordinate-driven, not table-driven.

### Hierarchies / consolidations

claw-edge has a single hierarchy hardcoded in [src/services/edge-calculator.ts:60-65](../src/services/edge-calculator.ts#L60-L65) — sharp tier preferred, mid fallback, soft never:

```ts
const sharpOdds = odds.results.filter((o) => o.tier === "sharp");
const midOdds = odds.results.filter((o) => o.tier === "mid");
const consensusBooks = sharpOdds.length > 0 ? sharpOdds : midOdds;
```

That's the entirety of the consolidation primitive. There is no roll-up across teams, conferences, weeks, or any other dimension.

**Closest existing pattern:** the `bookmakers.tier` column. It's a one-level taxonomy.

**What would need to change:** a real cube engine needs (1) parent-child relationships that are queryable in SQL, (2) aggregation rules per dimension level (sum, avg, weighted-avg, max, ...), (3) automatic recalculation when a child changes. None of those exist.

### A rules language (DSL)

claw-edge has hardcoded computation in TypeScript/Python. The closest thing to a "rule" is the way Lasso coefficients are stored as a JSON keyed by feature name — but that's just data, not a rule (the inference algorithm is fixed).

**No examples found** of a rule expression layer (no `if-then` evaluator, no Excel-formula-like parser, no SQL-as-rules pattern). Even the simulator's `bet_selection` parameter is just a string-to-key lookup ([training/simulator/core.py:222-226](../training/simulator/core.py#L222-L226)):

```python
metric_key = {
    "ev": "ev",
    "edge_pp": "edge",
    "kelly_stake": "kelly_full",
}[config.bet_selection]
```

**Closest existing pattern:** `SimConfig` with hardcoded enum values for `bet_selection`, `vig_method`, `bet_sizing`. It's "configuration over code," not a rules language.

**What would need to change:** a real DSL (probably an expression-tree parser with a small set of primitives — sum, avg, percentile, lag, lookup, model_call) plus a runtime that evaluates expressions against the cube store. claw-edge has the *primitives* (oddsmath.py, normal-cdf.ts, kelly-criterion.ts) but no parser or evaluator.

### A dependency graph (declarative recalc)

claw-edge has **implicit cron-driven recalc** with no declarative dependencies. When `team_stats` updates, nothing happens until the 4pm cron fires `preGameAnalysis`. There is no DAG that says "feature `combined_off_rating` depends on `home_off_rating` and `away_off_rating`; if either changes, recompute."

**Closest existing pattern:** The 12hr `staleThreshold` in `computeFeatures()` ([src/services/feature-engine.ts:64](../src/services/feature-engine.ts#L64)) — a TTL is the simplest dependency-tracking primitive. The `WHERE g.id NOT IN (SELECT game_id FROM features WHERE model_version = ?)` clause is an "update-if-missing" idempotency guard, not a dependency edge.

**What would need to change:** a real DAG needs (1) explicit upstream/downstream edges per cell, (2) a topological sort for evaluation order, (3) lazy-or-eager recompute policy. claw-edge would need a complete rewrite of the feature pipeline as a graph, not a sequence of CRON-bound functions.

### Cross-cube references

There is **only one cube** in claw-edge — the per-game predictions space. Everything else (wagers, bettor profiles, system_log) is essentially a side log; nothing in the prediction layer reads from those.

**Closest existing pattern:** [src/services/should-i-bet.ts](../src/services/should-i-bet.ts) (referenced in CLAUDE.md but not opened during this audit) — combines model + market + bettor data, which is a cross-cube reference if you squint. But it's hand-coded against specific tables.

**What would need to change:** cubes need stable identifiers, a reference syntax (e.g., `OtherCube[dim1=x, dim2=y].metric`), and a query planner that can fetch cross-cube cells without manual JOINs.

### Versioning of the planning model state

Every prediction has `model_version` and `predicted_at`, which gives you a per-prediction snapshot. But there is **no notion of "the entire state of the planning model at time T"** — you can't roll back to "the cube as it was on April 28."

**Closest existing pattern:** the **EXP-010 odds backfill** captured 5 seasons of historical bookmaker prices, which serves as a frozen-state snapshot for backtests. But that's specific to one table; there's no general "cube as of timestamp" primitive.

**What would need to change:** every cell write needs a write timestamp + previous-value pointer (or a separate audit table). For point-in-time queries you'd need either snapshot isolation in the storage layer or explicit versioning on every cell.

### Multi-user concurrent editing

claw-edge has a single bettor profile (`bettor_edwin`) with a single non-self user (`bettor_brother`) seeded for historical wager import. There is **no notion of concurrent edits** — every write goes through a Worker route with bearer auth, and the auth is a single shared token.

**Closest existing pattern:** the rate-limiter at [src/middleware/rate-limit.ts](../src/middleware/rate-limit.ts) tracks per-IP request counts in KV but does nothing to coordinate writes.

**What would need to change:** real multi-user editing needs (1) authenticated identities (not shared bearer tokens), (2) optimistic-locking or operational-transform conflict resolution, (3) per-user permission scopes. claw-edge has none of those.

### WASM-deployable engine

claw-edge runs on Cloudflare Workers (V8 isolates). Python code stays on Edwin's Mac or the DigitalOcean droplet — it's never in the request path.

**Closest existing pattern:** all the math primitives in TypeScript ([src/lib/normal-cdf.ts](../src/lib/normal-cdf.ts), [src/lib/implied-probability.ts](../src/lib/implied-probability.ts), [src/lib/kelly-criterion.ts](../src/lib/kelly-criterion.ts), [src/lib/vig-removal.ts](../src/lib/vig-removal.ts)) compile to standard ES2022, which is portable to WASM in principle but requires a runtime (V8, Node, Deno).

**What would need to change:** a WASM-deployable engine needs (1) no DOM/Node-specific primitives, (2) explicit memory management or a WASM-friendly GC, (3) a packageable artifact (e.g., AssemblyScript or Rust). claw-edge's codebase is pure TS but tightly coupled to Cloudflare bindings (`env.DB`, `env.KV`, `env.R2`); those would need to be abstracted behind a storage interface.

---

## Open questions for Edwin

1. **Domain scope for V2:** the inventory shows claw-edge has 50 features with 9 active, single-target regression, and a fixed-σ Gaussian distributional output. MarketingCubes V2's ML cells — do they need (a) classification, (b) multi-target regression, (c) hierarchical/Bayesian models with proper posteriors, or (d) all of the above? The V1.6 architecture won't generalize beyond (b) without significant work.

2. **Imputation contract:** the codex finding #6 mismatch between TRAINING_MEDIANS and column_means is a real production bug. Do you want V2 cells to enforce "training-derived imputation values are the only legal imputation," or is configurable imputation (median / mean / nearest-neighbor / model-specific) a feature?

3. **Distributional vs point output:** XGBoost gives points; Lasso gives mean+σ; cold-start gives a fixed σ. Should every V2 cell be required to expose `predict_distribution()`, or is point-only acceptable for some cells?

4. **Cross-cube ML cells:** the latent ModelCell pattern in claw-edge takes a flat FeatureVector. In a multi-cube system, where does feature aggregation happen — inside the cell, or upstream in the rules layer? (claw-edge does it in TypeScript-level feature engineering; that won't scale to N markets.)

5. **Hot-swap semantics:** V1.6 weights load from KV with embedded fallback. For V2, do you want (a) atomic blue-green swaps with traffic shifting, (b) hot-reload from a registry, or (c) compile-in only with a rebuild required to update? Each has implications for the MarketingCubes deployment model.

6. **"Cube as of T" feature priority:** point-in-time queries (versioning of the planning model state) are flagged as missing. How important is this for V2 launch — load-bearing for the audit story, or nice-to-have post-launch?

7. **DSL syntax:** if MarketingCubes V2 has a rules language, does it look more like (a) Excel formulas (`=SUM(Q1:Q4)`), (b) MDX/SQL hybrid, (c) Python-embedded DSL (like PySpark), or (d) something custom? claw-edge has zero precedent here so you have a clean slate.

8. **Calibration / post-hoc layer:** V1.6 has a working isotonic calibration pipeline (`fitCalibration` in [src/lib/calibrator.ts](../src/lib/calibrator.ts), `exp016_fit_calibration.py`). Do you want this lifted into a generic "post-cell" pattern (any cell's output can be passed through any post-cell), or is it specific to win-probability calibration?

9. **OOD-vs-IS as a first-class concept:** EXP-015 demonstrated that all-seasons backtests are systematically inflated. Do you want V2's evaluation harness to *require* an explicit IS/OOD split per evaluation run, or is that a per-experiment opt-in?

10. **Simulator portability:** the Python simulator at [training/simulator/](../training/simulator/) is the canonical evaluator and has hand-graded tests (`test_core.py`, `test_oddsmath.py`, `test_books.py`). Should V2 ship with a TypeScript twin (so the Worker can run backtests on the edge) or stay Python-only? The TS implementations of the primitives already exist (Section 4) — what's missing is the per-game grading loop.

11. **Issue-tracking discipline:** the codebase has `docs/issues/ISS-XXX.md` with status fields, but ISS-011 was `active` after the fix shipped. Do you want V2 to enforce "merging a fix changes the status field" via CI, or is the current "discipline + audit skill" model the path?

12. **Doctrine retirement:** CLAUDE.md edge-tier doctrine is empirically wrong, EXP-013 documented it, but CLAUDE.md hasn't been updated. Do you want V2 to have a stricter "doctrine = experiment-backed only" policy from day one, or is the looser working-memory style fine and audit catches drift?

---

## Appendix A: Production V1.6 weights (verbatim, abridged)

[training/artifacts/lasso_v16_20260408_020552/weights.json](../training/artifacts/lasso_v16_20260408_020552/weights.json) — the entire artifact is 1,805 bytes. Top-level fields:

```json
{
  "intercept": -435.176757,
  "coefficients": {
    "avg_pace": 3.015916,
    "pace_delta": 0.0,
    "combined_off_rating": 0.548263,
    "combined_def_rating": 0.602389,
    "home_net_rating": 0.0,
    "away_net_rating": 0.0,
    "rest_disparity": 0.0,
    "home_b2b": -0.0,
    "away_b2b": 0.0,
    "avg_recent_total_5": 0.064143,
    "avg_recent_total_10": 0.331426,
    "combined_ts_pct": 0.0,
    "combined_efg_pct": 0.0,
    "home_win_pct": 0.0,
    "away_win_pct": -0.0,
    "home_home_win_pct": 0.0,
    "away_road_win_pct": -0.0,
    "combined_pts_avg": 0.0,
    "combined_turnovers": -0.0,
    "combined_fg3_rate": 0.0,
    "combined_ft_rate": 0.0,
    "pts_diff": -0.0,
    "combined_assists": 0.0,
    "scoring_trend": 0.0,
    "home_off_vs_away_def": 0.0,
    "away_off_vs_home_def": -0.0,
    "total_volatility": -0.0,
    "h2h_last_total": -0.0,
    "h2h_avg_total": -0.0,
    "h2h_games_played": 0.0,
    "home_recent_total_avg": 0.046528,
    "away_recent_total_avg": 0.0,
    "margin_of_victory": -0.0,
    "combined_oreb_pct": -0.0,
    "combined_stl": -0.017643,
    "combined_pfd": 0.0,
    "combined_tm_tov_pct": -0.0,
    "combined_ft_pct": 0.0,
    "home_home_win_pct_real": 0.0,
    "away_road_win_pct_real": -0.0,
    "home_expected_starter_pts": 0.0,
    "home_missing_starter_pts": -0.0,
    "home_expected_starter_count": 0.0,
    "home_top_scorer_recent_played_pct": -0.0,
    "away_expected_starter_pts": 0.0,
    "away_missing_starter_pts": -0.0,
    "away_expected_starter_count": 0.0,
    "away_top_scorer_recent_played_pct": -0.0,
    ...
    "home_missing_top_scorers": -1.202871,
    "away_missing_top_scorers": -0.054086
  },
  "residual_std": 17.251
}
```

The 9 non-zero coefficients:

| Feature | Coefficient (raw space) | Interpretation |
|---|---|---|
| `avg_pace` | +3.015916 | strongest single signal — every extra possession contributes ~6 pts to the total |
| `combined_def_rating` | +0.602389 | combined opponent points-allowed-per-100 |
| `combined_off_rating` | +0.548263 | combined points-scored-per-100 |
| `avg_recent_total_10` | +0.331426 | rolling 10-game total average |
| `avg_recent_total_5` | +0.064143 | rolling 5-game total average |
| `home_recent_total_avg` | +0.046528 | home team's recent home-games average |
| `combined_stl` | -0.017643 | small negative (more steals = pace + transition, but Lasso disagrees) |
| `home_missing_top_scorers` | -1.202871 | every top-3 PPG scorer OUT for the home team drops the predicted total by 1.2 pts |
| `away_missing_top_scorers` | -0.054086 | weak away analog |

The `metrics.json` artifact at the same path includes:

```json
{
  "model": "lasso_v1.6",
  "algorithm": "Lasso (L1) with StandardScaler",
  "alpha": 0.7,
  "feature_count": 50,
  "non_zero_features": 9,
  "train_samples": 2948,
  "holdout_samples": 737,
  "holdout_metrics": {
    "mae": 13.78321341514041,
    "rmse": 17.305655054819688,
    "direction": 0.6635006784260515,
    "residual_std": 17.251174677778263
  },
  "train_metrics": {
    "mae": 14.25223355721189,
    "direction": 0.6485753052917232,
    "residual_std": 18.025504554053747
  },
  "trained_at": "2026-04-08T02:05:52.869021"
}
```

The MAE gap (train 14.25 vs holdout 13.78) is unusual — typically holdout > train. Two interpretations: (a) the chronological 80/20 split happens to put easier-to-predict games at the end of the period, or (b) the holdout is a different distribution (specifically, the V1.6 player features are densest in the most recent seasons because the BDL backfill is most complete there). EXP-015 strongly suggests (b) is correct given the IS-vs-OOD contamination findings.

---

## Appendix B: Verbatim normal-CDF and Kelly primitives (TypeScript)

These four files are the math foundation that every model and every backtest in the codebase depends on. Reproduced in full for transfer.

### [src/lib/normal-cdf.ts](../src/lib/normal-cdf.ts)

```ts
// Standard normal CDF using Abramowitz & Stegun formula 26.2.17.
// Accurate to ~5e-6, no external dependencies.
// Reference: Handbook of Mathematical Functions (1964), formula 26.2.17.

const b1 = 0.319381530;
const b2 = -0.356563782;
const b3 = 1.781477937;
const b4 = -1.821255978;
const b5 = 1.330274429;
const pp = 0.2316419;
const INV_SQRT_2PI = 1 / Math.sqrt(2 * Math.PI);

// P(Z <= x) for standard normal distribution.
export function normalCdf(x: number): number {
  if (x >= 0) {
    const t = 1 / (1 + pp * x);
    const pdf = INV_SQRT_2PI * Math.exp(-x * x / 2);
    return 1 - pdf * (b1 * t + b2 * t * t + b3 * t * t * t + b4 * t * t * t * t + b5 * t * t * t * t * t);
  }
  return 1 - normalCdf(-x);
}

// P(total > line) given a predicted mean and std.
// This is the core formula for distributional edge calculation.
// Throws if std <= 0 — callers must ensure a valid residual std is loaded.
export function probOverLine(
  mean: number,
  std: number,
  line: number
): number {
  if (std <= 0) {
    throw new Error(`probOverLine: std must be positive, got ${std}`);
  }
  const z = (line - mean) / std;
  return 1 - normalCdf(z);
}
```

### [src/lib/implied-probability.ts](../src/lib/implied-probability.ts)

```ts
export function americanToImpliedProbability(odds: number): number {
  if (odds === 0) {
    throw new Error("American odds cannot be zero");
  }
  if (odds < 0) {
    const absOdds = Math.abs(odds);
    return absOdds / (absOdds + 100);
  }
  return 100 / (odds + 100);
}

export function americanToDecimal(odds: number): number {
  if (odds === 0) {
    throw new Error("American odds cannot be zero");
  }
  if (odds < 0) {
    return 1 + 100 / Math.abs(odds);
  }
  return 1 + odds / 100;
}

export function decimalToAmerican(decimal: number): number {
  if (decimal <= 1) {
    throw new Error("Decimal odds must be greater than 1");
  }
  if (decimal >= 2) {
    return Math.round((decimal - 1) * 100);
  }
  return Math.round(-100 / (decimal - 1));
}

export function impliedToAmerican(prob: number): number {
  if (prob <= 0 || prob >= 1) {
    throw new Error("Implied probability must be between 0 and 1 (exclusive)");
  }
  if (prob > 0.5) {
    return Math.round((-prob * 100) / (1 - prob));
  }
  return Math.round((100 * (1 - prob)) / prob);
}
```

### [src/lib/vig-removal.ts](../src/lib/vig-removal.ts)

```ts
export function calculateVig(
  overImplied: number,
  underImplied: number
): number {
  return overImplied + underImplied - 1;
}

export function removeVig(
  overImplied: number,
  underImplied: number
): { overFair: number; underFair: number; vig: number } {
  const total = overImplied + underImplied;
  if (total <= 0) {
    throw new Error("Sum of implied probabilities must be positive");
  }
  const vig = total - 1;
  return {
    overFair: overImplied / total,
    underFair: underImplied / total,
    vig,
  };
}

export function removeVigPower(
  overImplied: number,
  underImplied: number
): { overFair: number; underFair: number; vig: number } {
  if (overImplied <= 0 || underImplied <= 0) {
    throw new Error("Implied probabilities must be positive");
  }

  const vig = overImplied + underImplied - 1;

  if (vig <= 0 || Math.abs(vig) < 1e-10) {
    return { overFair: overImplied, underFair: underImplied, vig: 0 };
  }

  let lo = 0;
  let hi = 100;
  const maxIterations = 200;
  const tolerance = 1e-12;

  for (let i = 0; i < maxIterations; i++) {
    const mid = (lo + hi) / 2;
    const sum = Math.pow(overImplied, mid) + Math.pow(underImplied, mid);

    if (Math.abs(sum - 1) < tolerance) {
      lo = mid;
      break;
    }

    if (sum > 1) {
      lo = mid;
    } else {
      hi = mid;
    }
  }

  const n = lo;
  const overFair = Math.pow(overImplied, n);
  const underFair = Math.pow(underImplied, n);

  const fairTotal = overFair + underFair;

  return {
    overFair: overFair / fairTotal,
    underFair: underFair / fairTotal,
    vig,
  };
}
```

### [src/lib/kelly-criterion.ts](../src/lib/kelly-criterion.ts)

```ts
export function kellyFraction(
  winProbability: number,
  decimalOdds: number,
  fraction: number = 0.25
): number {
  if (!Number.isFinite(winProbability) || winProbability <= 0 || winProbability >= 1) {
    return 0;
  }
  if (!Number.isFinite(decimalOdds)) {
    return 0;
  }
  if (decimalOdds <= 1) {
    return 0;
  }
  if (fraction <= 0) {
    return 0;
  }

  const b = decimalOdds - 1;
  const q = 1 - winProbability;
  const fullKelly = (winProbability * b - q) / b;

  if (fullKelly <= 0) {
    return 0;
  }

  return fullKelly * fraction;
}

export function suggestedUnits(
  kellyFrac: number,
  bankroll: number,
  unitSize: number
): number {
  if (kellyFrac <= 0 || bankroll <= 0 || unitSize <= 0) {
    return 0;
  }

  const wagerAmount = kellyFrac * bankroll;
  const units = wagerAmount / unitSize;

  return Math.round(units * 100) / 100;
}
```

These primitives are duplicated in Python at [training/simulator/oddsmath.py](../training/simulator/oddsmath.py). The file's docstring explicitly cross-references this:

```python
"""Math primitives for the EV simulator.
...
References:
    - Vig removal:        src/lib/vig-removal.ts (TypeScript twin)
    - American↔implied:   src/lib/implied-probability.ts
    - Kelly criterion:    src/lib/kelly-criterion.ts (TypeScript twin)
"""
```

If MarketingCubes V2 ships in WASM, these are the kernels that have to compile. None of them depend on Cloudflare bindings, the DOM, or Node — they're pure math. The hardest part of the WASM conversion is the rest of the codebase, not these.

---

## Appendix C: Calibration map fitting (full algorithm)

[src/lib/calibrator.ts](../src/lib/calibrator.ts) — the full implementation, since the calibration story is one of the strongest patterns to lift.

### `fitCalibration` — pool-adjacent-violators isotonic regression

```ts
export function fitCalibration(
  predictions: Array<{ rawProb: number; won: boolean }>,
  numBuckets: number = 8
): CalibrationMap | null {
  if (predictions.length < 20) return null;

  // Sort by raw probability
  const sorted = [...predictions].sort((a, b) => a.rawProb - b.rawProb);

  // Split into equal-sized buckets
  const bucketSize = Math.ceil(sorted.length / numBuckets);
  const rawPoints: Array<{ raw: number; calibrated: number; count: number }> = [];

  for (let i = 0; i < sorted.length; i += bucketSize) {
    const bucket = sorted.slice(i, Math.min(i + bucketSize, sorted.length));
    if (bucket.length === 0) continue;

    const avgRaw = bucket.reduce((s, p) => s + p.rawProb, 0) / bucket.length;
    const winRate = bucket.filter((p) => p.won).length / bucket.length;

    rawPoints.push({ raw: avgRaw, calibrated: winRate, count: bucket.length });
  }

  // Pool-adjacent-violators: enforce monotonicity
  const points = poolAdjacentViolators(rawPoints);

  // Compute Brier scores (before and after calibration)
  let rawBrier = 0;
  let calBrier = 0;
  for (const pred of predictions) {
    const outcome = pred.won ? 1 : 0;
    rawBrier += (pred.rawProb - outcome) ** 2;
    const calProb = calibrateWithPoints(pred.rawProb, points);
    calBrier += (calProb - outcome) ** 2;
  }
  rawBrier /= predictions.length;
  calBrier /= predictions.length;

  return {
    points: points.map((p) => ({ raw: p.raw, calibrated: p.calibrated })),
    fittedAt: new Date().toISOString(),
    sampleSize: predictions.length,
    rawBrier: Math.round(rawBrier * 10000) / 10000,
    calibratedBrier: Math.round(calBrier * 10000) / 10000,
  };
}
```

### `poolAdjacentViolators` (PAVA)

```ts
function poolAdjacentViolators(
  points: Array<{ raw: number; calibrated: number; count: number }>
): Array<{ raw: number; calibrated: number }> {
  if (points.length <= 1) return points;

  const blocks: Array<{ raw: number; calibrated: number; count: number }> =
    points.map((p) => ({ ...p }));

  let changed = true;
  while (changed) {
    changed = false;
    for (let i = 0; i < blocks.length - 1; i++) {
      if (blocks[i]!.calibrated > blocks[i + 1]!.calibrated) {
        const a = blocks[i]!;
        const b = blocks[i + 1]!;
        const totalCount = a.count + b.count;
        const pooled = {
          raw: (a.raw * a.count + b.raw * b.count) / totalCount,
          calibrated: (a.calibrated * a.count + b.calibrated * b.count) / totalCount,
          count: totalCount,
        };
        blocks.splice(i, 2, pooled);
        changed = true;
        break;
      }
    }
  }

  return blocks.map((b) => ({ raw: b.raw, calibrated: b.calibrated }));
}
```

### Python twin

[training/exp016_fit_calibration.py](../training/exp016_fit_calibration.py) re-implements both functions verbatim (already quoted in Section 8) so EXP-016 can fit calibration maps offline using the exact same algorithm and produce a JSON file ready to upload to KV via:

```bash
npx wrangler kv key put --binding=KV calibration:map:current \
    --remote --path=training/artifacts/calibration_map_v1.6_ood.json
```

This is the cleanest "trained artifact → production cell" loop in the entire codebase. MarketingCubes V2's training pipeline should mirror it: train offline, produce a JSON artifact whose schema is mirrored exactly by the production cell, deploy by `wrangler kv key put`.

---

## Appendix D: Pre-game analysis cron — full sequence

The `preGameAnalysis()` function at [src/scheduled/pre-game-analysis.ts:24-452](../src/scheduled/pre-game-analysis.ts#L24-L452) is the most complex orchestration in the Worker. Its sequence is the model for every "predict + persist + notify" pipeline:

1. **Step 0** ([:36-47](../src/scheduled/pre-game-analysis.ts#L36-L47)) — `syncPlayerAvailability()` to fix ISS-011 (write current OUT/QUESTIONABLE injuries to the table that feeds the V1.6 `missing_top_scorers` feature).

2. **Step 1** ([:50-64](../src/scheduled/pre-game-analysis.ts#L50-L64)) — `computeFeatures(env, modelVersion)` — computes today's pending games' feature vectors (with the 12hr staleness threshold so retries don't double-write).

3. **Step Calibration** ([:66-80](../src/scheduled/pre-game-analysis.ts#L66-L80)) — load `calibration:map:current` from KV. If absent, raw probabilities flow through (calibration is no-op).

4. **Step 2** ([:88-89](../src/scheduled/pre-game-analysis.ts#L88-L89)) — `predict(env, features, modelVersion)` for each game. The Lasso V1.6 cell.

5. **Step 2b** ([:92-108](../src/scheduled/pre-game-analysis.ts#L92-L108)) — `predictXGB()` + `ensemblePrediction(linear, xgb, 0.9)`. The XGBoost cell + ensemble post-cell.

6. **Step 3** ([:111-117](../src/scheduled/pre-game-analysis.ts#L111-L117)) — `calculateEdge(env, gameId, finalPredictedTotal, predictedStd, threshold)`. Edge calculation queries the latest odds_snapshots, vig-removes, computes `model_p_over - sharp_over_fair`.

7. **Step 4** ([:120](../src/scheduled/pre-game-analysis.ts#L120)) — `shopLines(env, gameId)`. Best-price-per-side line shop.

8. **Step 5** ([:130-230](../src/scheduled/pre-game-analysis.ts#L130-L230)) — `INSERT OR REPLACE INTO features` with all 50 features + edge metadata.

9. **Step 6** ([:235-340](../src/scheduled/pre-game-analysis.ts#L235-L340)) — `INSERT INTO predictions`:
   - Determines `recommendedSide` based on edge direction + threshold
   - Looks up `recommendedBook` and `bestPrice` from line-shop result
   - Calls `calibrate(rawWinProb, calibrationMap)` to get the calibrated win prob
   - Calls `kellyFraction(modelWinProb, decOdds, 0.25)` for sizing
   - Calls `suggestedUnits(quarterKellyFrac, 1000, 10)` — note hardcoded $1,000 bankroll, $10 unit
   - Stores `missing_scorers_at_predict` (sum of home + away missing top scorers) for the confirmation gate's later delta check

10. **Step Shadow** ([:376-440](../src/scheduled/pre-game-analysis.ts#L376-L440)) — runs every shadow model on the same features, computes panel agreement, multiplies production prediction's confidence by `(0.8 + 0.4 × agreementRatio)`, stores summary in `panel:agreement:latest` KV (TTL 1 day).

11. **Step Persist metadata** ([:442-451](../src/scheduled/pre-game-analysis.ts#L442-L451)) — writes `sync:analysis:last` to KV.

The pattern that's load-bearing for V2: **step 6 is idempotent and protective** — it queries for an existing prediction first, and only deletes-and-replaces if the prior prediction is unsettled. Settled predictions are never overwritten. This is the core "predictions are immutable except for outcome fields" rule made operational.

---

## Appendix E: Settlement and CLV — full sequence

[src/scheduled/post-game-settlement.ts](../src/scheduled/post-game-settlement.ts) has its own canonical flow:

1. **Step 0** ([:23-34](../src/scheduled/post-game-settlement.ts#L23-L34)) — `syncScoresOnly()` (do NOT run the full `syncStats()` because it triggers 5+ BDL calls and 429s when crons overlap).

2. **Step 1** ([:38-47](../src/scheduled/post-game-settlement.ts#L38-L47)) — `captureClosingLines()` retroactively marks pre-tip snapshots within a 30-day window.

3. **Step 2** ([:50-80](../src/scheduled/post-game-settlement.ts#L50-L80)) — `settlePredictions()` — fills `actual_total`, `result`, `closing_line`, `closing_over_implied`, `clv_points`, `clv_positive`, `clv_probability`, `residual`, `squared_residual`, `z_score`, `settled_at`.

4. **Step 2b** ([:90-111](../src/scheduled/post-game-settlement.ts#L90-L111)) — **ORDERING IS LOAD-BEARING:** `playerStatsBackfill()` runs BEFORE prop settlement. Codex audit 2026-04-29 caught that prop settlement was grading missing-stats games as `actual_value=0 / result=LOSS` because it couldn't distinguish "DNP" from "data outage." Backfilling stats first prevents this.

5. **Step 2c** ([:114-138](../src/scheduled/post-game-settlement.ts#L114-L138)) — `settlePlayerPredictions()`. Skips rows where the `player_game_stats` row is still missing → those stay `result IS NULL` and get graded on the next run.

6. **Step 3** ([:141-148](../src/scheduled/post-game-settlement.ts#L141-L148)) — `cleanupOldLogs(env, 7)` — deletes log entries older than 7 days.

The `settleSinglePrediction()` function ([src/services/clv-tracker.ts:64-189](../src/services/clv-tracker.ts#L64-L189)) is the per-prediction grader. Two CLV computations live there:

**Points-space CLV** ([src/services/clv-tracker.ts:108-120](../src/services/clv-tracker.ts#L108-L120)):

```ts
if (closingLine !== null && pred.recommended_side !== null) {
  const lineMovement = closingLine - pred.sharp_total_line;

  if (pred.recommended_side === "OVER") {
    clvPoints = Math.round(lineMovement * 10) / 10;
    clvPositive = lineMovement > 0;
  } else if (pred.recommended_side === "UNDER") {
    clvPoints = Math.round(-lineMovement * 10) / 10;
    clvPositive = lineMovement < 0;
  }
}
```

**Probability-space CLV** ([src/services/clv-tracker.ts:128-148](../src/services/clv-tracker.ts#L128-L148)):

```ts
if (
  closingLine !== null &&
  pred.recommended_side !== null &&
  pred.predicted_std !== null &&
  pred.predicted_std > 0
) {
  const probAtBetLine = probOverLine(pred.predicted_total, pred.predicted_std, pred.sharp_total_line);
  const probAtCloseLine = probOverLine(pred.predicted_total, pred.predicted_std, closingLine);
  if (pred.recommended_side === "OVER") {
    clvProbability = Math.round((probAtBetLine - probAtCloseLine) * 10000) / 10000;
  } else if (pred.recommended_side === "UNDER") {
    clvProbability = Math.round(((1 - probAtBetLine) - (1 - probAtCloseLine)) * 10000) / 10000;
  }
}
```

The probability-space version uses the model's predicted distribution to translate point movement into probability-space CLV — a 2-point move at line 180 (high variance) is a smaller probability shift than 2 points at line 240 (lower variance). This is one of the strongest examples of "use the cell's distributional output downstream" in the codebase.

---

## Appendix F: Bookmaker tier table (production seed + simulator alignment)

[schema/schema.sql:20-33](../schema/schema.sql#L20-L33) seeds 13 books. Migration-001 adds 2 more (`hardrock`, `fliff` — both soft).

| key | tier | source |
|---|---|---|
| pinnacle | sharp | schema.sql seed |
| circa | sharp | schema.sql seed |
| bookmaker | sharp | schema.sql seed |
| fanduel | mid | schema.sql seed |
| betrivers | mid | schema.sql seed |
| williamhill_us | mid | schema.sql seed |
| unibet | mid | schema.sql seed |
| draftkings | soft | schema.sql seed |
| betmgm | soft | schema.sql seed |
| caesars | soft | schema.sql seed |
| pointsbetus | soft | schema.sql seed |
| bovada | soft | schema.sql seed |
| barstool | soft | schema.sql seed |
| hardrock | soft | migration-001 |
| fliff | soft | migration-001 |

The Python simulator's `books.py` adds `superbook` (mid) and 11 additional soft books observed only in the historical odds backfill (`betonlineag`, `betus`, `lowvig`, `mybookieag`, `fanatics`, `foxbet`, `gtbets`, `intertops`, `twinspires`, `wynnbet`). Three aliases are defined: `circasports → circa`, `unibet_us → unibet`, `sugarhouse → betrivers`.

The simulator's tier classification is therefore **strictly larger** than production's. If a new book appears in odds data, the production schema silently treats it as soft (default), but the simulator's `books.py:CoverageTests.test_all_backfill_books_classified` ([training/simulator/test_books.py:147-176](../training/simulator/test_books.py#L147-L176)) fails CI until it's explicitly classified — preventing silent demotion of an actually-sharp book.

---

## Appendix G: Sync metadata KV keys (full table)

Every cron writes a `sync:*:last` JSON to KV so dashboards and the `/api/health` endpoint can show "last successful sync time" per pipeline. These are the integration points for monitoring:

| KV key | Writer | Schema (JSON shape) |
|---|---|---|
| `sync:odds:last` | `dailyOddsSync` ([src/services/odds-ingestion.ts:207](../src/services/odds-ingestion.ts#L207)) | `{ timestamp, snapshotsWritten, gamesProcessed, errors }` |
| `sync:stats:last` | `dailyStatsSync` ([src/services/stats-ingestion.ts:457](../src/services/stats-ingestion.ts#L457), [:701](../src/services/stats-ingestion.ts#L701)) | `{ timestamp, gamesUpdated, teamStatsRefreshed, errors }` |
| `sync:settlement:last` | `postGameSettlement` ([src/scheduled/post-game-settlement.ts:67](../src/scheduled/post-game-settlement.ts#L67)) | `{ timestamp, closingsCaptured, predictionsSettled, errors }` |
| `sync:prop_settlement:last` | `postGameSettlement` ([src/scheduled/post-game-settlement.ts:125](../src/scheduled/post-game-settlement.ts#L125)) (TTL 7 days) | `{ timestamp, pending, settled, noActualStat, errors }` |
| `sync:analysis:last` | `preGameAnalysis` ([src/scheduled/pre-game-analysis.ts:442](../src/scheduled/pre-game-analysis.ts#L442)) | `{ timestamp, modelVersion, gamesAnalyzed, predictionsCreated, errors }` |
| `sync:player_props:last` | `playerPropAnalysis` (similar shape) | per-cron payload |
| `sync:player_stats_backfill:last` | `playerStatsBackfill` ([src/services/player-stats-backfill.ts:131](../src/services/player-stats-backfill.ts#L131)) | per-cron payload |
| `sync:player_season:last` | `refreshSeasonAverages` ([src/services/player-stats-ingestion.ts:261](../src/services/player-stats-ingestion.ts#L261)) | per-cron payload |
| `sync:player_prop_odds:last` | `syncPlayerPropOdds` ([src/services/player-prop-ingestion.ts:179](../src/services/player-prop-ingestion.ts#L179)) | per-cron payload |
| `sync:injuries:last` | `syncPlayerAvailability` ([src/services/injury-sync.ts:313](../src/services/injury-sync.ts#L313)) | per-cron payload |

The `/api/health` endpoint at [src/routes/health.ts](../src/routes/health.ts) reads `sync:odds:last` and `sync:stats:last` and exposes them in the `sync` field of its response — a lightweight watchdog without a dedicated monitoring system.

This pattern is generalizable for V2: every recalc job writes a "last successful run" envelope to a stable KV key. The dashboard becomes a function of the union of those keys.

---

## Appendix H: Confirmation gate full timing diagram

The confirmation pass is the most subtle cron because it has to fire often enough to catch every game's pre-tip window without double-checking. The reasoning is preserved in code comments:

[src/scheduled/confirmation-pass.ts:58-94](../src/scheduled/confirmation-pass.ts#L58-L94):

```ts
// DYNAMIC TIMING: Check games that are 20-100 minutes from tipoff.
// The cron runs hourly; each game gets checked exactly once in this window.
// Window rationale:
//   - 20 min floor: close enough that late scratches are mostly known,
//     and far enough that the user still has time to not place the bet
//   - 100 min ceiling: slightly wider than hourly cron spacing to guarantee
//     every game is caught by exactly one fire, no matter when the cron runs
//   - status filter: only preliminary/unconfirmed games (already-confirmed
//     games are skipped so we don't double-check)
//
// Example: 6:10pm ET game. 21:00 UTC (5:00pm ET) fire catches it at 70 min
// before tipoff. 22:00 UTC fire sees it at 10 min (below floor, skipped).
// Example: 10:40pm ET game. 01:00 UTC fire catches it at 100 min before.
const predictions = await env.DB.prepare(
  `SELECT p.id, p.game_id, p.predicted_total, p.recommended_side,
          p.missing_scorers_at_predict, p.confirmation_status,
          g.home_team, g.away_team, g.bdl_game_id, g.commence_time
   FROM predictions p
   JOIN games g ON g.id = p.game_id
   WHERE p.result IS NULL
     AND p.recommended_side IS NOT NULL
     AND (p.confirmation_status = 'preliminary' OR p.confirmation_status = 'unconfirmed')
     AND unixepoch(g.commence_time) >= unixepoch('now', '+20 minutes')
     AND unixepoch(g.commence_time) <= unixepoch('now', '+100 minutes')
   ORDER BY g.commence_time ASC`
).all<...>();
```

The two cron declarations covering this — `0 20-23 * * *` and `0 0-3 * * *` (8 firings total, 4pm-11pm ET) — are necessary because Cloudflare Workers crons don't support range-and-modulo in a single declaration. Splitting across UTC midnight is the workaround.

The seasonal threshold tweak ([src/scheduled/confirmation-pass.ts:148-156](../src/scheduled/confirmation-pass.ts#L148-L156)):

```ts
// Seasonal threshold: last 2 weeks of regular season (April 1-14)
// use threshold of 1 instead of 2 because rest games are rampant.
// Playoffs (April 15+): back to 2 but future enhancement will flag any starter out.
const now = new Date();
const month = now.getUTCMonth();
const day = now.getUTCDate();
const isEndOfSeason = month === 3 && day <= 14;
const cancelThreshold = isEndOfSeason ? 1 : 2;
```

This is the "End-of-season minefield" memory in MEMORY.md made operational — but the threshold is a hand-tuned constant, not derived from data. A V2 cell that knows its own variance regime would self-tune this.

---

## Appendix I: System log + structured logging

[src/lib/logger.ts](../src/lib/logger.ts) (referenced but not read in full during this audit) is the structured logging facade. The `system_log` table ([schema/migration-007-system-log.sql](../schema/migration-007-system-log.sql)) is the backing store:

```sql
CREATE TABLE IF NOT EXISTS system_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL DEFAULT (datetime('now')),
  level TEXT NOT NULL,          -- 'info' | 'warn' | 'error' | 'debug'
  category TEXT NOT NULL,       -- 'cron_run' | 'cron_error' | 'api_call' | 'api_error' |
                                -- 'settlement' | 'prediction' | 'data_quality' | 'model_load' |
                                -- 'calibration' | 'admin'
  message TEXT NOT NULL,
  context TEXT,                 -- JSON blob
  resolved BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_log_level ON system_log(level);
CREATE INDEX idx_log_category ON system_log(category);
CREATE INDEX idx_log_timestamp ON system_log(timestamp);
```

Every cron logs `cron_run` start and end events, errors as `cron_error`, predictions as `prediction`. The retention policy is 7 days ([src/scheduled/post-game-settlement.ts:142-148](../src/scheduled/post-game-settlement.ts#L142-L148)):

```ts
try {
  const deleted = await cleanupOldLogs(env, 7);
  if (deleted > 0) {
    await log.debug(env, "admin", `Cleaned up ${deleted} old log entries`);
  }
} catch {
  // Non-fatal
}
```

The `/api/admin/logs` endpoint ([src/index.ts:392-425](../src/index.ts#L392-L425)) exposes the log table with filters by level / category / since-window / limit:

```ts
const logs = await queryLogs(c.env, {
  level,
  category: category as Parameters<typeof queryLogs>[1]["category"],
  since: sinceTimestamp,
  limit: limitStr ? parseInt(limitStr, 10) : 50,
});
```

Pattern for V2: a structured log table with predefined `category` enum, indexed for fast queries, with auto-rolloff. The `context` JSON blob is unstructured; the indexed fields (level, category, timestamp) are what queries filter by. This is a well-trodden Cloudflare-Workers + D1 pattern and ports cleanly.

---

## Appendix J: Admin trigger surface (full enumeration)

`POST /api/admin/trigger/:job` at [src/index.ts:74-264](../src/index.ts#L74-L264) is the manual override for every cron + several diagnostic operations. The full switch:

| job | Action | Direct cron? |
|---|---|---|
| `odds` | `dailyOddsSync(env)` | yes |
| `stats` | `dailyStatsSync(env)` | yes |
| `analysis` (`?force=true` for hard reset) | `preGameAnalysis(env)` | yes |
| `settlement` | `postGameSettlement(env)` | yes |
| `player-props` | `playerPropAnalysis(env)` | yes |
| `player-prop-odds` | `syncPlayerPropOdds(env)` | indirect (called by player-prop-analysis) |
| `player-stats-backfill` | `playerStatsBackfill(env)` | indirect (ISS-013 fix) |
| `confirm` | `confirmPredictions(env)` | yes |
| `adjust-predictions` | `applyPredictionAdjustments(env)` | yes |
| `backtest-adjustments` | `backtestAdjustments(env)` | diagnostic |
| `pit-histogram` | inline PIT histogram computation | diagnostic |
| `test-xgb` | direct fetch test for XGBoost connectivity (debug) | diagnostic |

Plus several non-`:job` admin endpoints:

| Path | Action |
|---|---|
| `POST /api/admin/backfill/players` | `backfillPlayerStats(env, start, end)` |
| `POST /api/admin/calibrate` | `fitCalibration(...)` from settled predictions, write to KV |
| `POST /api/admin/backfill` | `backfillGames(env, start, end)` |
| `POST /api/admin/shadow` | `registerShadowModel(version, weightsKey, description)` |
| `GET /api/admin/shadow/compare` | `compareShadowPerformance(env, shadow, production)` |
| `GET /api/admin/shadow` | list registered shadow models |
| `GET /api/admin/logs` | `queryLogs(env, { level, category, since, limit })` |

The pattern: every cron is callable manually with the same arguments via the admin trigger surface. This makes the system *fully replayable* — anything the cron can do, an operator can also do. V2 should preserve this rigorously: every scheduled job must have a manual trigger.

---

## Appendix K: Type system at a glance

The `FeatureVector` is the canonical input type, but the prediction pipeline produces a richer object hierarchy. From [src/lib/types.ts](../src/lib/types.ts):

| Type | Where it lives | Purpose |
|---|---|---|
| `Env` | `src/lib/types.ts:2-17` | Cloudflare bindings + secrets + env vars |
| `BookmakerTier` | `src/lib/types.ts:20` | `"sharp" \| "mid" \| "soft"` |
| `FeatureVector` | `src/lib/types.ts:101-162` | 50 nullable feature columns |
| `FeatureRow` | `src/lib/types.ts:165-179` | `FeatureVector` + persistence metadata + edge fields |
| `Prediction` | `src/lib/types.ts:182-217` | Full predictions row |
| `EdgeDirection` | `src/lib/types.ts:98` | `"OVER" \| "UNDER" \| "NO_EDGE"` |
| `PredictionResult` | `src/lib/types.ts:220` | `"WIN" \| "LOSS" \| "PUSH"` |
| `BetSizing` | `src/lib/types.ts:223` | `"flat" \| "kelly" \| "half_kelly" \| "quarter_kelly"` |
| `WagerResult` | `src/lib/types.ts:226` | `"WIN" \| "LOSS" \| "PUSH" \| "VOID" \| "CASHOUT"` |
| `BetSource` | `src/lib/types.ts:229` | `"manual" \| "imported_csv" \| "app_recommendation"` |
| `Wager` | `src/lib/types.ts:232-267` | Imported or manual wager record |
| `BankrollAccount` | `src/lib/types.ts:270-279` | Per-bettor account |
| `BankrollTransaction` | `src/lib/types.ts:291-301` | Append-only ledger |
| `Bettor` | `src/lib/types.ts:304-309` | User identity (Edwin or brother) |
| `MarketType` | `src/lib/types.ts:312-316` | `"moneyline" \| "spread" \| "total" \| "player_prop" \| ...` |
| `BacktestRun` | `src/lib/types.ts:319-347` | Aggregate metrics from one backtest invocation |
| `Player` / `PlayerGameStats` / `PlayerSeasonAverage` | `src/lib/types.ts:359-440` | Player layer types |
| `ResearchQueryType` | `src/lib/types.ts:442-450` | `"edge_scan" \| "game_analysis" \| ...` |
| `ModelVersion` | `src/lib/types.ts:452-463` | Row in `model_versions` table |
| `HealthResponse` | `src/lib/types.ts:478-496` | Shape of `/api/health` |

This type module is **purely descriptive of the database** — every type maps 1:1 to a table or column-set. There are no abstract types ("Cell", "Cube", "Dimension"). MarketingCubes V2 will need a much richer type vocabulary; claw-edge has none of it as precedent.

---

## Appendix L: Cross-cutting constants reference

Constants that appear in multiple places, often with subtle differences. These are the most common source of "the test passes but production silently does something else" bugs.

| Constant | Sites | Values |
|---|---|---|
| `residual_std` (V1.6) | TS embedded fallback `17.251` ([src/services/probability-model.ts:147](../src/services/probability-model.ts#L147)); KV-stored `model:weights:v1.6-lasso-player` (matching) ; `metrics.json` `17.251174677778263` ; walk-forward script `17.278` ([training/walk_forward_backtest.py uses training-fold std](../training/walk_forward_backtest.py)); EXP-011 `17.278` ([training/exp011_walk_forward_real_lines.py:32](../training/exp011_walk_forward_real_lines.py#L32)); ISS-011 walkback `17.251` ([training/iss011_walkback.py:58](../training/iss011_walkback.py#L58)); simulator default `17.278` ([training/simulator/config.py:51](../training/simulator/config.py#L51)) |
| `EDGE_THRESHOLD` (production) | wrangler.toml `DEFAULT_PROBABILITY_THRESHOLD = "0.10"` (the legacy `DEFAULT_EDGE_THRESHOLD = "3.0"` is unused) ; EXP-011 `EDGE_THRESHOLD = 0.10` ; simulator default 0.02 (EV) but experiments override to 0.10 (edge_pp) |
| `PLAYOFF_OFFSET_POINTS` | Lasso path `-18` ([src/services/probability-model.ts:238](../src/services/probability-model.ts#L238)); XGBoost path `-12` ([src/services/xgboost-client.ts:80](../src/services/xgboost-client.ts#L80)). **Mismatched.** Both are tactical corrections with no shared source. |
| `MIN_COMPLETED_GAMES_FOR_FULL_MODEL` | `50` ([src/services/probability-model.ts:216](../src/services/probability-model.ts#L216)). Below this, cold-start kicks in. |
| `ET_OFFSET_HOURS` | `-4` ([src/lib/dates.ts:13](../src/lib/dates.ts#L13)). EDT only — comment says "Change to -5 for EST in November." A recurring drift risk every November. |
| `staleThreshold` for features | 12 hours ([src/services/feature-engine.ts:64](../src/services/feature-engine.ts#L64)) |
| Closing-line lookback | 30 days ([src/services/clv-tracker.ts:281](../src/services/clv-tracker.ts#L281)) |
| Confirmation gate window | 20-100 minutes before tipoff ([src/scheduled/confirmation-pass.ts:80-81](../src/scheduled/confirmation-pass.ts#L80-L81)) |
| Confirmation gate threshold | 1 in April 1-14 (end-of-season), 2 otherwise ([src/scheduled/confirmation-pass.ts:154-156](../src/scheduled/confirmation-pass.ts#L154-L156)) |
| Lineup adjustment factor | `0.6 × missing_ppg` ([src/services/lineup-adjuster.ts:91](../src/services/lineup-adjuster.ts#L91)) |
| Lineup impact threshold | `pts_avg >= 15` PPG ([src/services/lineup-adjuster.ts:215](../src/services/lineup-adjuster.ts#L215)) |
| Bottom-up range guard | `[150, 320]` ([src/services/prediction-adjuster.ts](../src/services/prediction-adjuster.ts)) |
| Bottom-up nudge cap | ±15 points ([src/services/prediction-adjuster.ts](../src/services/prediction-adjuster.ts)) |
| Hardcoded bankroll | `1000` ([src/scheduled/pre-game-analysis.ts:268](../src/scheduled/pre-game-analysis.ts#L268)) |
| Hardcoded unit size | `10` ([src/scheduled/pre-game-analysis.ts:268](../src/scheduled/pre-game-analysis.ts#L268)) |
| Player feature minimum threshold | 30% of team's games AND avg ≥ 20 MPG ([src/services/feature-engine.ts:440-444](../src/services/feature-engine.ts#L440-L444)) |
| Top-3 PPG selection minimums | `minutes_avg >= 20` AND `games_played >= 5` ([src/scheduled/confirmation-pass.ts:265-266](../src/scheduled/confirmation-pass.ts#L265-L266)) |
| XGBoost timeout | 5,000 ms ([src/services/xgboost-client.ts:12](../src/services/xgboost-client.ts#L12)) |
| Confirmation gate cron expirationTtl | 86,400 (1 day) for KV summary |

Every one of these constants is a load-bearing magic number. V2 should treat them as named, versioned, typed configuration — not as scattered `const` declarations.
