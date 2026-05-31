// ===========================================================================
// Metrics, drawdown scans, and Monte Carlo (Steps 6-7; Decisions 6-7,
// Amendments 6, 7, 13). Included into `simulate.rs`.
// ===========================================================================

/// Recovery outcome relative to the max-drawdown trough (Amendment 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Bankroll never dropped below a prior peak.
    NeverUnderwater,
    /// Trough recovered to the prior peak within the path.
    Recovered,
    /// Trough never recovered to the prior peak by the end of the path.
    Unrecovered,
}

impl RecoveryStatus {
    fn as_str(self) -> &'static str {
        match self {
            RecoveryStatus::NeverUnderwater => "never_underwater",
            RecoveryStatus::Recovered => "recovered",
            RecoveryStatus::Unrecovered => "unrecovered",
        }
    }
}

/// Single-path metrics (Decision 7). Nullable metrics are `Option` and emit
/// `null` in JSON — never `∞`/NaN (Amendment 7).
#[derive(Debug, Clone)]
pub struct Metrics {
    pub start_bankroll: f64,
    pub final_bank: f64,
    /// Cumulative ROI: (final − start) / start (Amendment 13).
    pub roi: Option<f64>,
    pub total_staked: f64,
    pub total_pnl: f64,
    /// Per-bet ROI: total_pnl / total_staked (Amendment 13). Null on 0 stake.
    pub roi_per_bet: Option<f64>,
    pub n_bets: usize,
    pub wins: usize,
    pub losses: usize,
    pub pushes: usize,
    pub win_rate: Option<f64>,
    /// Largest peak-to-trough decline as a fraction of the peak.
    pub max_drawdown: f64,
    pub recovery_bets: Option<usize>,
    pub recovery_status: RecoveryStatus,
    pub sharpe: Option<f64>,
}

/// Scan the bankroll path for the max drawdown and recovery (Decision 7).
/// `peak` is seeded at `start_bankroll` so the first bet's drawdown is
/// measured against the starting capital (matches claw-core's curve).
fn drawdown_scan(start_bankroll: f64, path: &[f64]) -> (f64, Option<usize>, RecoveryStatus) {
    if path.is_empty() {
        return (0.0, None, RecoveryStatus::NeverUnderwater);
    }
    let mut peak = start_bankroll;
    let mut max_dd = 0.0_f64;
    let mut peak_at_maxdd = start_bankroll;
    let mut trough_idx: Option<usize> = None;
    for (i, &v) in path.iter().enumerate() {
        if v > peak {
            peak = v;
        }
        if peak > ZERO_EPS {
            let dd = (peak - v) / peak;
            if dd > max_dd {
                max_dd = dd;
                peak_at_maxdd = peak;
                trough_idx = Some(i);
            }
        }
    }
    if max_dd < 1e-12 {
        return (0.0, None, RecoveryStatus::NeverUnderwater);
    }
    // Recovery: first bet after the trough whose bankroll regains the prior
    // peak.
    let trough = trough_idx.unwrap_or(0);
    for (offset, &v) in path[trough..].iter().enumerate() {
        if v >= peak_at_maxdd {
            return (max_dd, Some(offset), RecoveryStatus::Recovered);
        }
    }
    (max_dd, None, RecoveryStatus::Unrecovered)
}

/// Sample standard deviation (ddof=1), consistent with ADR-0033 Amendment 3.
/// Returns `None` when n < 2.
fn sample_std(values: &[f64]) -> Option<(f64, f64)> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let ss: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    let var = ss / (n as f64 - 1.0);
    Some((mean, var.sqrt()))
}

/// Compute single-path metrics from a replay result.
fn compute_metrics(res: &ReplayResult) -> Metrics {
    let start = res.start_bankroll;
    let roi = if start.abs() < ZERO_EPS {
        None
    } else {
        Some((res.final_bank - start) / start)
    };
    let roi_per_bet = if res.total_staked.abs() < ZERO_EPS {
        None
    } else {
        Some(res.total_pnl / res.total_staked)
    };
    let win_rate = if res.wins + res.losses == 0 {
        None
    } else {
        Some(res.wins as f64 / (res.wins + res.losses) as f64)
    };
    let (max_dd, recovery_bets, recovery_status) = drawdown_scan(start, &res.bankroll_path);
    let sharpe = match sample_std(&res.per_bet_returns) {
        Some((mean, std)) if std > ZERO_EPS => {
            Some(mean / std * (res.per_bet_returns.len() as f64).sqrt())
        }
        _ => None,
    };
    Metrics {
        start_bankroll: start,
        final_bank: res.final_bank,
        roi,
        total_staked: res.total_staked,
        total_pnl: res.total_pnl,
        roi_per_bet,
        n_bets: res.n_bets,
        wins: res.wins,
        losses: res.losses,
        pushes: res.pushes,
        win_rate,
        max_drawdown: max_dd,
        recovery_bets,
        recovery_status,
        sharpe,
    }
}

// ===========================================================================
// Monte Carlo (Decision 6, Amendment 6)
// ===========================================================================

#[derive(Debug, Clone)]
pub enum Resample {
    Iid,
    Block(usize),
}

