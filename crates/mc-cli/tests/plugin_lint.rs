//! Phase 4A: plugin sanity-lint over `mosaic-plugin/`.
//!
//! Asserts the load-bearing structural invariants:
//!
//! 1. `mosaic-plugin/.claude-plugin/plugin.json` exists and parses as JSON
//!    (lightweight check — looks for the expected top-level keys).
//! 2. `mosaic-plugin/.mcp.json` exists.
//! 3. Every `skills/<name>/SKILL.md` has YAML frontmatter with `name:`
//!    and `description:` keys. The frontmatter is enclosed in `---` markers.
//! 4. Every `agents/*.md` has YAML frontmatter with `name:` and
//!    `description:` keys.
//! 5. Every `commands/*.md` has YAML frontmatter with at least
//!    `description:`.
//! 6. **No provider-specific tags** anywhere under skills/, agents/,
//!    commands/: no `<anthropic_specific>`, no `[OpenAI:`, no
//!    `claude:` or `gpt:` literal-string markers.
//! 7. No code/script files under skills/, agents/, commands/ — only
//!    `.md`. Hooks/ and examples/adapters/ get a placeholder README;
//!    examples/models/ has the Acme YAML + CSV (allowlisted).
//! 8. **`marketing-mix` is the ONLY domain schema** under
//!    `skills/domain-schemas/`.
//! 9. **MC3008 is permanently retired:** the debugging skill MUST flag
//!    MC3008 as retired (positive control on the registry text).

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn plugin_root() -> PathBuf {
    workspace_root().join("mosaic-plugin")
}

fn read_to_string(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {p:?}: {e}"))
}

fn collect_md_files(dir: &Path, into: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, into);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            into.push(path);
        }
    }
}

fn frontmatter(text: &str) -> Option<&str> {
    let stripped = text.strip_prefix("---\n")?;
    let end = stripped.find("\n---")?;
    Some(&stripped[..end])
}

#[test]
fn plugin_json_exists_and_has_expected_keys() {
    let p = plugin_root().join(".claude-plugin").join("plugin.json");
    assert!(p.exists(), "missing manifest: {p:?}");
    let body = read_to_string(&p);
    for key in [
        r#""name""#,
        r#""version""#,
        r#""description""#,
        r#""commands""#,
        r#""agents""#,
    ] {
        assert!(body.contains(key), "plugin.json missing key {key}");
    }
    // Must NOT carry the ADR-0008-sketch keys that Claude Code's loader doesn't read.
    for stale in [
        r#""displayName""#,
        r#""skills":"./skills""#,
        r#""mcpServers":"./.mcp.json""#,
    ] {
        assert!(
            !body.contains(stale),
            "plugin.json carries stale ADR-0008 sketch key {stale}"
        );
    }
}

