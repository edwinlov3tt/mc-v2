# Hooks — placeholder

Phase 4A ships **no hooks**. Two were originally proposed:

- `pre-commit-lint.json` — fire `mc model lint` on YAML changes before commit.
- `post-edit-validate.json` — fire `mc model validate` after a YAML edit.

Per [Phase 4A handoff §I](../../docs/handoffs/phase-4a-handoff.md), hooks "matter less than skills/agents/commands." The current Claude Code hook-spec format could not be verified from inside the build session (no live runtime to check shape against), and the headline acceptance gate ("fresh Claude Code instance produces working YAML from `/mosaic-init marketing-mix`") does not require hooks.

The Phase 4A completion report's "follow-up candidates" section names this as a candidate for a Phase 4A.1 amendment once the canonical hook-spec format is verified against a live Claude Code install.

When that amendment lands, this directory grows the two hook files; the `mosaic` plugin manifest may need a top-level `hooks` reference depending on whether the canonical format auto-discovers `hooks/*.json` or requires explicit listing.
