//! The opencode wand runner: one-shot `opencode serve` runs for the AI
//! generators (commit message, README, daily summary). Attach-or-spawn with
//! the forced ask-permission config, one session per prompt, the answer read
//! from the /event SSE stream, permission asks forwarded to the caller (the
//! UI's allow/reject popup), and a hard cap so a stuck run dies.

use anyhow::{anyhow, Context as _, Result};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Forces ask-permission on every tool so the popup gates edits/bash/webfetch.
pub const ASK_CONFIG: &str = r#"{"$schema":"https://opencode.ai/config.json","permission":{"edit":"ask","bash":"ask","webfetch":"ask"}}"#;

/// Hard cap for one wand run.
pub const RUN_CAP: Duration = Duration::from_secs(150);

/// Server startup health deadline.
const HEALTH_DEADLINE: Duration = Duration::from_secs(25);

/// Events a run forwards to the UI while it works.
#[derive(Clone, Debug)]
pub enum WandEvent {
    /// A tool asked for permission; the run waits for a reply.
    Ask { id: String, title: String },
    Log(String),
}

/// Controls the UI sends into a running wand.
#[derive(Clone, Debug)]
pub enum WandControl {
    Respond { id: String, response: String },
    Kill,
}

/// One complete wand run: server up → session → prompt → SSE answer.
/// Returns the final assistant text. The `stop` channel kills the run early
/// (a cancelled run's server dies with it via kill_on_drop).
pub async fn run_prompt(
    executable: &str,
    folder: &Path,
    model: Option<&str>,
    prompt: &str,
    events: async_channel::Sender<WandEvent>,
    control: async_channel::Receiver<WandControl>,
) -> Result<String> {
    let server = start_server(executable, folder, &events).await?;
    let base_url = server.base_url.clone();

    let result = wand_session(&base_url, model, prompt, &events, control).await;

    let mut server = server;
    server.stop();
    result
}

async fn start_server(
    executable: &str,
    folder: &Path,
    events: &async_channel::Sender<WandEvent>,
) -> Result<ServerProcess> {
    let port = find_free_port()?;
    let base_url = format!("http://127.0.0.1:{port}");
    let health_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    if is_healthy(&health_client, &base_url).await {
        let _ = events.send_blocking(WandEvent::Log(format!("attached to opencode at {base_url}")));
        return Ok(ServerProcess { child: None, base_url });
    }

    let mut command = tokio::process::Command::new(executable);
    command
        .args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
        .current_dir(if folder.exists() { folder } else { Path::new(".") })
        .env("OPENCODE_CONFIG_CONTENT", ASK_CONFIG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    command.env_remove("ELECTRON_RUN_AS_NODE");
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut child = command
        .spawn()
        .with_context(|| format!("'{executable}' failed to start"))?;
    let _ = events.send_blocking(WandEvent::Log(format!(
        "spawned {executable} serve (pid {:?})",
        child.id()
    )));

    let deadline = std::time::Instant::now() + HEALTH_DEADLINE;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("opencode serve did not become healthy within 25 s"));
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(anyhow!("opencode serve exited with code {status}"));
        }
        if is_healthy(&health_client, &base_url).await {
            return Ok(ServerProcess { child: Some(child), base_url });
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

struct ServerProcess {
    child: Option<tokio::process::Child>,
    base_url: String,
}

impl ServerProcess {
    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        self.child = None;
    }
}

