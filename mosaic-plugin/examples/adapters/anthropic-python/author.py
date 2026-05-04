"""Mosaic anthropic-python reference adapter (Phase 4B).

Reads the Mosaic plugin's skills/agents/commands/example content, builds a
single system prompt, calls the Anthropic API with a natural-language prompt,
and iterates against `mc model validate / lint / test` until convergence
(default 5 iterations). Reference adapter, not a production framework. Per
ADR-0008 amendments A + G: no async, no streaming, no concurrency, no retries
beyond SDK defaults, no rate limiting, no telemetry.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import anthropic

# Verified current via web_search 2026-05-03; confirm at execution time.
MODEL = "claude-opus-4-7"
# 16000 tokens is enough for a 350-line marketing-mix YAML with inline
# canonical_inputs; 8000 truncated mid-YAML on the canonical Q4-lift prompt.
MAX_TOKENS = 16000
DEFAULT_MAX_ITER = 5

RESPONSE_FORMAT_INSTRUCTION = (
    "Respond with the complete Mosaic YAML model in a single fenced block "
    "(```yaml ... ```) with no surrounding prose, commentary, or "
    "explanation. The validate/lint/test pipeline runs against the YAML "
    "directly; any text outside the fence will be discarded."
)
RECIPE_RESPONSE_FORMAT_INSTRUCTION = (
    "Respond with the complete Tessera recipe YAML in a single fenced block "
    "(```yaml ... ```) with no surrounding prose. The validation pipeline "
    "runs against the YAML directly; any text outside the fence will be "
    "discarded. The recipe MUST conform to the mc-recipe schema (version: 1, "
    "name, model, source, columns) and the six semantic rules from "
    "ADR-0010 Decision 7. Map only Input measures (never Derived). "
    "Default to wide format; the long-format `format:`/`long_format:` fields "
    "are 5A.1-pending and not accepted by the current schema."
)
YAML_FENCE = re.compile(r"```(?:yaml|yml)?\s*\n(.*?)```",
                        re.DOTALL | re.IGNORECASE)

# Phase 5A's six supported drivers (per ADR-0010 Decision 7).
SUPPORTED_DRIVERS = {"csv", "sqlite", "duckdb", "postgres",
                     "duckdb_postgres", "http_json"}
# Acme's Derived measures (per crates/mc-model/examples/acme.yaml). Phase 5A
# writes to Input measures only; mapping any of these fires MC5018.
ACME_DERIVED_MEASURES = {"Clicks", "Leads", "Customers", "Revenue",
                         "Gross_Profit"}


def find_plugin_root() -> Path:
    # mosaic-plugin/examples/adapters/anthropic-python/author.py -> parents[3].
    root = Path(__file__).resolve().parents[3]
    if not (root / ".claude-plugin" / "plugin.json").exists():
        raise SystemExit(f"Could not locate Mosaic plugin root at {root}")
    return root


def load_plugin_content(root: Path) -> str:
    sections: list[str] = []
    for kind in ("skills", "agents", "commands"):
        for md_path in sorted((root / kind).rglob("*.md")):
            rel = md_path.relative_to(root).as_posix()
            sections.append(
                f"# {rel}\n\n{md_path.read_text(encoding='utf-8').rstrip()}\n"
            )
    acme = root / "examples" / "models" / "acme-marketing.yaml"
    if acme.exists():
        rel = acme.relative_to(root).as_posix()
        sections.append(
            f"# Reference example: {rel}\n\n```yaml\n"
            f"{acme.read_text(encoding='utf-8').rstrip()}\n```\n"
        )
    return "\n".join(sections)


def build_system_prompt(plugin_content: str) -> str:
    preamble = (
        "You are the Mosaic authoring assistant. The content below is the "
        "Mosaic Claude Code plugin's institutional knowledge — skills, "
        "agents, commands, and a canonical reference example. Use it to "
        "author Mosaic YAML models that pass `mc model validate / lint / "
        "test`."
    )
    return f"{preamble}\n\n{plugin_content}\n\n{RESPONSE_FORMAT_INSTRUCTION}\n"


def load_import_content(root: Path) -> str:
    """Load the import-focused subset of plugin content + worked recipe
    examples from `crates/mc-recipe/examples/recipes/`."""
    sections: list[str] = []
    # Import skills (recipe-format, csv-mapping, sql-mapping, api-mapping).
    for md in sorted((root / "skills" / "import").rglob("*.md")):
        rel = md.relative_to(root).as_posix()
        sections.append(f"# {rel}\n\n{md.read_text(encoding='utf-8').rstrip()}\n")
    # Domain schema (Acme dim/measure namespace + role information).
    for md in sorted((root / "skills" / "domain-schemas").rglob("*.md")):
        rel = md.relative_to(root).as_posix()
        sections.append(f"# {rel}\n\n{md.read_text(encoding='utf-8').rstrip()}\n")
    # Importer agent + import command.
    for rel_path in ("agents/mosaic-importer.md", "commands/mosaic-import.md"):
        p = root / rel_path
        if p.exists():
            sections.append(f"# {rel_path}\n\n{p.read_text(encoding='utf-8').rstrip()}\n")
    # Worked recipe examples (skip the intentionally-invalid ones — they
    # teach anti-patterns better through the skill content than as in-prompt
    # references).
    repo_root = root.parent
    examples_dir = repo_root / "crates" / "mc-recipe" / "examples" / "recipes"
    if examples_dir.exists():
        for ex in sorted(examples_dir.glob("*.recipe.yaml")):
            if "invalid" in ex.name:
                continue
            rel = ex.relative_to(repo_root).as_posix()
            sections.append(
                f"# Reference recipe: {rel}\n\n```yaml\n"
                f"{ex.read_text(encoding='utf-8').rstrip()}\n```\n"
            )
    return "\n".join(sections)


def build_import_system_prompt(import_content: str) -> str:
    preamble = (
        "You are the Mosaic Importer. Translate the user's natural-language "
        "data-source description into a single Tessera recipe (`*.recipe.yaml`) "
        "that conforms to the `mc-recipe` schema and the six semantic rules "
        "from ADR-0010 Decision 7. The content below is the import-focused "
        "subset of the Mosaic plugin (recipe schema, driver-specific mapping "
        "skills, the importer agent, the /mosaic-import command, the Acme "
        "domain schema, and worked reference recipes)."
    )
    return (f"{preamble}\n\n{import_content}\n\n"
            f"{RECIPE_RESPONSE_FORMAT_INSTRUCTION}\n")


def extract_yaml(response_text: str) -> str:
    # Prefer a complete ```yaml...``` fenced block.
    m = YAML_FENCE.search(response_text)
    if m:
        return m.group(1).strip() + "\n"
    # Fallback: opening fence with no closing fence (truncated response).
    m = re.search(r"```(?:yaml|yml)?\s*\n(.*)\Z", response_text,
                  re.DOTALL | re.IGNORECASE)
    if m:
        return m.group(1).strip() + "\n"
    # No fence at all — treat the whole response as YAML.
    return response_text.strip() + "\n"


def run_mc(args: list[str]) -> tuple[int, str, str]:
    try:
        r = subprocess.run(["mc", *args], capture_output=True, text=True,
                           check=False)
    except FileNotFoundError as exc:
        raise SystemExit("`mc` not found on PATH. Install via "
                         "`cargo install --path crates/mc-cli`.") from exc
    return r.returncode, r.stdout, r.stderr


def parse_envelope(stdout: str) -> dict:
    if not stdout.strip():
        return {"schema_version": "1.0", "diagnostics": []}
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        return {"schema_version": "1.0", "diagnostics": [],
                "_raw": stdout[:1000]}


def tessera_dry_run_available() -> bool:
    """Probe whether `mc tessera dry-run --help` exits cleanly. Picks the
    machine-validation path when true, the structural-self-validation path
    when false."""
    try:
        r = subprocess.run(["mc", "tessera", "dry-run", "--help"],
                           capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return False
    return r.returncode == 0


def structural_validate_recipe(yaml_text: str) -> list[dict]:
    """Best-effort structural validation when `mc tessera dry-run` isn't
    available. Returns diagnostics in the same envelope shape as
    `mc-recipe`'s validator. Without `pyyaml` the parse step is regex-driven;
    this is the documented fallback (see Phase 5B handoff §6 + the
    `/mosaic-import` command)."""
    diags: list[dict] = []
    if "\t" in yaml_text:
        diags.append({"code": "MC5001", "severity": "error", "path": "/",
                      "message": "tab indentation is not valid YAML"})
    for field in ("version", "name", "model", "source", "columns"):
        if not re.search(rf"(?m)^{field}\s*:", yaml_text):
            diags.append({"code": "MC5007", "severity": "error",
                          "path": f"/{field}",
                          "message": f"required field `{field}` is missing"})
    if re.search(r"(?m)^version\s*:", yaml_text) \
            and not re.search(r"(?m)^version\s*:\s*1\b", yaml_text):
        diags.append({"code": "MC5012", "severity": "error",
                      "path": "/version",
                      "message": "version must be 1 (Phase 5A pin)"})
    m = re.search(r"(?m)^\s*driver\s*:\s*(\w+)", yaml_text)
    if not m:
        diags.append({"code": "MC5007", "severity": "error",
                      "path": "/source/driver",
                      "message": "source.driver is missing"})
    elif m.group(1) not in SUPPORTED_DRIVERS:
        diags.append({"code": "MC5002", "severity": "error",
                      "path": "/source/driver",
                      "message": f"unknown driver '{m.group(1)}'; must be "
                                 f"one of {sorted(SUPPORTED_DRIVERS)}"})
    if (re.search(r"(?m)^\s*query\s*:", yaml_text)
            and re.search(r"(?m)^\s*table\s*:", yaml_text)):
        diags.append({"code": "MC5003", "severity": "error",
                      "path": "/source",
                      "message": "`query:` and `table:` are mutually "
                                 "exclusive — pick one"})
    for measure in ACME_DERIVED_MEASURES:
        if re.search(rf"measure\s*:\s*{measure}\b", yaml_text):
            diags.append({"code": "MC5018", "severity": "error",
                          "path": "/columns/?",
                          "message": f"measure `{measure}` is Derived in "
                                     "Acme; Phase 5A writes Inputs only "
                                     "(map Spend/CPC/CVR/Close_Rate/AOV/"
                                     "COGS_Rate instead)"})
    if re.search(r"(?m)^\s*format\s*:\s*long\b", yaml_text):
        diags.append({"code": "MC5001", "severity": "error",
                      "path": "/source/format",
                      "message": "long-format support is Phase 5A.1; the "
                                 "current mc-recipe schema rejects "
                                 "`format: long` — emit a wide-format "
                                 "recipe instead"})
    return diags


def diagnostics_by_severity(env: dict, severity: str) -> list[dict]:
    # Case-insensitive: the actual envelope uses PascalCase ("Error",
    # "Warning") even though `skills/debugging/SKILL.md` documents
    # lowercase. The plugin doc is a Phase 4A.1 follow-up; the adapter
    # tolerates either casing today.
    target = severity.lower()
    return [d for d in env.get("diagnostics", [])
            if str(d.get("severity", "")).lower() == target]


def failed_goldens(env: dict) -> list[dict]:
    return [g for g in env.get("goldens", [])
            if str(g.get("status", "")).lower() != "pass"]


def format_feedback(stage: str, diagnostics: list[dict],
                    failures: list[dict]) -> str:
    lines: list[str] = []
    if diagnostics:
        lines.append(f"`mc model {stage}` reported {len(diagnostics)} "
                     "blocking diagnostic(s):\n")
        for i, d in enumerate(diagnostics, 1):
            path = d.get("path", {})
            ptr = ((path.get("yaml_pointer") or path.get("model_path"))
                   if isinstance(path, dict) else str(path)) or "(unknown)"
            lines.append(f"[{i}] {d.get('code')} ({d.get('severity')}) "
                         f"at {ptr}:")
            lines.append(f"    {d.get('message', '')}")
            if d.get("suggestion"):
                lines.append(f"    Suggested fix: {d['suggestion']}")
            lines.append("")
    if failures:
        lines.append(f"`mc model test` reported {len(failures)} "
                     "failing/erroring golden(s):\n")
        for i, g in enumerate(failures, 1):
            lines.append(f"[{i}] {g.get('name')} ({g.get('status')}):")
            for f in ("expected", "actual", "delta", "epsilon", "note"):
                if f in g:
                    lines.append(f"    {f}: {g[f]}")
            lines.append("")
    lines.append("Please respond with a corrected YAML — full file, in a "
                 "single ```yaml fenced block, no surrounding prose.")
    return "\n".join(lines)


def call_provider(client: anthropic.Anthropic, system_prompt: str,
                  messages: list[dict]) -> str:
    return client.messages.create(
        model=MODEL, max_tokens=MAX_TOKENS, system=system_prompt,
        messages=messages,
    ).content[0].text


def re_request(client, system_prompt: str, messages: list[dict],
               feedback: str) -> str:
    messages.append({"role": "user", "content": feedback})
    response_text = call_provider(client, system_prompt, messages)
    messages.append({"role": "assistant", "content": response_text})
    return extract_yaml(response_text)


def author(user_prompt: str, max_iter: int = DEFAULT_MAX_ITER,
           strict: bool = False) -> tuple[str, str, int, dict]:
    """Returns (yaml_text, status, iterations_used, last_envelope)."""
    plugin_root = find_plugin_root()
    print(f"[mosaic] plugin root: {plugin_root}", file=sys.stderr)
    system_prompt = build_system_prompt(load_plugin_content(plugin_root))
    print(f"[mosaic] system prompt: {len(system_prompt):,} chars",
          file=sys.stderr)

    client = anthropic.Anthropic()
    messages: list[dict] = [{"role": "user", "content": user_prompt}]
    print(f"[mosaic] calling {MODEL} (initial draft)...", file=sys.stderr)
    response_text = call_provider(client, system_prompt, messages)
    candidate = extract_yaml(response_text)
    messages.append({"role": "assistant", "content": response_text})

    last_env: dict = {}
    for attempt in range(1, max_iter + 1):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml",
                                         delete=False, encoding="utf-8") as t:
            t.write(candidate)
            tmp_path = t.name

        # Validate.
        _, out, _ = run_mc(["model", "validate", tmp_path, "--format", "json"])
        env = parse_envelope(out); last_env = env
        errs = diagnostics_by_severity(env, "error")
        if errs:
            print(f"[mosaic][iter {attempt}] validate: {len(errs)} error(s)",
                  file=sys.stderr)
            candidate = re_request(client, system_prompt, messages,
                                   format_feedback("validate", errs, []))
            continue

        # Lint (advisory unless --strict).
        _, out, _ = run_mc(["model", "lint", tmp_path, "--format", "json"])
        warns = diagnostics_by_severity(parse_envelope(out), "warning")
        if strict and warns:
            print(f"[mosaic][iter {attempt}] lint(--strict): {len(warns)} "
                  "warning(s)", file=sys.stderr)
            candidate = re_request(client, system_prompt, messages,
                                   format_feedback("lint", warns, []))
            continue

        # Test.
        _, out, _ = run_mc(["model", "test", tmp_path, "--format", "json"])
        test_env = parse_envelope(out); last_env = test_env
        test_errs = diagnostics_by_severity(test_env, "error")
        fails = failed_goldens(test_env)
        if test_errs or fails:
            print(f"[mosaic][iter {attempt}] test: {len(test_errs)} "
                  f"error(s) + {len(fails)} golden failure(s)",
                  file=sys.stderr)
            candidate = re_request(client, system_prompt, messages,
                                   format_feedback("test", test_errs, fails))
            continue

        print(f"[mosaic][iter {attempt}] converged: validate/lint/test all "
              "pass", file=sys.stderr)
        return candidate, "converged", attempt, test_env

    return candidate, "max_iterations", max_iter, last_env


def propose_recipe(user_prompt: str,
                   max_iter: int = DEFAULT_MAX_ITER
                   ) -> tuple[str, str, int, dict, str]:
    """Returns (yaml_text, status, iterations_used, last_envelope,
    validation_path). validation_path is "machine" if `mc tessera dry-run`
    was used, "structural" otherwise."""
    plugin_root = find_plugin_root()
    print(f"[mosaic] plugin root: {plugin_root}", file=sys.stderr)
    system_prompt = build_import_system_prompt(load_import_content(plugin_root))
    print(f"[mosaic] system prompt: {len(system_prompt):,} chars",
          file=sys.stderr)

    machine = tessera_dry_run_available()
    path_label = "machine" if machine else "structural"
    print(f"[mosaic] validation path: {path_label}", file=sys.stderr)

    client = anthropic.Anthropic()
    messages: list[dict] = [{"role": "user", "content": user_prompt}]
    print(f"[mosaic] calling {MODEL} (initial recipe draft)...",
          file=sys.stderr)
    response_text = call_provider(client, system_prompt, messages)
    candidate = extract_yaml(response_text)
    messages.append({"role": "assistant", "content": response_text})

    last_env: dict = {"schema_version": "1.0", "diagnostics": []}
    for attempt in range(1, max_iter + 1):
        if machine:
            with tempfile.NamedTemporaryFile(mode="w", suffix=".recipe.yaml",
                                             delete=False,
                                             encoding="utf-8") as t:
                t.write(candidate); tmp_path = t.name
            _, out, _ = run_mc(["tessera", "dry-run", tmp_path,
                                "--format", "json"])
            env = parse_envelope(out)
            errs = diagnostics_by_severity(env, "error")
        else:
            errs = structural_validate_recipe(candidate)
            env = {"schema_version": "1.0", "diagnostics": errs}
        last_env = env

        if not errs:
            print(f"[mosaic][iter {attempt}] recipe converged "
                  f"({path_label})", file=sys.stderr)
            return candidate, "converged", attempt, env, path_label

        print(f"[mosaic][iter {attempt}] {path_label} validation: "
              f"{len(errs)} error(s)", file=sys.stderr)
        candidate = re_request(client, system_prompt, messages,
                               format_feedback("dry-run", errs, []))

    return candidate, "max_iterations", max_iter, last_env, path_label


def main() -> int:
    p = argparse.ArgumentParser(
        description="Mosaic Anthropic Python reference adapter.",
    )
    p.add_argument("prompt", help="Natural-language description.")
    p.add_argument("--mode", choices=["author", "propose-recipe"],
                   default="author",
                   help="`author` (default) writes a Mosaic YAML model. "
                        "`propose-recipe` writes a Tessera recipe.")
    p.add_argument("--output", default=None,
                   help="Output path. Default: output.yaml (author mode) or "
                        "output.recipe.yaml (propose-recipe mode).")
    p.add_argument("--max-iterations", type=int, default=DEFAULT_MAX_ITER,
                   help=f"Iteration cap (default: {DEFAULT_MAX_ITER}).")
    p.add_argument("--strict", action="store_true",
                   help="(author mode) Treat MC3xxx lint warnings as "
                        "blocking. Ignored in propose-recipe mode.")
    args = p.parse_args()

    if args.mode == "propose-recipe":
        out_path = Path(args.output or "output.recipe.yaml")
        yaml_text, status, iters, env, path_label = propose_recipe(
            args.prompt, max_iter=args.max_iterations,
        )
        if status == "converged":
            out_path.write_text(yaml_text, encoding="utf-8")
            print(f"\n[mosaic] Recipe converged in {iters} iteration(s) "
                  f"({path_label} validation). Written to {out_path}")
            return 0
        failed_path = out_path.with_suffix(".failed.recipe.yaml")
        failed_path.write_text(yaml_text, encoding="utf-8")
        print(f"\n[mosaic] Recipe did NOT converge in {iters} iteration(s) "
              f"({path_label} validation). Last YAML at {failed_path}.")
        print("[mosaic] Last diagnostic envelope:")
        print(json.dumps(env, indent=2))
        return 2

    out_path = Path(args.output or "output.yaml")
    yaml_text, status, iters, env = author(
        args.prompt, max_iter=args.max_iterations, strict=args.strict,
    )
    if status == "converged":
        out_path.write_text(yaml_text, encoding="utf-8")
        print(f"\n[mosaic] Converged in {iters} iteration(s). "
              f"YAML written to {out_path}")
        return 0
    failed_path = out_path.with_suffix(".failed.yaml")
    failed_path.write_text(yaml_text, encoding="utf-8")
    print(f"\n[mosaic] Did NOT converge in {iters} iteration(s). "
          f"Last YAML written to {failed_path}.")
    print("[mosaic] Last diagnostic envelope:")
    print(json.dumps(env, indent=2))
    return 2


if __name__ == "__main__":
    sys.exit(main())