#[test]
fn mcp_json_exists() {
    let p = plugin_root().join(".mcp.json");
    assert!(p.exists(), "missing .mcp.json: {p:?}");
    let body = read_to_string(&p);
    assert!(body.contains(r#""mcpServers""#));
    assert!(body.contains(r#""mosaic""#));
    assert!(body.contains(r#""mcp""#));
}

#[test]
fn skills_have_valid_frontmatter() {
    let mut files = Vec::new();
    collect_md_files(&plugin_root().join("skills"), &mut files);
    let skills: Vec<_> = files
        .iter()
        .filter(|p| p.file_name().and_then(|f| f.to_str()) == Some("SKILL.md"))
        .collect();
    assert!(!skills.is_empty(), "no SKILL.md files found");
    for path in &skills {
        let body = read_to_string(path);
        let fm = frontmatter(&body)
            .unwrap_or_else(|| panic!("{path:?}: missing or malformed frontmatter"));
        assert!(
            fm.contains("name:"),
            "{path:?}: skill frontmatter missing 'name:'"
        );
        assert!(
            fm.contains("description:"),
            "{path:?}: skill frontmatter missing 'description:'"
        );
        let body_after = body.splitn(3, "---").nth(2).unwrap_or_default().trim();
        assert!(
            body_after.len() > 200,
            "{path:?}: skill body suspiciously short ({} chars) — skills should not be stubs",
            body_after.len()
        );
    }
}

#[test]
fn agents_have_valid_frontmatter() {
    let mut files = Vec::new();
    collect_md_files(&plugin_root().join("agents"), &mut files);
    assert!(!files.is_empty(), "no agent files found");
    for path in &files {
        let body = read_to_string(path);
        let fm = frontmatter(&body).unwrap_or_else(|| panic!("{path:?}: missing frontmatter"));
        assert!(fm.contains("name:"), "{path:?}: agent missing 'name:'");
        assert!(
            fm.contains("description:"),
            "{path:?}: agent missing 'description:'"
        );
    }
}

#[test]
fn commands_have_description_in_frontmatter() {
    let mut files = Vec::new();
    collect_md_files(&plugin_root().join("commands"), &mut files);
    assert!(!files.is_empty(), "no command files found");
    for path in &files {
        let body = read_to_string(path);
        let fm = frontmatter(&body).unwrap_or_else(|| panic!("{path:?}: missing frontmatter"));
        assert!(
            fm.contains("description:"),
            "{path:?}: command missing 'description:'"
        );
    }
}

#[test]
fn no_provider_specific_tags_in_plugin_content() {
    let dirs = [
        plugin_root().join("skills"),
        plugin_root().join("agents"),
        plugin_root().join("commands"),
    ];
    let banned_substrings = [
        "<anthropic_specific",
        "[OpenAI:",
        "[Anthropic:",
        "[Claude:",
        "[GPT:",
    ];
    let mut files = Vec::new();
    for d in &dirs {
        collect_md_files(d, &mut files);
    }
    for path in &files {
        let body = read_to_string(path);
        for banned in &banned_substrings {
            assert!(
                !body.contains(banned),
                "{path:?}: contains provider-specific tag {banned:?}"
            );
        }
    }
}

#[test]
fn skills_agents_commands_are_markdown_only() {
    // No code, no scripts, no compiled artifacts under skills/, agents/, commands/.
    let dirs = ["skills", "agents", "commands"];
    let allowed_extensions = ["md"];
    for dir in &dirs {
        let mut stack = vec![plugin_root().join(dir)];
        while let Some(d) = stack.pop() {
            if !d.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&d).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                assert!(
                    allowed_extensions.contains(&ext),
                    "{path:?}: only markdown files allowed under {dir}/, got .{ext}"
                );
            }
        }
    }
}

#[test]
fn marketing_mix_is_only_domain_schema() {
    let domain_dir = plugin_root().join("skills").join("domain-schemas");
    let entries: Vec<String> = std::fs::read_dir(&domain_dir)
        .unwrap_or_else(|e| panic!("read_dir {domain_dir:?}: {e}"))
        .filter_map(|r| r.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        entries,
        vec!["marketing-mix"],
        "Phase 4A ships only the marketing-mix domain (per ADR-0008 amendment F)"
    );
}

#[test]
fn debugging_skill_documents_mc3008_retired() {
    let path = plugin_root()
        .join("skills")
        .join("debugging")
        .join("SKILL.md");
    let body = read_to_string(&path);
    // Positive control: the skill MUST mention MC3008 as retired so the
    // LLM never reintroduces it.
    assert!(
        body.contains("MC3008"),
        "debugging skill must reference MC3008"
    );
    let around_mc3008 = body
        .lines()
        .filter(|l| l.contains("MC3008"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        around_mc3008.to_lowercase().contains("retired")
            || around_mc3008.to_lowercase().contains("retire"),
        "debugging skill must flag MC3008 as retired"
    );
}

#[test]
fn examples_adapters_is_phase_4b_placeholder() {
    let p = plugin_root()
        .join("examples")
        .join("adapters")
        .join("README.md");
    assert!(
        p.exists(),
        "examples/adapters/ must contain a Phase 4B placeholder"
    );
    let body = read_to_string(&p);
    assert!(
        body.to_lowercase().contains("phase 4b"),
        "adapters README must reference Phase 4B"
    );
    // Confirm there's no actual adapter code in the directory.
    let dir = p.parent().unwrap();
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            assert!(
                ext == "md",
                "examples/adapters/ must be empty except README.md, found {path:?}"
            );
        }
    }
}
