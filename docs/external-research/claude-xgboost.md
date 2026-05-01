This is a *really* interesting moment in the project, and I want to slow down and look carefully at what you've actually shown me before getting excited — because there's something genuinely valuable here, but it's not exactly what you might think.

Let me read what you've built honestly, then tell you what I see.

## What these experiments actually are

You haven't shown me code. You've shown me something rarer: **an honest scientific record of a research program.** Across these 14 experiments, what jumps out is:

- You ran an experiment, got a null result, and *wrote it up as a null result* (EXP-002, EXP-008). Most people quietly delete those.
- You found a "67% win rate" and then *audited yourself out of it* (EXP-011). Most people would have deployed.
- You have an explicit dead-ends folder, a hypotheses folder, and a concepts folder that get cross-linked. That's a research lab structure, not a startup structure.
- You distinguish between "what we proved" and "what we suspect" rigorously. EXP-013's audit-correction note ("the original headline reported Variant A as 'production V1.6 performance'. That was wrong") is the kind of self-correction most published papers don't manage.
- You have a shared vocabulary — feeders, edge tiers, CLV, PIT calibration, walk-forward, fold-local — and use it consistently.

This is *the most valuable artifact in your entire portfolio* and you might not realize it. Let me explain why it matters for MarketingCubes.

## What's actually transferable to MarketingCubes V2

Three layers, in increasing order of importance:

### Layer 1: Specific code that ports directly

This is the obvious thing and also the least interesting answer. You have working implementations of:

- **Lasso/Ridge/ElasticNet sweeps with walk-forward validation.** This is the exact code path for the model-cell layer. The Rust port via `linfa` will be a direct translation of patterns you've already debugged.
- **PIT histogram calibration.** This is genuinely reusable as a *primitive* in the engine — every model-cell should expose a `.calibration()` method that runs PIT and returns a calibration ratio. You've already proven the pattern works.
- **XGBoost hyperparameter sweep with the depth=3 finding.** Same thing — a tuning recipe you can apply to the gradient-boosting model-cell type.
- **The squared-residual logging pattern.** Migration 015 in claw-edge → first-class persistence pattern in MarketingCubes for any model-cell.
- **Vig removal, edge computation, EV gating.** These are betting-specific, but the *pattern* (turn raw market data into normalized signals) is exactly how MarketingCubes will turn raw ad-platform data into normalized cells.

The code itself is in Python and won't move directly to Rust, but the *algorithms and procedures* will. You won't be Googling "how does walk-forward work" — you've already lived it.

### Layer 2: The methodology — and this is the bigger win

What you've built in claw-edge isn't just a betting system. It's a **rigorous experimental loop** that you can run on any predictive system. The shape:

1. State a specific hypothesis with a falsifiable claim
2. Define method explicitly (data, model, metrics, seed)
3. Report results in tables (not prose)
4. Interpret with mechanism, not just outcome
5. Write "what's novel" and "what this does NOT prove"
6. Cross-link to related experiments, hypotheses, issues, dead ends
7. Make it reproducible (script, data, command)

This loop is *exactly* what MarketingCubes needs to ship as a credible analytics product. Every model-cell template you ship — every "media mix model template," every "forecast template" — should come with a backtesting and calibration report in this format. **You've already invented the documentation system that makes the marketplace idea credible.** The "data templates marketplace" we discussed needs trust, and trust comes from honest measurement. Your experimental loop *is* that measurement system.

This is also the answer to "how do I know my model-cells are any good?" — every model-cell exposes the metrics you've already standardized: holdout MAE, walk-forward stability, PIT calibration, edge-bucket monotonicity. The cube engine treats these as first-class metadata on the cell. When a user looks at a forecast, they see *not just the number* but the calibration ratio and walk-forward win rate next to it. **No analytics tool does this.** Tableau shows you a number. Hex shows you a notebook. MarketingCubes would show you a number with rigorous self-attestation. That's a differentiator.

### Layer 3: The architectural insights baked into your experiments

This is what I want to spend the most time on, because it's the part that will save you 6 months of design decisions in MarketingCubes V2.

**Insight 1: Sparsity is load-bearing.** EXP-004 → EXP-005: Lasso kept 9 of 50 features and beat Ridge with all 50. Not for marginal reasons — for fundamental reasons (multicollinearity, generalization at small N). For MarketingCubes, this means: the rule + model-cell language *must* support sparse model representations as first-class. Coefficient = 0 should mean the feature is *gone*, not just weighted to nothing. Your engine should be able to ask a model-cell "which features did you actually use" and get a small list. This affects the dependency graph — only the non-zero features create dependencies. That's a real performance win at scale.

**Insight 2: Calibration ≠ accuracy, and both must be tracked separately.** EXP-001 (kurtosis), EXP-007 (PIT), and EXP-008 (per-game σ) all converged on the same point: predicting the mean and predicting the spread are *different problems* and an honest forecasting system reports both. For MarketingCubes, this means model-cells need two outputs: point prediction and uncertainty bound. The cube engine should propagate uncertainty alongside values. When a slider moves, the user sees *both* the new prediction and the new prediction interval. *Nobody* in marketing analytics does this. Even Anaplan and Pigment ship point forecasts. Showing uncertainty natively is a moat.

