# product/

Product framing from the pre-engine phase: the original PRD and the transfer inventory that catalogued the engine-shaped patterns from the predecessor project.

These files **drove the spec** in [`../specs/`](../specs/) but are not themselves contracts. They are kept for context — if you need to know why a particular feature is in or out of scope, the rationale chain often starts here, then runs through [`../external-conversations/`](../external-conversations/) (where the scope-discipline argument played out), and lands in the brief and [`../decisions/0001-phase-1-scope.md`](../decisions/0001-phase-1-scope.md).

## Contents

- [`MC-PRD.md`](./MC-PRD.md) — original product requirements document. Pre-engine. Drove the brief but is not a contract.
- [`transfer-inventory.md`](./transfer-inventory.md) — catalogued engine-shaped patterns from the predecessor project (claw-edge). Used as input to the brief; not a contract.

## How to think about these

- **Read for context, not as authority.** The brief and engine-semantics doc supersede everything in here.
- **Vision-level features in the PRD are not scope.** Phase 1 narrowed sharply (see [`../decisions/0001-phase-1-scope.md`](../decisions/0001-phase-1-scope.md)). Many PRD features are deferred to Phase 2+ or further.
- **The transfer inventory's patterns are guides, not specifications.** Where the brief differs from the inventory, the brief wins.

## What does NOT belong here

- Active phase work → [`../specs/`](../specs/), [`../reports/`](../reports/), [`../handoffs/`](../handoffs/).
- Decisions made about how to interpret the PRD → [`../decisions/`](../decisions/).
- New product framing for future phases — write that in a new brief in [`../specs/`](../specs/), not as a PRD update here. Keep this folder historical.
