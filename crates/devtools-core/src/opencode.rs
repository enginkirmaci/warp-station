//! The wand prompt builders and model cache. The runs themselves happen in
//! warp-agent over its door (see `warp.rs`); DevTools only assembles the
//! prompts from local git state and remembers the model catalog between
//! sessions.

use std::path::Path;
use std::time::Duration;

/// Hard cap for one wand run.
pub const RUN_CAP: Duration = Duration::from_secs(150);

// ---- prompt templates (user file → seed-once → built-in) ----------------------

/// Appended to every filled prompt: the run must answer from the given
/// input, not explore the repository on its own. Never part of the stored
/// template body — the files hold the raw template only.
const NO_TOOLS_SUFFIX: &str = "\n\nThe input above is complete — do not use any tools; reply with the final answer text directly.";

/// One user-editable prompt template: the overriding file under the
/// assistant folder, the placeholders the builder fills in, and the built-in
/// default body.
pub struct PromptTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub file_name: &'static str,
    pub placeholders: &'static [&'static str],
    pub default: &'static str,
}

/// Every wand prompt template in Settings-listed order — the single source
/// of truth the three prompt builders load from.
pub const PROMPT_TEMPLATES: &[PromptTemplate] = &[
    PromptTemplate {
        id: "commit-message",
        title: "Commit message",
        file_name: "commit-message.md",
        placeholders: &["{file_list}", "{diff}", "{context}"],
        default: r#"You are a git commit message generator. Given a diff (and optional context), output ONE commit message following Conventional Commits.

FORMAT:
<type>(<scope>): <subject>

<body>

<footer>

RULES:
- type: feat, fix, refactor, perf, test, docs, style, chore, build, ci
- scope: optional, lowercase, the affected module/service/package (infer from changed file paths)
- subject: imperative mood ("add", not "added"/"adds"), lowercase start, no trailing period, max 72 chars
- body: optional, wrap at 100 chars, explain WHAT changed and WHY (not how) — omit if the subject is fully self-explanatory
- footer: only if there are breaking changes ("BREAKING CHANGE: ...") or issue refs ("Closes #123") supplied in context
- If the diff touches multiple unrelated concerns, pick the dominant one for the subject and note the rest briefly in the body
- Never invent file names, ticket numbers, or reasoning not supported by the diff
- Output ONLY the commit message text — no markdown fences, no explanation, no quotes around it

INPUT:
Changed files: {file_list}

Diff:
{diff}

Additional context (ticket/issue, manual notes — may be empty):
{context}
"#,
    },
    PromptTemplate {
        id: "readme",
        title: "README",
        file_name: "readme.md",
        placeholders: &["{repo_name}", "{file_tree}", "{context}"],
        default: r###"You are a README.md generator. Given a repository's name and file tree (plus optional context), write a concise, professional README.md for the project.

RULES:
- Start with a single "# <project name>" heading and a one-paragraph description
- Infer the project's purpose from the file tree; never invent features, commands, badges, or links the tree does not support
- Include "## Installation" and "## Usage" sections with fenced code blocks kept generic
- Write clear, simple English; keep the whole document under ~120 lines
- Output ONLY the README markdown — no wrapping code fences, no explanation

INPUT:
Project name: {repo_name}

File tree:
{file_tree}

Additional context (may be empty):
{context}
"###,
    },
    PromptTemplate {
        id: "daily-summary-all",
        title: "Daily summary",
        file_name: "daily-summary-all.md",
        placeholders: &["{date}", "{repos}"],
        default: r##"You are a daily work-summary writer. Given several repositories' commits and still-uncommitted changes from a single day, write one short markdown report that tells the developer what they did that day across their projects and where each effort left off.

FORMAT:
# Work summary, <date>

## What was done
(one bullet per repository that had commits, or per coherent group of related commits; start each bullet with the repository name in bold)

## Work in progress
(one bullet per repository with uncommitted changes — what is half-done right now; omit the section when there are none)

## Where you left off
(2-3 sentences per active repository, or one paragraph covering the day when the efforts intertwine)

## Suggested next steps
(2-5 concrete bullets that follow from the state above)

RULES:
- Ground every statement in the commit subjects and the changed-file lists — never invent work, decisions, or outcomes
- Group related commits into one bullet when they clearly belong together; never quote subjects verbatim
- Keep the whole report under ~60 lines; no tables, no horizontal rules
- Output ONLY the markdown report — no wrapping code fences, no explanation

INPUT:
Day: {date}

Repository activity:
{repos}
"##,
    },
];

