//! The GitHub activity service: per-repo `gh` queries run inside the repo
//! folder (gh resolves owner/name from the git remote). One `gh repo view`
//! proves the repo lives on GitHub and yields its link, then the open PR and
//! issue lists feed the column chips and the details tabs. Payloads are
//! parsed defensively from serde_json Values — gh's shapes drift.

use crate::process::{self, ProcessRunOptions};
use std::path::Path;
use std::time::Duration;

/// Upper bound for one gh invocation; a hung network call must not stall the pass.
const GH_TIMEOUT: Duration = Duration::from_secs(20);

/// Caps each list fetch (and therefore the chip counts).
const ITEM_LIMIT: usize = 50;

const PR_FIELDS: &str = "number,title,url,author,labels,isDraft,headRefName,baseRefName,reviewDecision,updatedAt";
const ISSUE_FIELDS: &str = "number,title,url,author,labels,updatedAt";

#[derive(Clone, Debug, Default)]
pub struct GitHubItem {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub updated_at: Option<String>,
    pub head_ref_name: Option<String>,
    pub base_ref_name: Option<String>,
    pub is_pull_request: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GitHubSnapshot {
    pub url: Option<String>,
    pub pulls: Vec<GitHubItem>,
    pub issues: Vec<GitHubItem>,
    /// False when `gh repo view` failed (not a GitHub repo, or gh/auth
    /// trouble) — the cell stays empty instead of showing a misleading OK.
    pub available: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GitHubRepoDetails {
    pub owner: Option<String>,
    pub created_at: Option<String>,
    pub primary_language: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub default_branch: Option<String>,
    pub url: Option<String>,
}

async fn run_gh(gh_exe: &str, folder: &Path, args: &[&str]) -> Option<String> {
    let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let result = process::run(ProcessRunOptions {
        file_name: gh_exe,
        args: &argv,
        working_dir: Some(folder),
        timeout: Some(GH_TIMEOUT),
        environment: vec![
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GH_PROMPT".into(), "disabled".into()),
        ],
        ..Default::default()
    })
    .await
    .ok()?;
    if result.succeeded() {
        Some(result.stdout)
    } else {
        None
    }
}

/// One repo's full GitHub probe. A missing gh binary returns an unavailable
/// snapshot (the pass gates on resolution before spawning anyway).
pub async fn refresh(folder: &Path, gh_exe: &str) -> GitHubSnapshot {
    let Some(repo_json) = run_gh(gh_exe, folder, &["repo", "view", "--json", "url"]).await else {
        return GitHubSnapshot::default();
    };
    let url = serde_json::from_str::<serde_json::Value>(&repo_json)
        .ok()
        .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(str::to_string));
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return GitHubSnapshot::default();
    };

    let limit = ITEM_LIMIT.to_string();
    let pr_args: Vec<&str> = vec!["pr", "list", "--json", PR_FIELDS, "--limit", &limit];
    let issue_args: Vec<&str> = vec!["issue", "list", "--json", ISSUE_FIELDS, "--limit", &limit];
    let (pr_json, issue_json) = tokio::join!(
        run_gh(gh_exe, folder, &pr_args),
        run_gh(gh_exe, folder, &issue_args)
    );
    GitHubSnapshot {
        url: Some(url),
        pulls: parse_items(pr_json.as_deref(), true),
        issues: parse_items(issue_json.as_deref(), false),
        available: true,
    }
}

/// The Overview sidebar's repo metadata (owner, language, license, topics…).
pub async fn repo_details(folder: &Path, gh_exe: &str) -> Option<GitHubRepoDetails> {
    let json = run_gh(
        gh_exe,
        folder,
        &["repo", "view", "--json", "owner,createdAt,primaryLanguage,licenseInfo,repositoryTopics,defaultBranchRef,url"],
    )
    .await?;
    let v = serde_json::from_str::<serde_json::Value>(&json).ok()?;
    Some(GitHubRepoDetails {
        owner: v.pointer("/owner/login").and_then(|x| x.as_str()).map(str::to_string),
        created_at: v.get("createdAt").and_then(|x| x.as_str()).map(str::to_string),
        primary_language: v.get("primaryLanguage").and_then(|x| x.as_str()).map(str::to_string),
        license: v.pointer("/licenseInfo/name").and_then(|x| x.as_str()).map(str::to_string),
        topics: v
            .get("repositoryTopics")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        default_branch: v.pointer("/defaultBranchRef/name").and_then(|x| x.as_str()).map(str::to_string),
        url: v.get("url").and_then(|x| x.as_str()).map(str::to_string),
    })
}

fn parse_items(json: Option<&str>, is_pull_request: bool) -> Vec<GitHubItem> {
    let Some(json) = json else { return Vec::new() };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let number = item.get("number").and_then(|n| n.as_u64())?;
            Some(GitHubItem {
                number,
                title: item.get("title").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
                url: item.get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                author: item.pointer("/author/login").and_then(|a| a.as_str()).map(str::to_string),
                labels: item
                    .get("labels")
                    .and_then(|l| l.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                is_draft: item.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
                review_decision: item.get("reviewDecision").and_then(|r| r.as_str()).map(str::to_string),
                updated_at: item.get("updatedAt").and_then(|u| u.as_str()).map(str::to_string),
                head_ref_name: item.get("headRefName").and_then(|b| b.as_str()).map(str::to_string),
                base_ref_name: item.get("baseRefName").and_then(|b| b.as_str()).map(str::to_string),
                is_pull_request,
            })
        })
        .collect()
}
