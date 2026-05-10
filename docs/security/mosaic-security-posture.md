# Mosaic Security Posture

**Status:** Active — maintained document; update when new attack surfaces are added or tooling changes
**Date:** 2026-05-09
**Scope:** Secure-development baseline for the Mosaic codebase; prerequisite to any Grout implementation work
**Owner:** project lead

> This is the "boring security first" document. It establishes the development-side security hygiene baseline that must be in place before any advanced application-layer integrity work (Grout) ships. Good security begins with supply chain hygiene, hardened inputs, responsible disclosure, and a clear threat model — not with watermarking.
>
> Aligned with NIST SSDF (SP 800-218) principles: secure the development environment, protect code from unauthorized access, produce well-secured software, and respond to vulnerabilities.

---

## 1. Supply chain: dependency auditing

### `cargo audit`

Run `cargo audit` in CI and before every release. Checks all dependencies against the RustSec Advisory Database for known CVEs and security advisories.

```bash
# Install
cargo install cargo-audit

# Run
cargo audit

# CI integration: fail the build on any unfixed advisory
cargo audit --deny warnings
```

**Policy:** No unfixed high-severity advisories may be present at release time. Medium-severity advisories require a documented mitigation or suppression with justification.

**Suppression format (in `audit.toml` at workspace root):**
```toml
[[advisories]]
id = "RUSTSEC-YYYY-XXXX"
reason = "Not reachable in our usage because [specific reason]"
```

### `cargo deny`

Run `cargo deny` to check licenses, detect duplicate dependencies, and block disallowed crates.

```bash
# Install
cargo install cargo-deny

# Run
cargo deny check

# Checks: advisories, bans, licenses, sources
```