#[derive(Debug, Clone)]
pub struct Bands {
    pub p5: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p95: f64,
}

#[derive(Debug, Clone)]
pub struct MonteCarloResult {
    pub runs: usize,
    pub resample: String,
    pub final_bank: Bands,
    pub roi: Bands,
    pub max_drawdown: Bands,
    /// Fraction of runs ending below the starting bankroll.
    pub p_underwater: f64,
}

/// Nearest-rank percentile (Amendment 6 — fixed method for reproducible
/// CIs). `sorted` is ascending; `p` in [0,100].
fn nearest_rank(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    let rank = (p / 100.0 * n as f64).ceil() as usize;
    let idx = rank.clamp(1, n) - 1;
    sorted[idx]
}

fn bands_from(mut vals: Vec<f64>) -> Bands {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Bands {
        p5: nearest_rank(&vals, 5.0),
        p25: nearest_rank(&vals, 25.0),
        p50: nearest_rank(&vals, 50.0),
        p75: nearest_rank(&vals, 75.0),
        p95: nearest_rank(&vals, 95.0),
    }
}

/// Lightweight sequential replay over a resampled index sequence. Returns
/// `(final_bank, max_drawdown)`. Used only by Monte Carlo (no curve / cells).
fn replay_indices(
    pool: &[BetRecord],
    indices: &[usize],
    start_bankroll: f64,
    sizing: &SizingRule,
    odds_src: &OddsSource,
) -> (f64, f64) {
    let mut bank = start_bankroll;
    let mut path: Vec<f64> = Vec::with_capacity(indices.len());
    for &i in indices {
        let rec = &pool[i];
        let odds = match resolve_odds(rec, odds_src) {
            Some(o) if o > 1.0 => o,
            _ => continue,
        };
        let stake = match sizing.size(rec, odds, start_bankroll, bank) {
            SizeOutcome::Stake(s) => s.min(bank),
            SizeOutcome::Skip(_) => continue,
        };
        let (_, new_bank) = apply_outcome(rec.outcome, stake, odds, bank);
        bank = new_bank;
        path.push(bank);
        if bank <= ZERO_EPS {
            bank = bank.max(0.0);
            break;
        }
    }
    let (max_dd, _, _) = drawdown_scan(start_bankroll, &path);
    (bank, max_dd)
}

/// Block-bootstrap candidate block start positions: non-overlapping fixed
/// blocks of length `len` (Amendment 6).
fn block_starts(n: usize, len: usize) -> Vec<usize> {
    if len == 0 || n == 0 {
        return vec![0];
    }
    let mut starts = Vec::new();
    let mut s = 0;
    while s + len <= n {
        starts.push(s);
        s += len;
    }
    if starts.is_empty() {
        starts.push(0); // n < len: one short block covering the whole pool
    }
    starts
}

/// Run the Monte Carlo resampling wrapper.
fn run_monte_carlo(
    pool: &[BetRecord],
    start_bankroll: f64,
    sizing: &SizingRule,
    odds_src: &OddsSource,
    runs: usize,
    resample: &Resample,
    seed: u64,
) -> MonteCarloResult {
    let n = pool.len();
    let mut rng = SplitMix64::new(seed);
    let mut finals: Vec<f64> = Vec::with_capacity(runs);
    let mut rois: Vec<f64> = Vec::with_capacity(runs);
    let mut dds: Vec<f64> = Vec::with_capacity(runs);
    let mut underwater = 0usize;

    let resample_desc = match resample {
        Resample::Iid => "iid".to_string(),
        Resample::Block(l) => format!("block:{l}"),
    };

    for _ in 0..runs {
        let indices: Vec<usize> = match resample {
            Resample::Iid => {
                if n == 0 {
                    Vec::new()
                } else {
                    (0..n).map(|_| rng.index(n)).collect()
                }
            }
            Resample::Block(len) => {
                let starts = block_starts(n, *len);
                let mut seq: Vec<usize> = Vec::with_capacity(n);
                while seq.len() < n && !starts.is_empty() {
                    let s = starts[rng.index(starts.len())];
                    let end = (s + len).min(n);
                    for idx in s..end {
                        seq.push(idx);
                        if seq.len() >= n {
                            break;
                        }
                    }
                }
                seq.truncate(n);
                seq
            }
        };
        let (final_bank, max_dd) = replay_indices(pool, &indices, start_bankroll, sizing, odds_src);
        let roi = if start_bankroll.abs() < ZERO_EPS {
            0.0
        } else {
            (final_bank - start_bankroll) / start_bankroll
        };
        if final_bank < start_bankroll {
            underwater += 1;
        }
        finals.push(final_bank);
        rois.push(roi);
        dds.push(max_dd);
    }

    let p_underwater = if runs == 0 {
        0.0
    } else {
        underwater as f64 / runs as f64
    };

    MonteCarloResult {
        runs,
        resample: resample_desc,
        final_bank: bands_from(finals),
        roi: bands_from(rois),
        max_drawdown: bands_from(dds),
        p_underwater,
    }
}

/// Default block length: `L = max(1, round(sqrt(N)))` (Amendment 6).
fn default_block_len(n: usize) -> usize {
    ((n as f64).sqrt().round() as usize).max(1)
}