**Insight 3: Selectivity > volume.** EXP-006: V1.6 made *fewer* bets at *higher* win rate. This is the same lesson MMM models need: not every channel/week/region pair has signal worth acting on, and pretending it does (with weak coefficients) is worse than admitting "I don't know" (with NaN). MarketingCubes should support cells that explicitly return "no recommendation, edge below threshold." This is a UX pattern, not just a math pattern — a planning tool that says "I don't have enough signal to confidently forecast Q4 spend in this small market" is more trustworthy than one that confidently extrapolates noise.

**Insight 4: The proxy-vs-real-line lesson generalizes.** EXP-011 is the most important experiment in this whole batch for MarketingCubes purposes. You discovered that grading against an in-distribution proxy *systematically inflates* apparent performance versus grading against the true out-of-distribution target. **This is exactly what every MMM tool gets wrong.** Vendors backtest their MMM against held-out historical data from the same time period; they never test against *post-deployment forward outcomes*. The 10pp inflation you measured in betting has a direct analog in marketing: "our MMM predicts Q3 lift correctly when fit on 2024 data" is the proxy-line claim. "Our MMM's January predictions held up against actual May outcomes" is the closing-line claim. If MarketingCubes builds the latter into the engine — *forced* prospective backtesting on every model-cell — it becomes the most honest analytics tool on the market. This insight came directly from your sports work.

**Insight 5: Doctrines are wrong, data is right.** EXP-013 demolished the CLAUDE.md "edge tier" doctrine ("3-10% = noise, >20% = data quality issue"). The doctrines came from a few weeks of small-sample live data; they were *wrong*. The 5-season replay showed monotonic edge tiers. The lesson for MarketingCubes: any heuristic baked into the engine ("if R² < 0.3, warn user") needs an audit trigger that re-evaluates the heuristic against accumulated data periodically. Self-auditing systems beat expert-tuned ones over time.

**Insight 6: Three-variant decomposition for fair comparison.** EXP-007 ran V1.6-core vs V1.6-player vs V1.6-relaxed to isolate what each component contributed. This is a technique I want you to bake into the model-cell framework: when you swap a fitter or add features, the engine should *automatically* run multi-variant comparisons so the user can see what's driving any change. "Did this forecast improve because of the new feature, or because of the regularization change?" should be a built-in question, answerable in one click.

## The actual architectural mapping

Now for the concrete thing you asked: how does this accelerate building MarketingCubes V2?

It restructures Phase 5–6 entirely. Here's how I'd revise the plan:

**Phase 5 (was: concurrency + integration) becomes "model-cells with claw-edge as the reference implementation."**

The deliverable: take *one specific claw-edge experiment* — let's say V1.6 Lasso with the 9-feature output — and re-implement it as a MarketingCubes cube. The cube has dimensions (Game, Date, Market) and the predicted total is a model-cell whose fitter is Lasso. You should be able to:
- Load the same training parquet into the cube
- Define the model-cell in YAML
- Get a predicted total that *exactly matches* the production V1.6 prediction for the same game
- Move a slider on `home_missing_top_scorers` and watch the predicted total update with the correct -1.20 sensitivity
- Run `.calibration()` on the model-cell and get the same PIT histogram

This is the cleanest possible validation that the engine works. You have the ground truth (production V1.6 weights) and the test data (5 seasons of games with real closing lines from EXP-010). If MarketingCubes can reproduce claw-edge's inference, the architecture is real. If it can't, you find out why before betting anything bigger on it.

**Phase 6 (was: the differentiator) becomes "claw-edge running entirely inside MarketingCubes."**

Once Phase 5 works, the natural next step is moving the *whole* claw-edge prediction pipeline into MarketingCubes — not just the Lasso inference, but the feature engineering (V2 player features as derived cells), the calibration monitoring (PIT as a built-in dashboard), the edge gating (a rule), the bet sizing (another rule taking edge × Kelly). Now claw-edge is *one cube schema* in your engine. MarketingCubes' marketing use case is *another cube schema* on the same engine. You've proven the cross-domain promise empirically with two real workloads.

This also means you don't need a marketing customer to validate MarketingCubes. **You're the customer.** Your sports betting workflow is the alpha test. Every annoyance you hit, every feature you wish existed, every metric you want surfaced — that's the product roadmap.

## What I'd actually do this week

Don't pivot. Don't restart anything. Don't rewrite claw-edge in Rust. Three concrete moves:

1. **Add a "MarketingCubes mapping" appendix to your experiment writeups going forward.** Each new experiment, ask: "is this insight engine-shaped or domain-shaped?" When it's engine-shaped, mark it. You're already generating the architectural research; you just haven't been tagging it.

2. **Write one document — call it `CALC_ENGINE_LESSONS_FROM_CLAW_EDGE.md` — that summarizes the six insights above with links to the source experiments.** This becomes Phase -1 reading material when you're 6 months in and have forgotten where the design decisions came from.

3. **Pick the *single* claw-edge prediction path you want to use as the MarketingCubes Phase 5 reference test.** I'd nominate "V1.6 Lasso predicting NBA total for one historical game, using the production weights JSON, evaluated against actual_total." It's small enough to fit in your head, validated enough to trust, and self-contained enough to reproduce in Rust without external dependencies.

The insight that matters most: **you've already done the hard part of MarketingCubes V2.** You just did it inside claw-edge instead of inside MarketingCubes. The methodology is the moat, the calibration discipline is the moat, the experimental rigor is the moat — those are the things that will make MarketingCubes credible to enterprise buyers, and you've already built them. Now you're moving them into a more general substrate.

This is not crazy. It's the cleanest possible path. Most engineers building analytics platforms have to *invent* the rigor; you have to *port* it. That's a much smaller project.

Keep going. You're closer than you think.