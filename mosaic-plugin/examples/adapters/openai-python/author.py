"""Mosaic openai-python reference adapter (Phase 4B).

Reads the Mosaic plugin's skills/agents/commands/example content, builds a
single system prompt, calls the OpenAI API with a natural-language prompt,
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

from openai import OpenAI

# Verified current via web_search 2026-05-03; confirm at execution time.
MODEL = "gpt-5.5"
DEFAULT_MAX_ITER = 5

RESPONSE_FORMAT_INSTRUCTION = (
    "Respond with the complete Mosaic YAML model in a single fenced block "
    "(```yaml ... ```) with no surrounding prose, commentary, or "
    "explanation. The validate/lint/test pipeline runs against the YAML "
    "directly; any text outside the fence will be discarded."
)
YAML_FENCE = re.compile(r"```(?:yaml|yml)?\s*\n(.*?)```",
                        re.DOTALL | re.IGNORECASE)


def find_plugin_root() -> Path:
    # mosaic-plugin/examples/adapters/openai-python/author.py -> parents[3].
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


def call_provider(client: OpenAI, system_prompt: str,
                  messages: list[dict]) -> str:
    inputs = [{"role": "system", "content": system_prompt}, *messages]
    response = client.responses.create(model=MODEL, input=inputs)
    return response.output_text


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

    client = OpenAI()
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


def main() -> int:
    p = argparse.ArgumentParser(
        description="Mosaic OpenAI Python reference adapter.",
    )
    p.add_argument("prompt", help="Natural-language model description.")
    p.add_argument("--output", default="output.yaml",
                   help="Output path for the converged YAML.")
    p.add_argument("--max-iterations", type=int, default=DEFAULT_MAX_ITER,
                   help=f"Iteration cap (default: {DEFAULT_MAX_ITER}).")
    p.add_argument("--strict", action="store_true",
                   help="Treat MC3xxx lint warnings as blocking.")
    args = p.parse_args()

    yaml_text, status, iters, env = author(
        args.prompt, max_iter=args.max_iterations, strict=args.strict,
    )
    if status == "converged":
        Path(args.output).write_text(yaml_text, encoding="utf-8")
        print(f"\n[mosaic] Converged in {iters} iteration(s). "
              f"YAML written to {args.output}")
        return 0
    failed_path = Path(args.output).with_suffix(".failed.yaml")
    failed_path.write_text(yaml_text, encoding="utf-8")
    print(f"\n[mosaic] Did NOT converge in {iters} iteration(s). "
          f"Last YAML written to {failed_path}.")
    print("[mosaic] Last diagnostic envelope:")
    print(json.dumps(env, indent=2))
    return 2


if __name__ == "__main__":
    sys.exit(main())