async fn is_healthy(client: &reqwest::Client, base_url: &str) -> bool {
    matches!(
        client.get(format!("{base_url}/global/health")).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

fn find_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn wand_session(
    base_url: &str,
    model: Option<&str>,
    prompt: &str,
    events: &async_channel::Sender<WandEvent>,
    control: async_channel::Receiver<WandControl>,
) -> Result<String> {
    let http = reqwest::Client::new();

    // one session per prompt
    let resp = http
        .post(format!("{base_url}/session"))
        .json(&json!({ "title": "devtools wand" }))
        .send()
        .await?;
    let session: Value = error_for_status(resp).await?.json().await?;
    let session_id = session
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("session id missing"))?
        .to_string();

    // fire-and-forget send; the answer arrives on the event stream
    let mut body = json!({
        "parts": [{ "type": "text", "text": prompt }],
    });
    if let Some(model) = model.filter(|m| !m.is_empty()) {
        body["modelID"] = json!(model);
    }
    let resp = http
        .post(format!("{base_url}/session/{session_id}/message"))
        .json(&body)
        .send()
        .await?;
    error_for_status(resp).await?;

    // the SSE stream: assistant text accumulates on message.part.updated,
    // permission asks pause for a reply, session.idle completes the run
    let resp = http.get(format!("{base_url}/event")).send().await?;
    let resp = error_for_status(resp).await?;

    use futures_util::StreamExt as _;
    let mut stream = resp.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut assistant_text = String::new();
    let started = std::time::Instant::now();

    loop {
        if started.elapsed() >= RUN_CAP {
            return Err(anyhow!("the wand run exceeded its time cap"));
        }
        let chunk = tokio::select! {
            biased;
            maybe_control = control.recv() => {
                match maybe_control {
                    Ok(WandControl::Kill) | Err(_) => return Err(anyhow!("cancelled")),
                    Ok(WandControl::Respond { id, response }) => {
                        let resp = http
                            .post(format!("{base_url}/permission/{id}/reply"))
                            .json(&json!({ "response": response }))
                            .send()
                            .await;
                        if let Ok(resp) = resp {
                            let _ = error_for_status(resp).await;
                        }
                        continue;
                    }
                }
            }
            chunk = stream.next() => match chunk {
                Some(Ok(bytes)) => bytes,
                Some(Err(err)) => return Err(err.into()),
                None => return Err(anyhow!("the event stream ended before an answer")),
            },
        };
        buffer.extend_from_slice(&chunk);

        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            let line = line.trim_end_matches('\r');
            let Some(payload) = line.strip_prefix("data:") else { continue };
            let payload = payload.trim();
            if payload.is_empty() {
                continue;
            }
            let Ok(evt) = serde_json::from_str::<Value>(payload) else { continue };
            let kind = evt.get("type").and_then(Value::as_str).unwrap_or_default();
            match kind {
                "message.part.updated" => {
                    // the part carries the full text-so-far each time
                    let part = evt.get("part").cloned().unwrap_or(Value::Null);
                    if part.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(text) = part.get("text").and_then(Value::as_str) {
                            assistant_text = text.to_string();
                        }
                }
                "permission.updated" => {
                    let status = evt.pointer("/permission/status").and_then(Value::as_str);
                    if status != Some("pending") {
                        continue;
                    }
                    let permission = evt.get("permission").cloned().unwrap_or(Value::Null);
                    let id = permission.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let title = permission
                        .pointer("/payload/title")
                        .or_else(|| permission.get("title"))
                        .and_then(Value::as_str)
                        .unwrap_or("permission requested")
                        .to_string();
                    if !id.is_empty() {
                        let _ = events.send_blocking(WandEvent::Ask { id, title });
                    }
                }
                "session.idle"
                    if !assistant_text.is_empty() => {
                        return Ok(assistant_text);
                    }
                _ => {}
            }
        }
    }
}

async fn error_for_status(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let detail: String = detail.chars().take(1000).collect();
        return Err(anyhow!("{status}{detail}"));
    }
    Ok(resp)
}

// ---- prompt templates (user file → seed-once → built-in) ----------------------

fn load_template(file_name: &str, default: &str) -> String {
    let path = crate::paths::AppPaths::opencode_dir().join(file_name);
    if let Ok(text) = std::fs::read_to_string(&path) {
        return text;
    }
    let _ = std::fs::create_dir_all(crate::paths::AppPaths::opencode_dir());
    if std::fs::write(&path, default).is_ok() {
        return default.to_string();
    }
    default.to_string()
}

/// The commit-message wand's filled prompt (Conventional Commits generator).
pub fn commit_message_prompt(file_list: &str, diff: &str, context: &str) -> String {
    const DEFAULT: &str = r#"You are a git commit message generator. Given a diff (and optional context), output ONE commit message following Conventional Commits.

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
"#;
    load_template("commit-message.md", DEFAULT)
        .replace("{file_list}", file_list)
        .replace("{diff}", diff)
        .replace("{context}", context)
}

/// The README wand's filled prompt.
pub fn readme_prompt(repo_name: &str, file_tree: &str, context: &str) -> String {
    const DEFAULT: &str = r###"You are a README.md generator. Given a repository's name and file tree (plus optional context), write a concise, professional README.md for the project.

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
"###;
    load_template("readme.md", DEFAULT)
        .replace("{repo_name}", repo_name)
        .replace("{file_tree}", file_tree)
        .replace("{context}", context)
}