fn template_by_id(id: &str) -> Option<&'static PromptTemplate> {
    PROMPT_TEMPLATES.iter().find(|t| t.id == id)
}

/// The template's file under the assistant folder; None for unknown ids.
pub fn template_path(id: &str) -> Option<std::path::PathBuf> {
    template_by_id(id).map(|t| crate::paths::AppPaths::assistant_dir().join(t.file_name))
}

/// The stored raw template body; None for unknown ids or read failures.
pub fn read_template(id: &str) -> Option<String> {
    std::fs::read_to_string(template_path(id)?).ok()
}

/// Writes the raw template body (no suffix, no placeholder filling). The
/// assistant folder exists from seeding but is (re)created defensively.
pub fn write_template(id: &str, content: &str) -> Result<(), String> {
    let path = template_path(id).ok_or_else(|| format!("unknown template id: {id}"))?;
    std::fs::create_dir_all(crate::paths::AppPaths::assistant_dir())
        .and_then(|_| std::fs::write(&path, content))
        .map_err(|err| format!("{err}"))
}

/// Restores the built-in default body.
pub fn reset_template(id: &str) -> Result<(), String> {
    let default = template_by_id(id)
        .ok_or_else(|| format!("unknown template id: {id}"))?
        .default;
    write_template(id, default)
}

/// True when the stored body differs from the built-in default; a missing
/// file is not customized.
pub fn is_customized(id: &str) -> bool {
    template_by_id(id)
        .zip(read_template(id))
        .is_some_and(|(t, content)| content != t.default)
}

/// The seed-once read behind the builders: the user file when present, else
/// the default written out for next time.
fn load_template(id: &str) -> String {
    let Some(tpl) = template_by_id(id) else {
        return String::new();
    };
    let path = crate::paths::AppPaths::assistant_dir().join(tpl.file_name);
    if let Ok(text) = std::fs::read_to_string(&path) {
        return text;
    }
    let _ = std::fs::create_dir_all(crate::paths::AppPaths::assistant_dir());
    if std::fs::write(&path, tpl.default).is_ok() {
        return tpl.default.to_string();
    }
    tpl.default.to_string()
}

/// The commit-message wand's filled prompt (Conventional Commits generator).
pub fn commit_message_prompt(file_list: &str, diff: &str, context: &str) -> String {
    load_template("commit-message")
        .replace("{file_list}", file_list)
        .replace("{diff}", diff)
        .replace("{context}", context)
        + NO_TOOLS_SUFFIX
}

/// The README wand's filled prompt.
pub fn readme_prompt(repo_name: &str, file_tree: &str, context: &str) -> String {
    load_template("readme")
        .replace("{repo_name}", repo_name)
        .replace("{file_tree}", file_tree)
        .replace("{context}", context)
        + NO_TOOLS_SUFFIX
}

/// The daily-summary wand's filled prompt.
pub fn daily_summary_prompt(date_label: &str, repositories_block: &str) -> String {
    load_template("daily-summary-all")
        .replace("{date}", date_label)
        .replace("{repos}", repositories_block)
        + NO_TOOLS_SUFFIX
}

/// The README prompt's file tree: a recursive listing capped at
/// `max_lines`, skipping the noise directories.
pub fn file_tree(folder: &Path, max_lines: usize) -> String {
    fn walk(dir: &Path, prefix: &str, skip: bool, lines: &mut Vec<String>, max: usize) {
        if skip || lines.len() >= max {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut names: Vec<(String, bool)> = entries
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                (name, is_dir)
            })
            .filter(|(name, _)| {
                !name.starts_with('.')
                    && !matches!(
                        name.as_str(),
                        "node_modules" | "target" | "bin" | "obj" | "dist" | "build" | "packages"
                    )
            })
            .collect();
        names.sort();
        for (name, is_dir) in names {
            if lines.len() >= max {
                return;
            }
            lines.push(format!("{prefix}{name}{}", if is_dir { "/" } else { "" }));
            if is_dir {
                walk(&dir.join(&name), &format!("{prefix}  "), false, lines, max);
            }
        }
    }
    let mut lines = Vec::new();
    walk(folder, "", false, &mut lines, max_lines);
    lines.join("\n")
}

