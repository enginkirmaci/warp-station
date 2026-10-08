//! The repos list projection: filter (search text, tag chip) + sort (mode,
//! favorites always floating to the top). Pure functions over the entry list,
//! so ordering rules are unit-testable without the store.

use devtools_core::repo_store::{RepoEntry, FAVORITES_TAG};
use devtools_core::settings::RepoSortMode;

pub fn project(
    repos: &[RepoEntry],
    search: &str,
    tag: Option<&str>,
    sort: RepoSortMode,
) -> Vec<usize> {
    let needle = search.trim().to_lowercase();
    let mut rows: Vec<usize> = repos
        .iter()
        .enumerate()
        .filter(|(_, repo)| {
            if !needle.is_empty()
                && !repo.name.to_lowercase().contains(&needle)
                && !repo.folder_path.to_string_lossy().to_lowercase().contains(&needle)
            {
                return false;
            }
            match tag {
                None => true,
                Some(t) if t.eq_ignore_ascii_case(FAVORITES_TAG) => repo.is_favorite(),
                Some(t) => repo.tags.iter().any(|rt| rt.eq_ignore_ascii_case(t)),
            }
        })
        .map(|(ix, _)| ix)
        .collect();

    rows.sort_by(|&a, &b| {
        let (ra, rb) = (&repos[a], &repos[b]);
        // favorites first; then the mode's order (Name ascending, the rest
        // descending); name breaks ties. Option's None < Some puts repos
        // without a commit date last in LastActivity.
        rb.is_favorite()
            .cmp(&ra.is_favorite())
            .then_with(|| match sort {
                RepoSortMode::Name => std::cmp::Ordering::Equal,
                RepoSortMode::LastActivity => rb.git_last_commit_at.cmp(&ra.git_last_commit_at),
                RepoSortMode::Changes => changes_weight(rb).cmp(&changes_weight(ra)),
                RepoSortMode::PullRequests => rb.github_pr_count.cmp(&ra.github_pr_count),
                RepoSortMode::Issues => rb.github_issue_count.cmp(&ra.github_issue_count),
            })
            .then_with(|| ra.name.to_lowercase().cmp(&rb.name.to_lowercase()))
    });
    rows
}

fn changes_weight(repo: &RepoEntry) -> u32 {
    repo.git_modified_count * 10 + repo.git_to_push_count * 5 + repo.git_to_pull_count
}

/// Every distinct tag across the list; favorites and platform always lead
/// (the .NET toolbar shows both unconditionally).
pub fn all_tags(repos: &[RepoEntry]) -> Vec<String> {
    let mut tags: Vec<String> = vec![FAVORITES_TAG.to_string(), devtools_core::scan::PLATFORM_TAG.to_string()];
    for repo in repos {
        for tag in &repo.tags {
            if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.clone());
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
        RepoEntry {
            name: name.into(),
            folder_path: PathBuf::from(path),
            solution_path: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: false,
            github_pr_count: 0,
            github_issue_count: 0,
        }
    }

    fn repos() -> Vec<RepoEntry> {
        vec![
            repo("alpha", "/x/alpha", &["favorites"]),
            repo("beta", "/x/beta", &["work"]),
            repo("gamma", "/x/gamma", &[]),
        ]
    }

    #[test]
    fn favorites_float_and_name_sorts() {
        let all = repos();
        let rows = project(&all, "", None, RepoSortMode::Name);
        let names: Vec<&str> = rows.iter().map(|&i| all[i].name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn search_matches_name_or_path() {
        let all = repos();
        let rows = project(&all, "/x/beta", None, RepoSortMode::Name);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "beta");

        let rows = project(&all, "AMM", None, RepoSortMode::Name);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "gamma");
    }

    #[test]
    fn tag_filter_favorites_and_user_tags() {
        let all = repos();
        let rows = project(&all, "", Some("favorites"), RepoSortMode::Name);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "alpha");

        let rows = project(&all, "", Some("work"), RepoSortMode::Name);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "beta");
    }

    #[test]
    fn all_tags_leads_with_favorites_then_platform() {
        let mut all = repos();
        all[1].tags.push("platform".into());
        let tags = all_tags(&all);
        assert_eq!(tags[0], "favorites");
        assert_eq!(tags[1], "platform");
        assert!(tags.contains(&"work".to_string()));
    }
}