/// The daily-summary wand's filled prompt.
pub fn daily_summary_prompt(date_label: &str, repositories_block: &str) -> String {
    const DEFAULT: &str = r##"You are a daily work-summary writer. Given several repositories' commits and still-uncommitted changes from a single day, write one short markdown report that tells the developer what they did that day across their projects and where each effort left off.

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
"##;
    load_template("daily-summary-all.md", DEFAULT)
        .replace("{date}", date_label)
        .replace("{repos}", repositories_block)
}

/// The README prompt's file tree: a recursive listing capped at
/// `max_lines`, skipping the noise directories.
pub fn file_tree(folder: &Path, max_lines: usize) -> String {
    fn walk(dir: &Path, prefix: &str, skip: bool, lines: &mut Vec<String>, max: usize) {
        if skip || lines.len() >= max {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
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
pub fn format_repositories_block(
    activities: &[(String, crate::model::GitDayActivity)],
) -> String {
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
                    format!(" — {} file(s), +{additions}/−{deletions}", commit.files.len())
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
                out.push_str(&format!("… and {} more commits\n", activity.commits.len() - MAX_COMMITS));
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
                working.push_str(&format!("- {}  {}{}\n", file.display_status_code(), file.path, counts));
                if working.len() > MAX_WORKING_CHARS {
                    working.push_str("… (truncated)\n");
                    break;
                }
            }
            if activity.uncommitted_files.len() > MAX_FILES {
                working.push_str(&format!("… and {} more files\n", activity.uncommitted_files.len() - MAX_FILES));
            }
            out.push_str(working.trim_end_matches('\n'));
            out.push('\n');
        }
        out.push('\n');
    }
    if activities.len() > MAX_REPOS {
        out.push_str(&format!("(… and {} more active repositories omitted)\n", activities.len() - MAX_REPOS));
    }
    out.trim_end().to_string()
}

// ---- model catalog (`opencode models` + TTL cache) -----------------------------

const MODEL_CLI_TIMEOUT: Duration = Duration::from_secs(15);

fn models_cache_path() -> std::path::PathBuf {
    crate::paths::AppPaths::opencode_dir().join("models.cache.json")
}

/// The cached catalog (no CLI spawn); the configured default lands on top
/// when it is missing from the list.
pub fn cached_models(default_model: &str) -> Vec<String> {
    let mut models: Vec<String> = std::fs::read_to_string(models_cache_path())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    merge_default(&mut models, default_model);
    models
}

/// Lists models via `opencode models` (one line per provider/model-id; the
/// '/' guard drops banners). Only a non-empty result is cached, so a
/// transient CLI failure never clobbers a good catalog.
pub async fn list_models(executable: &str, default_model: &str) -> Vec<String> {
    let argv = vec!["models".to_string()];
    let result = crate::process::run(crate::process::ProcessRunOptions {
        file_name: executable,
        args: &argv,
        timeout: Some(MODEL_CLI_TIMEOUT),
        strip_electron_variable: true,
        ..Default::default()
    })
    .await;
    let models: Vec<String> = match result {
        Ok(result) if result.succeeded() => result
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| line.contains('/'))
            .map(str::to_string)
            .collect(),
        Ok(result) => {
            if result.timed_out {
                tracing::warn!("'{executable} models' timed out after 15s");
            }
            Vec::new()
        }
        Err(err) => {
            tracing::warn!("'{executable} models' failed: {err}");
            Vec::new()
        }
    };

    if !models.is_empty() {
        let mut unique = models.clone();
        unique.dedup();
        let mut merged = unique.clone();
        merge_default(&mut merged, default_model);
        let path = models_cache_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&merged) {
            let _ = std::fs::write(&path, json);
        }
        return merged;
    }
    let mut cached = cached_models(default_model);
    merge_default(&mut cached, default_model);
    cached
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
        let prompt = commit_message_prompt("a.cs", "@@ -1 +1 @@", "tone: stuff");
        assert!(prompt.contains("Changed files: a.cs"));
        assert!(prompt.contains("@@ -1 +1 @@"));
        assert!(prompt.contains("tone: stuff"));

        let readme = readme_prompt("proj", "src/\n  main.rs", "");
        assert!(readme.contains("Project name: proj"));
        assert!(readme.contains("src/"));

        let summary = daily_summary_prompt("Oct 8", "- **repo**: things");
        assert!(summary.contains("Day: Oct 8"));
    }
}