// ---- the daily-summary repositories block --------------------------------------

/// The summary prompt's repository activity block (12 repos, 20 commits, 80
/// files, 4000 chars per repo).
pub fn format_repositories_block(activities: &[(String, crate::model::GitDayActivity)]) -> String {
    const MAX_REPOS: usize = 12;
    const MAX_COMMITS: usize = 20;
    const MAX_FILES: usize = 80;
    const MAX_WORKING_CHARS: usize = 4000;

    let mut out = String::new();
    for (repo_name, activity) in activities.iter().take(MAX_REPOS) {
        out.push_str("## ");
        out.push_str(repo_name);
        if let Some(branch) = &activity.branch_name {
            out.push_str(&format!(" (branch {branch})"));
        }
        out.push_str("\nCommits:\n");
        if activity.commits.is_empty() {
            out.push_str("(no commits that day)\n");
        } else {
            for commit in activity.commits.iter().take(MAX_COMMITS) {
                let (additions, deletions): (u32, u32) = commit
                    .files
                    .iter()
                    .filter(|f| f.additions.is_some() || f.deletions.is_some())
                    .fold((0, 0), |(a, d), f| {
                        (a + f.additions.unwrap_or(0), d + f.deletions.unwrap_or(0))
                    });
                let counts = if commit.files.is_empty() {
                    String::new()
                } else {
                    format!(
                        " — {} file(s), +{additions}/−{deletions}",
                        commit.files.len()
                    )
                };
                out.push_str(&format!(
                    "- {} {} {}{}\n",
                    &commit.hash[..7.min(commit.hash.len())],
                    commit.when.format("%H:%M"),
                    commit.subject.replace('\n', " "),
                    counts,
                ));
            }
            if activity.commits.len() > MAX_COMMITS {
                out.push_str(&format!(
                    "… and {} more commits\n",
                    activity.commits.len() - MAX_COMMITS
                ));
            }
        }
        out.push_str("Uncommitted changes:\n");
        if activity.uncommitted_files.is_empty() {
            out.push_str("(working tree clean — nothing uncommitted)\n");
        } else {
            let mut working = String::new();
            for file in activity.uncommitted_files.iter().take(MAX_FILES) {
                let counts = match (file.additions, file.deletions) {
                    (Some(a), Some(d)) => format!(" (+{a}/−{d})"),
                    (Some(a), None) => format!(" (+{a})"),
                    (None, Some(d)) => format!(" (−{d})"),
                    (None, None) => String::new(),
                };
                working.push_str(&format!(
                    "- {}  {}{}\n",
                    file.display_status_code(),
                    file.path,
                    counts
                ));
                if working.len() > MAX_WORKING_CHARS {
                    working.push_str("… (truncated)\n");
                    break;
                }
            }
            if activity.uncommitted_files.len() > MAX_FILES {
                working.push_str(&format!(
                    "… and {} more files\n",
                    activity.uncommitted_files.len() - MAX_FILES
                ));
            }
            out.push_str(working.trim_end_matches('\n'));
            out.push('\n');
        }
        out.push('\n');
    }
    if activities.len() > MAX_REPOS {
        out.push_str(&format!(
            "(… and {} more active repositories omitted)\n",
            activities.len() - MAX_REPOS
        ));
    }
    out.trim_end().to_string()
}

// ---- model catalog (the door is the source; the data root keeps the cache) ---

fn models_cache_path() -> std::path::PathBuf {
    crate::paths::AppPaths::models_cache_file()
}

/// The cached catalog (no door round-trip); the configured default lands on
/// top when it is missing from the list.
pub fn cached_models(default_model: &str) -> Vec<String> {
    let mut models: Vec<String> = std::fs::read_to_string(models_cache_path())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    models.retain(|model| {
        let model = model.trim();
        !model.is_empty() && !model.starts_with('(')
    });
    merge_default(&mut models, default_model);
    models
}