**Policy for `deny.toml`:**
- Ban licenses: GPL-2.0, AGPL-3.0 (copyleft incompatible with commercial use)
- Allow: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, CC0-1.0
- Block crates from unknown/unreviewed sources (allow crates.io + github.com)
- Alert on duplicate versions of the same crate (flag for review; don't auto-fail unless high-risk crate)

### Dependency update policy

- Review `cargo update` output monthly
- Pin major versions explicitly in `Cargo.toml`; allow patch updates implicitly
- New direct dependencies require: (1) license review, (2) crate popularity/maintenance check (downloads, recent commits), (3) review of the crate's own dependency tree for hidden heavy or risky deps
- The five allowed runtime deps for `mc-core` are binding per CLAUDE.md §1. New deps to `mc-core` require a SPEC QUESTION first.

---

## 2. Input hardening

### Upload and zip file handling

The demo server (`mc-demo-server`) accepts ZIP uploads. ZIP files are an attack surface:
- **Zip slip** — path traversal via `../` in archive entry names. Validate that every extracted path stays within the target directory.
- **Zip bombs** — small archives that expand to enormous sizes. Enforce uncompressed size limit before extraction.
- **Deeply nested archives** — archives containing archives. Don't recursively extract.

**Current policy:**
```rust
// Maximum upload size: 50MB compressed
// Maximum uncompressed: 500MB (10x ratio limit)
// Validate every entry path: no `..`, no absolute paths
// Reject archives-within-archives
```

Validate that the path normalization strip happens BEFORE checking the result against the allowed directory, not after. This is the most common zip-slip implementation bug.

### YAML and TOML parsing

`mc-model` parses YAML model files. YAML has a history of security issues (billion laughs attacks, arbitrary code execution in unsafe parsers).

**Policy:**
- Use `serde_yaml` with no custom tag handling
- Enforce maximum nesting depth (default is unbounded in some YAML parsers)
- Enforce maximum string length on individual values
- Reject model files larger than 10MB
- Add a `mc model validate --strict-limits` flag that enforces stricter bounds for untrusted input

### CSV and Parquet ingestion

Tessera ingests CSV and Parquet from customer data. These are attack surfaces for:
- Extremely large files that exhaust memory
- Malformed records that exercise error paths in parsers
- CSV injection (if outputs are ever written back to CSV and opened in Excel — `=cmd(...)` style)

**Policy:**
- Maximum Tessera import file size: 500MB per file, 2GB per recipe run (configurable per deployment)
- CSV output never includes values that start with `=`, `+`, `-`, `@`, `|` without escaping — if Mosaic ever writes CSV for human use, prefix these with a `'` character
- Validate record counts; reject files with >100M rows (misconfiguration or attack signal)

### Formula parser

The formula parser (`mc-model/src/parser.rs`) accepts user-authored formulas. Expressions can be deep:
- Deeply nested expressions that exhaust the stack
- Expressions that produce very long evaluation chains

**Policy:**
- Maximum expression depth: 50 nodes (configurable; current default should be documented)
- Maximum formula string length: 10KB
- Parser errors produce diagnostics (per ADR-0024 rich diagnostics), not panics

### Narrative template YAML

Narrative templates (`mc-narrative`) are YAML files with embedded expression strings. Same concerns as model YAML plus:
- Expressions in templates are evaluated against cube data — expression depth limits apply
- Template files: maximum 1MB
- Template library: maximum 10,000 templates per workspace

---

## 3. Fuzzing plan

Fuzzing is essential for parsers. Priority order for Mosaic:

**P0 — Formula parser (`mc-model/src/parser.rs`):** Accepts the widest range of user-authored input. Fuzz with `cargo-fuzz`. Target: no panics, no memory corruption, all errors return `Result::Err`.

**P1 — YAML model loader (`mc-model/src/loader.rs`):** User-authored model files. Fuzz with `cargo-fuzz`. Target: no panics on malformed YAML.

**P2 — Tessera CSV driver:** Customer data ingestion. Fuzz with `cargo-fuzz`. Target: no panics on malformed CSV.

**P3 — ZIP upload handler (`mc-demo-server`):** User-uploaded ZIP files. Target: no path traversal, no resource exhaustion.

**When to add fuzzing:** Before Phase 8 (service daemon) ships. The daemon introduces network-exposed endpoints with user-controlled input. Fuzz the parsers before those endpoints go live.

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Create fuzz target for formula parser
cargo fuzz add formula_parser

# Run
cargo fuzz run formula_parser
```

---

## 4. Threat model (baseline)

A full threat model lives in `docs/research-notes/grout-security-architecture-vision.md`. This section captures the development-side view.

### The current attack surface

**Shape 1 (Library + CLI) — current:**
- Local file read/write on `.mosaic/` directory — trust model is OS filesystem permissions
- `mc-demo-server` HTTP endpoints — network-exposed if `mc start` is run; accepts ZIP uploads
- YAML and CSV parsing from user-provided files

**Shape 4 (Daemon) — Phase 8:**
- Persistent HTTP/MCP API endpoint — larger network attack surface
- SQLite database file — persistent state; local filesystem still the trust model for single-user
- Multi-user: first time session tokens or API keys are needed

**Shape 6 (Cloud) — Phase 9:**
- Multi-tenant: cross-tenant data isolation becomes critical
- External network: TLS, auth, rate limiting all required
- Customer data: GDPR, CCPA, SOC2 considerations

### Application-layer threats we own

Per `docs/research-notes/grout-security-architecture-vision.md`:
- Storage-layer integrity of Mosaic's own data structures (hash chains when Grout ships)
- Append-only audit trail integrity (hash chains)
- Data exfiltration attribution (watermarking on derived exports when Grout ships)

### Infrastructure threats we don't own (operator's responsibility)

- Ransomware (infrastructure-layer attack)
- Network security (TLS configuration, firewall, DDoS)
- OS hardening (patch management, privilege separation)
- Backup management
- Physical security

---

## 5. Secure coding checklist (for PR review)

### Input handling
- [ ] User-controlled input goes through a length/size check before processing
- [ ] File paths from user input are sanitized (no `../`, no absolute paths to system directories)
- [ ] YAML/JSON parsing uses bounded configurations (depth limit, size limit)
- [ ] Uploaded archives are extracted with zip-slip protection

### Error handling
- [ ] No panics in user-facing code paths — all errors propagate as `Result`
- [ ] Error messages don't leak internal file paths, stack traces, or sensitive state
- [ ] Parser errors produce user-friendly diagnostics (per ADR-0024), not raw Rust errors

### Dependencies
- [ ] No new `mc-core` dependency without SPEC QUESTION (per CLAUDE.md §1)
- [ ] New dependencies in shell crates reviewed for license, maintenance, and dep tree
- [ ] `cargo audit` passes with no unfixed advisories

### Secrets and credentials
- [ ] No API keys, tokens, passwords, or signing keys in source code
- [ ] Tessera recipes with credentials use the secrets layer (Phase 5D or environment variables)
- [ ] Log output strips credential values (no `Bearer token_value` in debug logs)

---

## 6. Responsible disclosure

### Policy (draft — to be published before any public release)

Mosaic is not yet in public production. Before any public release (Phase 6C distribution + install pipeline), publish a responsible disclosure policy at the project's primary web presence.

**Draft policy:**

> If you discover a security vulnerability in Mosaic, please report it privately before public disclosure. Email: [security@mosaic-tool.dev — placeholder, update before publishing]
>
> We commit to: acknowledging receipt within 48 hours, providing an initial assessment within 7 days, and coordinating disclosure timing with you.
>
> Please do not: disclose publicly before we have had a reasonable opportunity to address the issue, use the vulnerability to access or modify data that isn't yours, or perform denial-of-service attacks.
>
> We will: credit you in release notes (unless you request anonymity), attempt to address the issue before public disclosure, and keep you informed of our progress.

### CVE process

For critical vulnerabilities in shipping code, file a CVE if the issue is in Mosaic's own code (not a dependency). If the issue is in a dependency, report it to RustSec (`https://rustsec.org`) and the upstream crate maintainer.

---

## 7. CI gates (to implement before Phase 8)

These checks should run on every PR before Phase 8 (service daemon) ships:

```yaml
# .github/workflows/security.yml (placeholder)
security:
  steps:
    - cargo audit --deny warnings
    - cargo deny check
    - cargo clippy --all-targets -- -D warnings  # already in CI; catches memory issues
    - cargo test --workspace                       # already in CI
    # Future when fuzzing is set up:
    # - cargo fuzz run formula_parser -- -max_total_time=60
```

**Minimum gate before Phase 8:** `cargo audit` and `cargo deny` must pass on every merge. Phase 8 introduces network-exposed endpoints; the supply chain must be clean before that happens.

---

## 8. What this is NOT

This document is NOT:

- A penetration test or security audit result
- A claim that Mosaic is "secure" in any general sense
- A substitute for professional security review before handling sensitive customer data at scale
- The Grout architecture (see `docs/research-notes/grout-security-architecture-vision.md`)

This document establishes the development-side hygiene baseline. It reduces supply-chain risk, hardens parsers against obvious attacks, and creates the infrastructure for responsible disclosure. It is the necessary foundation — not the complete picture.

**Before handling customer financial data in production (Phase 9):** engage a professional security review. The review should cover the threat model, the access control model (ADR-0026 capability grants), the network exposure (Phase 8 daemon), and the storage model. This document's checklist is the internal baseline; the professional review is the external validation.

---

## Cross-links

- **Grout research note:** [`../research-notes/grout-security-architecture-vision.md`](../research-notes/grout-security-architecture-vision.md) — application-layer integrity; builds on this baseline
- **ADR-0025:** [`../decisions/0025-kernel-discipline-and-deployment-architecture.md`](../decisions/0025-kernel-discipline-and-deployment-architecture.md) — kernel is sync/no-cloud; this document covers the shell crates' attack surface
- **ADR-0026:** [`../decisions/0026-org-workspace-resource-scope-capability-grants.md`](../decisions/0026-org-workspace-resource-scope-capability-grants.md) — capability grants are the access control model; their integrity is Grout's job
- **CLAUDE.md** — forbidden patterns (no `unsafe`, no `unwrap`, no secrets in source code) — kernel-level constraints that overlap with security posture
- **RustSec:** https://rustsec.org — advisory database for `cargo-audit`
- **NIST SSDF:** SP 800-218 — secure software development framework referenced for alignment
