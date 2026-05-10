# Grout — Security Architecture Vision and Research Document

**Status:** Research document — pre-ADR design capture; NOT implementation authorization
**Date:** 2026-05-07
**Last updated:** 2026-05-09
**Compiled by:** Claude Desktop, synthesizing multi-turn conversation with project owner + prior Claude PM input + GPT framing
**Scope:** Application-layer integrity architecture for Mosaic; threat model; mechanism design; deployment-shape evolution; cryptographic provenance
**Sequencing:** Future work (Phase 8.5 or Phase 9); this document captures design space without committing to implementation timing

> **Source-of-truth hierarchy note:** This is a research document, not a binding ADR. Grout has no shipping ADR yet. When Grout's ADR drafts (Phase 8.5+), it supersedes this document. Until then, this captures design intent only.
>
> **Prerequisite before any Grout implementation:** `docs/security/mosaic-security-posture.md` — the secure-development baseline (cargo audit, cargo deny, input limits, etc.) must be in place before Grout primitives ship.
>
> Grout is Mosaic's application-layer integrity architecture. Its invariant: Mosaic's data structures must be verifiably intact, regardless of what the surrounding infrastructure has done. Grout does not replace operator-level security (firewall, IAM, patch policy). It provides defense-in-depth at the application layer when infrastructure controls fail or are misconfigured. With cryptographic provenance, Grout extends Mosaic's positioning from "deterministic interpretation engine" to "deterministic interpretation engine with cryptographic provenance" — a meaningful differentiator for compliance-sensitive deployments and the cartridge marketplace.

---

## Part 1: The mental model

### The brick-and-mortar metaphor

A brick wall uses mortar not to make the bricks stronger, but to ensure the joints between them resist cracking under stress. The bricks are the operator's infrastructure (their problem). Grout fills the joints — the transitions between trust boundaries — with cryptographic integrity so that cracks in the infrastructure don't silently corrupt Mosaic's semantics.

This metaphor produces clean scope decisions:

- **Operator's bricks:** firewall configuration, IAM policies, patch management, encryption-at-rest, network segmentation, OS hardening
- **Grout's mortar:** cryptographic integrity of Mosaic data structures, append-only audit trails, signed exports, watermarked data, canary detection, provenance verification

When someone proposes "Grout should manage user passwords" or "Grout should configure firewalls" — the answer is no. Those are operator concerns. Grout's scope is bounded to what Mosaic can guarantee about its own data.

### The realistic framing: detection and recovery, not prevention

**Grout cannot prevent ransomware.** Ransomware is an infrastructure-layer attack — an attacker with write access encrypts or destroys data. No application-layer integrity tool can prevent this; the attacker has the same write access Mosaic has.