/// Persists a fresh catalog from the door; only a non-empty result is
/// cached, so a transient failure never clobbers a good catalog.
pub fn store_models_cache(models: &[String], default_model: &str) {
    if models.is_empty() {
        return;
    }
    let mut merged: Vec<String> = models
        .iter()
        .filter_map(|model| {
            let model = model.trim();
            (!model.is_empty() && !model.starts_with('(')).then(|| model.to_string())
        })
        .collect();
    merged.dedup();
    merge_default(&mut merged, default_model);
    let path = models_cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&merged) {
        let _ = std::fs::write(&path, json);
    }
}

fn merge_default(models: &mut Vec<String>, default_model: &str) {
    let default = default_model.trim();
    if default.is_empty() {
        return;
    }
    match models.iter().position(|m| m.eq_ignore_ascii_case(default)) {
        Some(0) => {}
        Some(ix) => {
            let entry = models.remove(ix);
            models.insert(0, entry);
        }
        None => models.insert(0, default.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the two HOME-sensitive template tests against each other:
    /// the builder test must never read a customized body the write/reset
    /// test parked in a scratch HOME (and vice versa).
    static TEMPLATE_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn default_model_floats_to_top() {
        let mut models = vec!["a/x".to_string(), "b/y".to_string()];
        merge_default(&mut models, "b/y");
        assert_eq!(models, vec!["b/y".to_string(), "a/x".to_string()]);
        merge_default(&mut models, "c/z");
        assert_eq!(models[0], "c/z");
        let mut empty: Vec<String> = Vec::new();
        merge_default(&mut empty, "");
        assert!(empty.is_empty());
    }

    #[test]
    fn templates_fill_placeholders() {
        let _guard = TEMPLATE_HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prompt = commit_message_prompt("a.cs", "@@ -1 +1 @@", "tone: stuff");
        assert!(prompt.contains("Changed files: a.cs"));
        assert!(prompt.contains("@@ -1 +1 @@"));
        assert!(prompt.contains("tone: stuff"));
        // every wand ends with the no-tools guard: the run must answer from
        // the assembled input
        assert!(prompt.ends_with(NO_TOOLS_SUFFIX));

        let readme = readme_prompt("proj", "src/\n  main.rs", "");
        assert!(readme.contains("Project name: proj"));
        assert!(readme.contains("src/"));
        assert!(readme.ends_with(NO_TOOLS_SUFFIX));

        let summary = daily_summary_prompt("Oct 8", "- **repo**: things");
        assert!(summary.contains("Day: Oct 8"));
        assert!(summary.ends_with(NO_TOOLS_SUFFIX));
    }

    #[test]
    fn registry_lists_the_three_builder_templates() {
        assert_eq!(
            PROMPT_TEMPLATES.iter().map(|t| t.id).collect::<Vec<_>>(),
            ["commit-message", "readme", "daily-summary-all"]
        );
        // the registry's files are exactly the ones the builders load
        assert!(template_path("commit-message").is_some_and(|p| p.ends_with("commit-message.md")));
        assert!(template_path("readme").is_some_and(|p| p.ends_with("readme.md")));
        assert!(
            template_path("daily-summary-all").is_some_and(|p| p.ends_with("daily-summary-all.md"))
        );
        assert!(template_path("unknown").is_none());
        // every default documents its own placeholders
        for template in PROMPT_TEMPLATES {
            assert!(
                !template.placeholders.is_empty(),
                "{} lists placeholders",
                template.id
            );
            for placeholder in template.placeholders {
                assert!(
                    template.default.contains(placeholder),
                    "{} default misses {placeholder}",
                    template.id
                );
            }
        }
    }

    #[test]
    fn write_then_reset_round_trips_customization() {
        let _guard = TEMPLATE_HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().expect("scratch home");
        // SAFETY: process-global env, held only for this test's body; the
        // lock keeps the sibling builder test from reading this HOME
        let original = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", home.path()) };

        assert!(!is_customized("readme"), "nothing written yet");
        write_template("readme", "custom body {repo_name}").expect("write");
        assert_eq!(
            read_template("readme").as_deref(),
            Some("custom body {repo_name}")
        );
        assert!(is_customized("readme"));
        reset_template("readme").expect("reset");
        assert!(!is_customized("readme"));
        assert_eq!(
            read_template("readme").as_deref(),
            template_by_id("readme").map(|t| t.default)
        );
        // unknown ids never touch the disk
        assert!(write_template("unknown", "x").is_err());
        assert!(reset_template("unknown").is_err());
        assert!(!is_customized("unknown"));

        match original {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}