What Grout CAN do:
1. Detect ransomware quickly (canary files / hash chain breaks)
2. Make recovery possible (signed exports + verifiable backups)
3. Make tampering obvious (so corrupted data isn't unknowingly served)
4. Identify the source of leaked data (provenance + watermarking on derived exports)
5. Enable cross-instance trust verification (cryptographic provenance)

**This is meaningful but bounded.** The pitch is "we detect integrity violations and make recovery verifiable" not "we make Mosaic unhackable." The latter is a claim no responsible security architecture makes.

### What Grout is NOT replacing

| Concern | Who owns it |
|---|---|
| Firewall configuration | Operator |
| IAM policies and user authentication | Operator (at deployment shell layer) |
| Patch management and OS hardening | Operator |
| Encryption at rest | Storage layer (filesystem/database) |
| Backups and backup management | Operator |
| Network-layer TLS | Operator / standard libraries |
| User credential management | Operator |
| Ransomware prevention | Impossible at application layer |

---

## Part 2: Threat model

Being explicit about what Grout protects against is as important as the mechanisms. The threat model determines what Grout does and what it doesn't.

### Threats Grout protects against

**1. Storage-layer compromise.** An attacker with write access to the database or files modifies data. Grout's hash chains make modifications detectable; canaries fire when unauthorized access patterns emerge.

**2. Retroactive log sanitization.** An attacker erases their own audit trail after the fact. Hash-chained append-only logs make sanitization detectable — missing or modified rows break the chain.

**3. Privilege escalation via grant manipulation.** An attacker modifies grant records to elevate their access. Signed grant chains (when Grout integrates with ADR-0026's capability grants) detect tampering before the elevated access is honored.

**4. Silent data corruption.** Whether malicious or due to infrastructure bugs, corruption is detected before corrupted data is served. Hash verification on read paths catches this.

**5. Recovery from infrastructure loss.** Region failure, regulatory seizure, vendor bankruptcy — operators can verify exported archives without any Mosaic infrastructure and reconstruct state.

**6. Data exfiltration attribution.** When data leaks (whether via attack, insider threat, or misconfiguration), provenance markers in export artifacts identify the source workspace. Forensic capability when prevention fails.

**7. Cartridge piracy.** Vendors selling cartridges (Phase 10+ marketplace) can detect unauthorized copies via watermarks embedded in benchmark datasets (see watermarking guardrail in Part 3).

**8. Cross-org data leakage.** When data flows between organizations, provenance verification ensures the data originated where it claims and hasn't been tampered with.

### Threats Grout does NOT protect against

**1. Compromise of the running Mosaic process.** If the process is fully compromised, it has access to the signing key. Grout cannot defend against an attacker who has owned the runtime.

**2. Operator-controlled infrastructure failures.** Firewall down, IAM misconfigured, weak OS passwords — these are outside Grout's scope.

**3. User credential theft.** Grout protects data integrity, not authentication. Stolen credentials with legitimate access are out of scope.

**4. Malicious users with legitimate write access.** Grout makes their actions auditable but cannot prevent legitimate users from doing harmful things.

**5. Deletion of the entire workspace.** If an attacker deletes everything, there's nothing to verify. Grout enables verification when data exists; it doesn't prevent destruction.

**6. Side-channel attacks on signing keys.** Timing attacks, power analysis — below Grout's abstraction layer.

---

## Part 3: The five Grout primitives

These are the core mechanisms. Each does something specific; they compose well; the set is small enough to actually ship.

### Primitive 1: Hash-chained append-only logs

Every append-only structure (JSONL logs, ledger rows, grant records) is hash-chained. Each entry's hash includes the previous entry's hash, creating a linked chain.

```
entry_1: { ...content..., hash: sha256(content_1) }
entry_2: { ...content..., prev_hash: hash_1, hash: sha256(prev_hash || content_2) }
entry_3: { ...content..., prev_hash: hash_2, hash: sha256(prev_hash || content_3) }
```

**What this guarantees:**
- A missing entry breaks the chain
- A modified entry breaks all subsequent hashes
- Reordering breaks the chain
- Insertion requires recalculating all subsequent hashes

**Inspiration:** Certificate Transparency (RFC 6962) is built on append-only Merkle Tree logs with exactly these properties. SLSA and in-toto use similar chain-of-custody patterns for software supply chain provenance. Mosaic's approach adapts this pattern to business-number artifacts rather than software artifacts.

**Mosaic structures that need hash-chaining:**
- Tessera import audit log (already append-only; add chain in Grout phase)
- Post-hoc write log (already append-only; add chain)
- Interpretation ledger (Phase 7A.2; already append-only JSONL — design hash fields from start)
- Grant records (Phase 4C; design with chain in mind)
- Snapshot history (first-class; add chain)

### Primitive 2: Canary records and honeytokens

Insert deliberately fake records that legitimate users would never query. If those records appear elsewhere or are accessed, the data was compromised.

**Types of canaries:**

**Canary workspace.** A workspace that exists in the org's workspace table but should never be accessed in production. No legitimate users, no real models. Any access triggers an alert. Catches account takeovers and credential abuse.

**Canary grant.** A grant that appears to give high-privilege access but is never exercised by legitimate users. An attacker scanning for escalation paths exercises it → alert. Routes to a monitored decoy, not real data.

**Canary record.** A deliberately fake row in a real dataset. A marketing cube has markets Tampa, Houston, Atlanta — and "Springvale" (fake). Real analysts never query Springvale. An attacker who exfiltrates the dataset picks it up; if Springvale appears in another system, the data was compromised.

**Canary cell.** Specific cube coordinates that should never be queried legitimately. Triggered by query, useful for detecting reconnaissance.

**Constraints (binding when implemented):**
- Zero false positives — a canary that fires during legitimate use is worse than no canary
- Per-deployment, not hardcoded — each deployment generates its own canaries; hardcoded names can be avoided by attackers who read the source
- Tiered response — a GET on a canary is different from a DELETE; responses escalate by action severity
- Optional and disabled by default in personal/dev workspaces — production enables; dev disables

### Primitive 3: Signed exports and verifiable archives

Every data export from a Mosaic workspace is cryptographically signed. The signature is verifiable independently of any Mosaic instance.

**The export bundle:**
```
my-workspace-export.zip
├── manifest.json              # version, source, timestamp, content hash
├── data/                      # actual cube data
├── ledger.jsonl               # interpretation ledger (hash-chained)
├── audit.jsonl                # audit log (hash-chained)
├── provenance.json            # who exported, when, what cubes
└── signature.sig              # Ed25519 signature of the manifest
```

**What this guarantees:**
- Exported data hasn't been tampered with (manifest hash verifies content)
- The export came from a specific workspace (signature verifies origin)
- The export happened at a specific time
- Verification works without Mosaic infrastructure — just a signature verifier and the public key

**The disaster-recovery scenario:** Region failure, regulatory seizure, vendor bankruptcy — the customer has signed export bundles. They verify them with the public key. The data is reconstructable and trustworthy without depending on Mosaic infrastructure being online.

**Proposed cryptographic primitives (subject to cryptographic review before shipping):**
- Content hashing: SHA-256
- Signing: Ed25519

These are the industry-standard defaults for this kind of work and are what SLSA, Sigstore, and most software supply-chain tooling uses. They are **proposed defaults, not irreversible constitutional commitments** — a cryptographic review before shipping Grout may surface reasons to adjust (algorithm aging, FIPS requirements, hardware HSM compatibility). Don't treat them as locked-in until that review completes.

### Primitive 4: Cryptographic provenance markers

Every cell tracks not just provenance type (input/derived/consolidation/override) but source instance and signature when data flows between Mosaic instances.

**The provenance record:**
```json
{
  "source_instance_id": "instance-xyz",
  "source_workspace_id": "ws-abc-123",
  "source_org_id": "org-def-456",
  "exported_at": "2026-05-09T14:30:00Z",
  "data_hash": "sha256:...",
  "signature": "ed25519:...",
  "schema_version": "1.0"
}
```

**Verification flow:**
1. Workspace A exports data with provenance record
2. Workspace B imports the data
3. Mosaic looks up Workspace A's public key (via registry, direct exchange, or trust-on-first-use)
4. Mosaic verifies the signature on the provenance record
5. Verified data is tagged in Workspace B as "originated from ws-abc-123 on date X"

**Inspiration sources:** SLSA defines provenance as verifiable information about where, when, and how an artifact was produced. C2PA (Content Credentials) frames provenance as a way to record how content was created and changed over time. in-toto chains together multiple parties' actions in a verifiable supply chain. Mosaic is adapting these well-established patterns to business-number artifacts — not inventing cryptographic provenance from scratch.

### Primitive 5: Steganographic watermarking on derived numeric data

**Critical guardrail — read before implementing:**

> **Watermarking may ONLY be applied to derived export artifacts or benchmark/cartridge datasets where an explicit tolerance/materiality policy permits it.**
>
> **Watermarking MUST NEVER:**
> - Mutate authoritative cube state
> - Modify canonical inputs
> - Alter signed audit logs
> - Touch financial reporting values used in compliance reporting
> - Modify any number that a human decision-maker relies on as authoritative
>
> A three-cent change on `$11,500.00` may be acceptable in a forensic export copy. It is never acceptable in a source-of-truth planning cube or a regulatory submission.

**What watermarking enables (on permissible targets):**
- Detection of leaked data even when sidecar files (provenance.json, signature.sig) are stripped
- Watermark can survive copy-paste, export to Excel, conversion to PDF
- Forensic proof of leak source even from partial data
- Anti-piracy protection for cartridge benchmark libraries (the most natural application)

**How it works (academic reference: Agrawal & Kiernan, "Watermarking Relational Databases," VLDB 2002):**
- Cell value of `$11,500.00` becomes `$11,500.03` (3-unit adjustment in a low-order digit)
- Adjustment is well within accounting materiality thresholds for forensic copies
- The pattern of modifications across many cells encodes a cryptographic identifier
- A verifier with the source's public key checks the statistical pattern of low-order digits

**Implementation requirements:**
- Minimum ~50-100 cells for reliable encoding
- Single-value extraction cannot be watermarked
- Requires dedicated cryptographic review before shipping — subtle bugs break the property entirely
- Aggressive rounding by adversaries defeats it (but also destroys data usefulness)
- **Detection only** — does not prevent exfiltration

**Most natural first application:** cartridge benchmark libraries. These are aggregate statistical datasets (percentiles, industry averages) where small adjustments are within the natural variability of the data and the forensic copy use case is clean.

---

## Part 4: Deployment-shape evolution

Grout's invariant stays constant (Mosaic data is verifiably intact) but its mechanisms evolve with the deployment shape.

### Shapes 1-2: File-based (current state)

The `.mosaic/` directory is the database. Grout's job is narrow:
- Hash-chain the JSONL logs
- Canary files that legitimate ops never touch
- Signed integrity manifest outside the workspace directory
- Workspace-scoped signing keys in `.mosaic/grout-keys/` with restricted permissions

**Threat model at this shape:** single-user; primary threats are accidental corruption, ransomware, data loss. Grout provides detection and recovery via signed exports.

### Shape 4: Service daemon (Phase 8)

The daemon introduces real persistence — SQLite for ledger/metadata, snapshot files, a process boundary.

- **Append-only audit tables** — every write goes through an audit row; each row signed; retroactive sanitization detectable
- **Hash-chained ledger rows** — `row_hash = sha256(prev_hash || content)`; missing/reordered rows break the chain
- **Canary rows** — known-value rows in key tables; checked on startup; missing canary = unauthorized table access
- **Daemon adopts workspace's keys** — continues the workspace-portable key model

**Threat model at this shape:** multi-user; adds credential abuse, privilege escalation, malicious users with legitimate access.

### Shape 6: Cloud service (Phase 9-10)

Multi-tenant, managed object storage, org/workspace architecture from ADR-0026 fully realized.

- **Customer-controlled signing keys** — integrity manifests signed with a key in the customer's KMS (AWS KMS, Azure Key Vault, GCP KMS, or HSM); Mosaic doesn't hold the key; even vendor compromise can't forge integrity proofs
- **Cross-org grant integrity** — each ADR-0026 capability grant is signed; tampering produces a detectable hash break before the grant is honored
- **Export-and-verify** — `mc grout export` generates verifiable archives; customer verifies without any Mosaic infrastructure

**Threat model at this shape:** multi-tenant; adds cross-tenant access, vendor compromise, insider threats at the vendor.

### Beyond Shape 6: Cartridge marketplace

At the marketplace stage (Phase 10+):
- **Cartridge watermarking** — vendor benchmark libraries carry watermarks; pirated cartridges detectable
- **Cross-instance trust registry** — optional registry where orgs publish workspace public keys
- **Federated provenance** — data flowing through multiple Mosaic instances accumulates a verifiable chain

---

## Part 5: How Grout maps to the org/workspace architecture

The org/workspace model (ADR-0026) introduces trust boundaries: Org → Workspace → Managed Org. Grout enforces these boundaries at the application layer, independently of the infrastructure layer.

### The capability-grant integrity story

Per ADR-0026, inter-org relationships use capability-based grants (`use`, `view`, `fork`, `contribute`, `admin`). Grout makes these grants cryptographically trustworthy:

- Each capability grant is signed by the grantor's key
- Grants are recorded in an append-only, hash-chained grant log
- Verification happens at every cross-boundary access, not just at auth time

**The attack this defends:** An attacker gets database write access and modifies a grant row to elevate their access. Grout's signed grant chain means the modification produces a hash break, detectable before the elevated grant is honored. Infrastructure IAM is the first defense; Grout is the last.

### Multi-tier organizational structures

- **Parent org viewing child orgs:** parent's view grant is signed; child verifies; child's data isn't exposed beyond the granted scope
- **Agency managing client:** management grant is signed; client retains ability to revoke; agency cannot fabricate access
- **Vendor distributing cartridge:** cartridge signed by vendor; partner installs with use grant; cartridge integrity verified on every load

Without Grout, these relationships rely entirely on infrastructure controls. With Grout, they have application-layer enforcement that survives infrastructure compromise.

---

## Part 6: Grout as cryptographic-provenance product

### The strategic positioning

> Mosaic is not just a deterministic interpretation engine. It is a **deterministic interpretation engine with cryptographic provenance**. Every interpretation is logged with verifiable evidence. Every export carries cryptographic proof of origin. Every cross-instance data flow is verifiable end-to-end. When data leaks, the source is identifiable. When infrastructure fails, the data is recoverable. When integrity is questioned, the answer is mathematical, not operational.

This positioning matters for:
- **Compliance-sensitive industries** (finance, healthcare, legal, government) where audit trails must be cryptographically verifiable
- **Cross-organizational data sharing** (agency-client, parent-subsidiary, vendor-partner) where trust boundaries need application-layer enforcement
- **AI training data provenance** (emerging regulatory concern) where data lineage from source to model is becoming required
- **Cartridge marketplace** where vendor IP needs anti-piracy protection
- **Adversarial environments** where infrastructure compromise is plausible

### The analogues in adjacent fields

Mosaic would be among the first planning/analytics tools to bring this level of cryptographic provenance to business numbers. The closest analogues are:

- **Software supply chain** (Sigstore, in-toto, SLSA): provenance for build artifacts. Mosaic adapts the same pattern to business-number artifacts.
- **Content authentication** (C2PA, Adobe Content Credentials): provenance for media. Mosaic is the C2PA equivalent for financial model outputs.
- **Distributed ledgers**: append-only, hash-chained records of state changes. Mosaic's ledger applies the same primitives to interpretation events rather than financial transactions.

Grout does not claim to be inventing these cryptographic patterns. It is applying them — with appropriate domain adaptations — to a new category of artifact (business-number analysis).

---

## Part 7: Hashing and signing — design direction

### Proposed cryptographic primitives (subject to review)

**Content hashing:** SHA-256. Universal, fast, well-supported. Used for:
- Cube model file integrity (model.yaml, fixtures.csv)
- Tessera recipe integrity
- Narrative template integrity
- Benchmark library entry integrity
- Export bundle content hashes
- Hash-chain construction in append-only logs

**Signing:** Ed25519. Strong, fast, short signatures. Used for:
- Export bundle signatures
- Grant record signatures
- Provenance record signatures
- Cartridge watermark verification

**Why SHA-256 and Ed25519:** These are the same choices made by SLSA, Sigstore, and most software supply chain tooling. They have no known practical attacks, are supported by all major cryptographic libraries, and are the expected baseline for this kind of work.

**Review requirement (binding):** Before any signing or watermarking primitive ships, engage external cryptographic review. These are easy to implement incorrectly in ways that destroy the security property entirely. SHA-256 and Ed25519 are the proposed starting point, not locked-in commitments — a review may surface FIPS requirements, HSM compatibility needs, or algorithm aging concerns that justify adjustment.

### Where encryption belongs (and doesn't)

**Encryption at rest:** Operator concern. Mosaic doesn't manage filesystem or database encryption.

**Encryption in transit:** TLS via standard libraries. Mosaic uses TLS; doesn't manage TLS configuration.

**Encryption of cube cells:** No. Cube cells are f64 values; encryption breaks all cube semantics. Confidentiality at the cell level is an access-control problem, not a Grout problem.

**Encryption of signing keys at rest:** Yes, in daemon and cloud shapes. Keys stored on disk should be encrypted with a passphrase or wrapped with a KMS key.

---

## Part 8: Implementation sequencing

### Prerequisites before any Grout work (binding)

**Secure development baseline first.** `docs/security/mosaic-security-posture.md` must be in place before Grout primitives ship. This includes cargo audit, cargo deny, dependency policy, upload/zip hardening, parser fuzzing, and responsible disclosure. Grout is not a shortcut to security — it's an advanced capability that builds on a secure foundation.

**Cryptographic review.** Watermarking and signature implementation are easy to do wrong. Subtle bugs break the entire property. Before any Grout primitive ships, engage someone with cryptographic expertise. This is not where to save money.

**Threat model validation.** When implementation begins, the threat model gets re-validated. Tabletop exercises ("if an attacker did X, what would Grout detect?") should be standard practice.

**Performance impact assessment.** Hashing every row, signing every export, verifying every grant — these have costs. Benchmark before shipping.

**Backward compatibility plan.** Existing workspaces without Grout markers need a migration story. New Mosaic versions shouldn't break old workspaces.

### What to commit to NOW (architectural protections)

These are small, cheap commitments that protect future Grout work without requiring Grout to ship:

**1. Append-only structures designed for future hash-chaining.** Stable row identity, append-only semantics, reserved space for hash fields. Applies to: Tessera audit log, post-hoc write log, interpretation ledger (Phase 7A.2), grant records (Phase 4C).

**2. Cell provenance reserves `source_instance_id` slot.** Per ADR-0025 Rule 4, every cell has provenance. Reserve space for `source_instance_id` field even if not populated yet.

**3. Threat model documented.** `docs/security/mosaic-security-posture.md` captures the baseline threat model. It evolves as Mosaic grows.

### Not building now

Don't build any of this yet. The current work is Phase 6D + 6E + 7A.6. Grout is Phase 8.5 or Phase 9 territory.

### Phased implementation when the time comes

**Phase 8.5 (after daemon ships): Foundational Grout**
- Hash chains on existing append-only structures
- Workspace-scoped signing keys
- Signed export bundles
- Basic canary support (canary records, canary workspaces)
- `mc grout verify` command for export verification

**Phase 9 (with cloud service): Customer-controlled keys**
- KMS integration for customer key management
- Cross-org grant integrity
- Verified imports
- Canary grants

**Phase 10 (with cartridge marketplace): Provenance and watermarking**
- Cartridge benchmark watermarking (with cryptographic review and explicit tolerance policy)
- Cross-instance trust registry
- Federated provenance chains

---

## Part 9: Honest assessment of limitations

### Watermarking limits
- Requires sufficient data volume (~50+ cells for reliable encoding)
- Single-value extraction cannot be watermarked
- Aggressive rounding by adversaries defeats it (but also destroys data usefulness)
- Cryptographic review essential; subtle bugs break the property
- Detection only — does not prevent exfiltration
- Must NEVER be applied to authoritative cube state (see guardrail in Primitive 5)

### Provenance limits
- Only works between Mosaic instances; data exported to Excel loses markers unless watermarks are also present
- Requires trust model for cross-org key verification
- Determined attackers can strip provenance records (watermarks may still detect this case)
- Adds metadata overhead to all data flows

### Canary limits
- Detect reconnaissance, not targeted access
- Sophisticated attackers who read the code can recognize and avoid hardcoded patterns — use per-deployment generated canaries
- Require operational alerting infrastructure to be useful
- Generate noise if not calibrated carefully

### Hash chain limits
- Detect tampering, don't prevent it
- Lost or corrupted entries are detectable but not recoverable from the chain alone
- Don't protect against deletion of the entire chain

### Signed export limits
- Verify integrity at export time, not real-time
- Don't help if attacker compromises before export
- Require key management infrastructure
- Storage cost for signature data

**The honest pitch:** Grout makes integrity violations detectable and data leakage attributable. It does not make Mosaic unbreakable. It is one layer in a defense-in-depth strategy, not a complete security solution.

---

## Part 10: Strategic positioning this enables

**Compliance positioning:**
> "Every interpretation in Mosaic is cryptographically logged. Every data flow is verifiable. Every export is signed. When auditors ask 'how do you prove this number is correct?' the answer is mathematical, not operational."

Meaningful for: Financial services (SOX, regulatory reporting), Healthcare (HIPAA), Legal (e-discovery), Government (FedRAMP, FISMA), AI governance (emerging training data provenance regulations).

**Anti-piracy positioning (cartridge marketplace):**
> "Cartridges in Mosaic carry cryptographic watermarks on benchmark datasets. Pirated cartridges are detectable. Vendor IP is protected at the data layer."

**Trust-boundary positioning:**
> "When you give someone access to a Mosaic workspace, the access is cryptographically scoped. Privilege escalation through database manipulation is detectable before it's honored."

**Disaster recovery positioning:**
> "If Mosaic the infrastructure is compromised or gone, your historical data integrity is preserved in signed exports. The canonical record is the verifiable archive, not the live system."

---

## Action items

### Immediate (capture, not build)

1. ✅ **This document** — design capture for Grout
2. **Create `docs/security/mosaic-security-posture.md`** — secure-development baseline (in progress as of this integration pass)
3. **Preserve append-only structure properties** — when implementing ledger (Phase 7A.2) and grant records (Phase 4C), design with hash-chainability in mind: stable row identity, append-only semantics, reserved hash fields

### When Phase 8 begins

4. Draft ADR for foundational Grout (hash chains, signed exports, basic canaries)
5. Engage cryptographic review before any primitive ships
6. Build threat model tabletop exercises into the phase

### Far future (Phase 9+)

7. Design cross-instance trust registry when marketplace use cases drive demand
8. Implement cartridge watermarking with cryptographic review, strict materiality policy, and forensic testing
9. Build customer-controlled key management when cloud service has paying customers

---

## Cross-links

- **ADR-0025** — kernel discipline; Grout is a deployment-shell concern per Decision 1
- **ADR-0026** — org/workspace/capability grants; Grout's signed grant chains enforce these boundaries
- **`docs/security/mosaic-security-posture.md`** — secure-development baseline; prerequisite to Grout implementation
- **`docs/strategy/mosaic-architecture-and-vision.md`** — strategic reference document
- **`docs/roadmap/MASTER_PHASE_PLAN.md`** — phase sequencing

### External references for further research

- **Hash chains and append-only logs:** Certificate Transparency (RFC 6962), Git's commit graph
- **Cryptographic provenance:** in-toto framework, SLSA (Supply chain Levels for Software Artifacts), Sigstore
- **Content authentication:** C2PA (Content Credentials), Adobe's Content Authenticity Initiative
- **Watermarking:** Agrawal & Kiernan, "Watermarking Relational Databases," VLDB 2002
- **Honeypots/honeytokens:** Thinkst Canary, Thinkst Canarytokens
- **Customer-controlled keys:** AWS KMS, Azure Key Vault, GCP KMS, HashiCorp Vault
- **Signature schemes:** Ed25519 (RFC 8032), Sigstore's keyless signing model
- **Secure development:** NIST SSDF (SP 800-218), RustSec advisory database, `cargo-audit`, `cargo-deny`

---

## Appendix A: One-paragraph summary for project owner

Grout is Mosaic's application-layer integrity architecture. Its job is to make Mosaic's data structures verifiably intact regardless of what happens to the surrounding infrastructure. It does this through five primitives: hash-chained append-only logs, canary records, signed exports, cryptographic provenance markers, and steganographic watermarking (on derived export artifacts only — never on authoritative cube state). Together, these enable detection of integrity violations, attribution of data leaks, recovery from infrastructure failures, and cross-instance trust verification. Grout does not prevent attacks (operators are responsible for infrastructure security) but it makes attacks detectable and data leaks attributable. The architecture is inspired by established patterns (CT, SLSA, C2PA) adapted to business-number artifacts. Implementation is Phase 8.5+ work; for now, the document captures design and small architectural commitments protect future implementation without requiring Grout to ship.

## Appendix B: One-sentence pitch

Mosaic is a planning engine where every interpretation is cryptographically logged, every data flow is verifiably attributed, and every export is auditable without depending on Mosaic infrastructure to verify it.

---

**End of Grout vision document. Update as primitives are designed and ADRs draft. Hand to next PM instance after compaction events alongside the architecture-and-vision document. The Grout ADR is the binding document when it drafts; this research note is the predecessor.**
